//! R5's `StreetRouter` with its states kept, for the street legs of r5r's
//! `detailed_itineraries()`.
//!
//! The travel-time searches of [`super::router`] keep one time per edge and
//! may pop equal times in any order, which gives the right times but not
//! R5's paths: which of two equally fast paths R5 reports depends on the
//! order in which `java.util.PriorityQueue` hands out equal keys, on the A*
//! heuristic toward a destination, on target pruning, and on the order of
//! the states R5 keeps per edge. This module follows `StreetRouter.route()`
//! (R5 7.6) step by step: a state arena with back pointers, the per-edge
//! state lists of `bestStatesAtEdge` in insertion order, the exact queue,
//! the `StopVisitor` of transit stop searches (at most 200 stops, none
//! closer than five minutes by bicycle or car), reverse searches (r5r's
//! `R5REgressRouter`), and `getState(Split)` / `getStateAtVertex` with their
//! tie rules.

use super::flags::*;
use super::labels::cms_to_ms;
use super::link::Split;
use super::router::{car_turn_cost, Mode};
use super::StreetNetwork;
use std::collections::HashMap;

/// No state (`backState == null`).
pub(crate) const NONE: u32 = u32::MAX;

/// R5 `PointToPointQuery.MAX_ACCESS_STOPS`.
pub(crate) const MAX_ACCESS_STOPS: usize = 200;

/// `SphericalDistanceLibrary.EARTH_CIRCUMFERENCE_METERS * 1000 / (360 *
/// VertexStore.FIXED_FACTOR)`.
fn mm_per_unit_lat_fixed() -> f64 {
    (40041.4385_f64 * 1000.0 * 1000.0) / (360.0 * 1e7)
}

/// One `StreetRouter.State`.
#[derive(Debug, Clone)]
pub(crate) struct St {
    pub(crate) vertex: u32,
    /// `backEdge`; negative for a state not made by traversing an edge.
    pub(crate) back_edge: i32,
    pub(crate) dur: i32,
    /// Millimetres.
    pub(crate) dist: i32,
    /// `streetMode`: the mode the last edge was traversed by (a bicycle
    /// search walks some edges).
    pub(crate) mode: Mode,
    pub(crate) back: u32,
    /// Index into the restriction maps, or `NONE` (`turnRestrictions == null`).
    tr: u32,
    heuristic: i32,
}

/// Restriction progress, `(restriction, position)` sorted by restriction
/// (R5's `TIntIntMap turnRestrictions`; its iteration order changes no
/// outcome).
type RMap = Vec<(u32, u32)>;

/// `java.util.PriorityQueue` with a comparator on an `int` key: the same
/// sift rules, so equal keys leave in R5's order.
#[derive(Default)]
struct JavaQueue {
    a: Vec<(i32, u32)>,
}

impl JavaQueue {
    fn clear(&mut self) {
        self.a.clear();
    }

    fn is_empty(&self) -> bool {
        self.a.is_empty()
    }

    fn offer(&mut self, x: (i32, u32)) {
        let mut k = self.a.len();
        self.a.push(x);
        while k > 0 {
            let parent = (k - 1) >> 1;
            let e = self.a[parent];
            if x.0 >= e.0 {
                break;
            }
            self.a[k] = e;
            k = parent;
        }
        self.a[k] = x;
    }

    fn poll(&mut self) -> Option<(i32, u32)> {
        let last = self.a.pop()?;
        if self.a.is_empty() {
            return Some(last);
        }
        let result = self.a[0];
        let n = self.a.len();
        let half = n >> 1;
        let mut k = 0;
        while k < half {
            let mut child = 2 * k + 1;
            let mut c = self.a[child];
            let right = child + 1;
            if right < n && c.0 > self.a[right].0 {
                child = right;
                c = self.a[child];
            }
            if last.0 <= c.0 {
                break;
            }
            self.a[k] = c;
            k = child;
        }
        self.a[k] = last;
        Some(result)
    }
}

/// Request settings a search reads (R5 `ProfileRequest`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Req {
    /// `walkSpeed`, `bikeSpeed` in m/s as `float`.
    pub(crate) walk: f32,
    pub(crate) bike: f32,
    /// `bikeTrafficStress` (r5r's `max_lts`).
    pub(crate) lts: i32,
}

impl Req {
    pub(crate) fn new(walk_speed_kmh: f64, bike_speed_kmh: f64, max_lts: i32) -> Self {
        Req {
            walk: (walk_speed_kmh * 5.0 / 18.0) as f32,
            bike: (bike_speed_kmh * 5.0 / 18.0) as f32,
            lts: max_lts,
        }
    }

    /// `getSpeedForMode` (car: R5's unused `carSpeed` default).
    fn speed(&self, mode: Mode) -> f32 {
        match mode {
            Mode::Walk => self.walk,
            Mode::Bicycle => self.bike,
            Mode::Car => 2.22f32,
        }
    }
}

/// A street search, as one `StreetRouter` instance.
pub(crate) struct Router<'a> {
    pub(crate) net: &'a StreetNetwork,
    pub(crate) mode: Mode,
    pub(crate) reverse: bool,
    pub(crate) req: Req,
    /// `timeLimitSeconds` (0: none).
    pub(crate) time_limit: i32,
    /// `distanceLimitMeters` (0: none).
    pub(crate) distance_limit_m: i32,
    /// Transit stop search: the stop of each vertex (`-1` for none) and the
    /// `StopVisitor`'s minimum time.
    stop_search: Option<(&'a [i32], i32)>,
    /// `StopVisitor.stops`: stop -> seconds.
    pub(crate) stops: HashMap<u32, i32>,
    pub(crate) states: Vec<St>,
    maps: Vec<RMap>,
    /// `bestStatesAtEdge`: per edge, its live states in insertion order.
    at_edge: HashMap<u32, Vec<u32>>,
    queue: JavaQueue,
    dest: Option<Split>,
    best_at_dest: i32,
    pub(crate) origin_split: Option<Split>,
    max_abs_origin_lat: i32,
    mm_per_lon_fixed: f64,
    max_speed_s_per_mm: f64,
    /// Restrictions by their last edge, for reverse searches (indices into
    /// `reverse_restrictions`).
    reverse_starts: Vec<(u32, u32)>,
    /// Set when the search met something R5 does not reproduce (see
    /// [`Router::unsupported`]).
    pub(crate) unsupported: Option<&'static str>,
    /// With `Some`, the via edges of a turn restriction with two or more
    /// of them may be in either order: R5 reverses that array in place,
    /// in the network every search shares, on each check of a reverse car
    /// search (`TurnRestriction.reverse` in `canTurnFrom`), so the order a
    /// check sees depends on every earlier check, in this call or an
    /// earlier one, on any thread. A check whose outcome depends on the
    /// order is "ambiguous"; the `i`-th one met takes the order
    /// `choices[i]` (`true`: reversed), or, past the end, reversed in a
    /// reverse search and as built in a forward one. The orders taken are
    /// recorded in `via_used`. With `None`, the order is the one built (a
    /// fresh network), and a reverse search that checks such a
    /// restriction sets `unsupported`.
    pub(crate) via_choices: Option<Vec<bool>>,
    pub(crate) via_used: Vec<bool>,
}

impl<'a> Router<'a> {
    pub(crate) fn new(net: &'a StreetNetwork, mode: Mode, req: Req, reverse: bool) -> Self {
        let mut reverse_starts = Vec::new();
        if reverse && mode == Mode::Car {
            reverse_starts = net
                .reverse_restrictions
                .iter()
                .enumerate()
                .map(|(i, r)| (r.to_edge, i as u32))
                .collect();
            reverse_starts.sort_unstable();
        }
        Router {
            net,
            mode,
            reverse,
            req,
            time_limit: 0,
            distance_limit_m: 0,
            stop_search: None,
            stops: HashMap::new(),
            states: Vec::new(),
            maps: Vec::new(),
            at_edge: HashMap::new(),
            queue: JavaQueue::default(),
            dest: None,
            best_at_dest: i32::MAX,
            origin_split: None,
            max_abs_origin_lat: i32::MIN,
            mm_per_lon_fixed: 0.0,
            max_speed_s_per_mm: 0.0,
            reverse_starts,
            unsupported: None,
            via_choices: None,
            via_used: Vec::new(),
        }
    }

    /// Make this a transit stop search (`transitStopSearch = true`):
    /// `vertex_stop[v]` is the stop at vertex `v`, or -1.
    pub(crate) fn stop_search(&mut self, vertex_stop: &'a [i32]) {
        // ProfileRequest.getMinTimeSeconds: minBikeTime, minCarTime = 5
        let min = match self.mode {
            Mode::Walk => 0,
            Mode::Bicycle | Mode::Car => 300,
        };
        self.stop_search = Some((vertex_stop, min));
    }

    pub(crate) fn set_destination(&mut self, split: Split) {
        self.dest = Some(split);
    }

    fn speed_on(&self, e: usize, mode: Mode) -> f32 {
        match mode {
            Mode::Car => cms_to_ms(self.net.edge_speed_cms[e]),
            m => self.req.speed(m),
        }
    }

    fn push_state(&mut self, s: St) -> u32 {
        self.states.push(s);
        (self.states.len() - 1) as u32
    }

    fn put_at_edge(&mut self, e: u32, id: u32) {
        self.at_edge.entry(e).or_default().push(id);
    }

    fn key(&self, id: u32) -> i32 {
        let s = &self.states[id as usize];
        s.dur.wrapping_add(s.heuristic)
    }

    /// `setOrigin(lat, lon)` once the split is found: the two start states,
    /// queued and recorded, without the limit check.
    pub(crate) fn set_origin_split(&mut self, split: Split) {
        self.origin_split = Some(split);
        let fwd = split.edge as usize;
        let off = split.distance_to_edge_mm / ((self.req.walk * 1000.0f32) as i32);
        let sp1 = self.speed_on(fwd, self.mode);
        let d1 = ((split.distance1_mm / 1000) as f32 / sp1) as i32 + off;
        let sp0 = self.speed_on(fwd + 1, self.mode);
        let d0 = ((split.distance0_mm / 1000) as f32 / sp0) as i32 + off;
        let (mut v0, mut v1) = (split.vertex0, split.vertex1);
        if self.reverse {
            std::mem::swap(&mut v0, &mut v1);
        }
        let mk = |vertex, edge: usize, dur, dist| St {
            vertex,
            back_edge: edge as i32,
            dur,
            dist,
            mode: self.mode,
            back: NONE,
            tr: NONE,
            heuristic: 0,
        };
        let mut s0 = mk(
            v0,
            fwd + 1,
            d0,
            split.distance0_mm + split.distance_to_edge_mm,
        );
        let mut s1 = mk(v1, fwd, d1, split.distance1_mm + split.distance_to_edge_mm);
        self.start_turn_restriction(self.mode, &mut s0);
        self.start_turn_restriction(self.mode, &mut s1);
        let a = self.push_state(s0);
        let b = self.push_state(s1);
        self.queue.offer((self.key(a), a));
        self.queue.offer((self.key(b), b));
        self.put_at_edge(fwd as u32 + 1, a);
        self.put_at_edge(fwd as u32, b);
        self.max_abs_origin_lat = split.fixed_lat;
    }

    /// `setOrigin(int fromVertex)`: one state with no back edge, queued but
    /// not recorded.
    pub(crate) fn set_origin_vertex(&mut self, v: u32) {
        self.max_abs_origin_lat = self.net.v_lat[v as usize];
        let id = self.push_state(St {
            vertex: v,
            back_edge: -1,
            dur: 0,
            dist: 0,
            mode: self.mode,
            back: NONE,
            tr: NONE,
            heuristic: 0,
        });
        self.queue.offer((self.key(id), id));
    }

    /// r5r's `R5REgressRouter.setOrigin`: a reverse search from a point,
    /// seeded with the partial edges to the split and the walk off the
    /// street, as the costs RAPTOR gets. False when no seed survives.
    pub(crate) fn set_egress_origin(&mut self, split: Split) -> bool {
        debug_assert!(self.reverse);
        self.origin_split = Some(split);
        self.max_abs_origin_lat = split.fixed_lat;
        self.seed(
            split.vertex0,
            split.edge,
            split.distance0_mm,
            split.distance_to_edge_mm,
        );
        self.seed(
            split.vertex1,
            split.edge + 1,
            split.distance1_mm,
            split.distance_to_edge_mm,
        );
        !self.queue.is_empty()
    }

    fn seed(&mut self, vertex: u32, edge: u32, partial_mm: i32, off_mm: i32) {
        let e = edge as usize;
        let probe_start = St {
            vertex: self.net.edge_to(e) as u32,
            back_edge: -1,
            dur: 0,
            dist: 0,
            mode: self.mode,
            back: NONE,
            tr: NONE,
            heuristic: 0,
        };
        let mut probe = self.traverse_with(&probe_start, NONE, e, self.req);
        if probe.is_none() && self.mode == Mode::Bicycle && self.req.lts > 0 && self.req.lts < 4 {
            let mut relaxed = self.req;
            relaxed.lts = 4;
            probe = self.traverse_with(&probe_start, NONE, e, relaxed);
        }
        let Some(probe) = probe else { return };
        let len = self.net.pair_length_mm[e / 2];
        let partial_s = if partial_mm == 0 {
            0
        } else {
            (f64::from(probe.dur) * (f64::from(partial_mm) / f64::from(len))) as i32
        };
        let off_s = off_mm / ((1000.0f32 * self.req.walk) as i32);
        let dur = (i64::from(partial_s) + i64::from(off_s)) as i32;
        if self.time_limit > 0 && dur >= self.time_limit {
            return;
        }
        let mut st = St {
            vertex,
            back_edge: edge as i32,
            dur,
            dist: partial_mm + off_mm,
            mode: probe.mode,
            back: NONE,
            tr: NONE,
            heuristic: 0,
        };
        self.start_turn_restriction(probe.mode, &mut st);
        let id = self.push_state(st);
        self.queue.offer((self.key(id), id));
        self.put_at_edge(edge, id);
    }

    /// `EdgeStore.startTurnRestriction(streetMode, reverseSearch, s1)`:
    /// `prev_mode` is the mode of the state left.
    fn start_turn_restriction(&mut self, prev_mode: Mode, s: &mut St) {
        if prev_mode != Mode::Car || s.back_edge < 0 {
            return;
        }
        let e = s.back_edge as u32;
        let starting: Vec<u32> = if self.reverse {
            let lo = self.reverse_starts.partition_point(|&(f, _)| f < e);
            self.reverse_starts[lo..]
                .iter()
                .take_while(|&&(f, _)| f == e)
                .map(|&(_, r)| r)
                .collect()
        } else {
            if self.net.edge_flags[e as usize] & RESTRICTION_START == 0 {
                return;
            }
            self.net.restrictions_starting_at(e).collect()
        };
        if starting.is_empty() {
            return;
        }
        let mut map = if s.tr == NONE {
            RMap::new()
        } else {
            self.maps[s.tr as usize].clone()
        };
        for r in starting {
            match map.binary_search_by_key(&r, |&(k, _)| k) {
                Ok(i) => map[i].1 = 1,
                Err(i) => map.insert(i, (r, 1)),
            }
        }
        self.maps.push(map);
        s.tr = (self.maps.len() - 1) as u32;
    }

    /// `canTurnFrom(s0, s1)` for a step onto `edge`: `None` when forbidden,
    /// else the restriction progress the new state carries (`None` inside
    /// for no map).
    fn can_turn_from(&mut self, s0: &St, edge: u32) -> Option<Option<RMap>> {
        if s0.tr == NONE || s0.mode != Mode::Car {
            return Some(None);
        }
        let mut out = RMap::new();
        for &(r, pos) in &self.maps[s0.tr as usize] {
            let rest = if self.reverse {
                &self.net.reverse_restrictions[r as usize]
            } else {
                &self.net.restrictions[r as usize]
            };
            let via = &rest.via_edges;
            let k = via.len();
            if self.reverse && k > 1 && self.via_choices.is_none() {
                // R5 reverses the restriction's via edges in place on every
                // such check (TurnRestriction.reverse), changing the
                // network for every later search: see `via_choices`
                self.unsupported = Some(
                    "a car search backwards through a turn restriction with two or more via ways",
                );
            }
            let to_edge = if self.reverse {
                rest.from_edge
            } else {
                rest.to_edge
            };
            let p = (pos - 1) as usize;
            if p < k {
                // the via edge R5 compares with: in the order built, or in
                // the reversed order the array may be left in
                let (built, reversed) = (via[p], via[k - 1 - p]);
                let mut expected = built;
                if let Some(choices) = &self.via_choices {
                    if built != reversed && ((edge == built) != (edge == reversed)) {
                        let i = self.via_used.len();
                        let rev = choices.get(i).copied().unwrap_or(self.reverse);
                        self.via_used.push(rev);
                        if rev {
                            expected = reversed;
                        }
                    }
                }
                if edge != expected {
                    if rest.only {
                        return None;
                    }
                } else {
                    out.push((r, pos + 1));
                }
            } else if to_edge != edge {
                if rest.only {
                    return None;
                }
            } else if !rest.only {
                return None;
            }
        }
        Some(Some(out))
    }

    /// `Edge.traverse(s0, streetMode, req, timeCalculator)` from state
    /// `s0` (id `s0_id`, or `NONE` for a probe) onto directed edge `e`.
    fn traverse_with(&mut self, s0: &St, s0_id: u32, e: usize, req: Req) -> Option<St> {
        let net = self.net;
        let vertex = if self.reverse {
            net.edge_from(e)
        } else {
            net.edge_to(e)
        } as u32;
        let tr = self.can_turn_from(s0, e as u32)?;
        let mut s1 = St {
            vertex,
            back_edge: e as i32,
            dur: s0.dur,
            dist: s0.dist,
            mode: self.mode,
            back: s0_id,
            tr: NONE,
            heuristic: 0,
        };
        // the progress carried on (an empty map is dropped), then the
        // restrictions starting on this edge
        if let Some(m) = tr {
            if !m.is_empty() {
                self.maps.push(m);
                s1.tr = (self.maps.len() - 1) as u32;
            }
        }
        self.start_turn_restriction(s0.mode, &mut s1);
        let flags = net.edge_flags[e];
        if s0.back_edge >= 0
            && s0.back != NONE
            && flags & LINK != 0
            && net.edge_flags[s0.back_edge as usize] & LINK != 0
        {
            return None;
        }
        let mut mode = self.mode;
        match mode {
            Mode::Walk => {
                if flags & ALLOWS_PEDESTRIAN == 0 {
                    return None;
                }
            }
            Mode::Bicycle => {
                let mut walk = flags & ALLOWS_BIKE == 0;
                if req.lts > 0 && req.lts < 4 {
                    if flags & BIKE_LTS_4 != 0 {
                        walk = true;
                    }
                    if req.lts < 3 && flags & BIKE_LTS_3 != 0 {
                        walk = true;
                    }
                    if req.lts < 2 && flags & BIKE_LTS_2 != 0 {
                        walk = true;
                    }
                }
                if walk {
                    if flags & ALLOWS_PEDESTRIAN == 0 {
                        return None;
                    }
                    mode = Mode::Walk;
                }
            }
            Mode::Car => {
                if flags & ALLOWS_CAR == 0 {
                    return None;
                }
            }
        }
        s1.mode = mode;
        let len = net.pair_length_mm[e / 2];
        let speed = match mode {
            Mode::Car => cms_to_ms(net.edge_speed_cms[e]),
            m => req.speed(m),
        };
        let trav = (f64::from(len) / 1000.0 / f64::from(speed)).ceil() as i32;
        let turn = if s0.back_edge >= 0 && mode == Mode::Car {
            let (from, to) = if self.reverse {
                (e, s0.back_edge as usize)
            } else {
                (s0.back_edge as usize, e)
            };
            car_turn_cost(net.out_angle(from), net.in_angle(to))
        } else {
            0
        };
        s1.dur = s1.dur.wrapping_add(trav + turn);
        s1.dist = s1.dist.wrapping_add(len);
        if s1.dur == s0.dur {
            s1.dur += 1;
        }
        if s1.dist == s0.dist {
            s1.dist += 1;
        }
        Some(s1)
    }

    /// `dominates(s1, s2)`.
    fn dominates(&self, a: &St, b: &St) -> bool {
        if a.tr == NONE && b.tr == NONE {
            return a.dur <= b.dur;
        }
        if a.tr != NONE && b.tr != NONE {
            return self.maps[a.tr as usize] == self.maps[b.tr as usize];
        }
        false
    }

    /// `isDominated(newState)`: drops the existing states it dominates.
    fn is_dominated(&mut self, s: &St) -> bool {
        let e = s.back_edge as u32;
        let Some(list) = self.at_edge.get(&e) else {
            return false;
        };
        let mut keep = Vec::with_capacity(list.len());
        let mut dominated = false;
        for (i, &id) in list.iter().enumerate() {
            let ex = &self.states[id as usize];
            if self.dominates(ex, s) {
                dominated = true;
                keep.extend_from_slice(&list[i..]);
                break;
            } else if !self.dominates(s, ex) {
                keep.push(id);
            }
        }
        let list = self.at_edge.get_mut(&e).expect("present");
        *list = keep;
        dominated
    }

    fn calc_heuristic(&self, s: &St) -> i32 {
        let Some(d) = self.dest else { return 0 };
        let v = s.vertex as usize;
        let dlat = d.fixed_lat - self.net.v_lat[v];
        let dlon = d.fixed_lon - self.net.v_lon[v];
        let x = self.mm_per_lon_fixed * f64::from(dlon);
        let y = mm_per_unit_lat_fixed() * f64::from(dlat);
        let dist = (x * x + y * y).sqrt();
        (dist * self.max_speed_s_per_mm) as i32
    }

    /// `StreetRouter.route()`.
    pub(crate) fn route(&mut self) {
        if let Some(d) = self.dest {
            let max_abs = d
                .fixed_lat
                .abs()
                .max(self.max_abs_origin_lat.wrapping_abs());
            let rad = (f64::from(max_abs) / 1e7).to_radians();
            self.mm_per_lon_fixed = mm_per_unit_lat_fixed() * rad.cos();
            let mut speed = f64::from(self.req.speed(self.mode));
            if speed == 0.0 {
                speed = 36.11;
            }
            self.max_speed_s_per_mm = 1.0 / (speed * 1000.0);
        }
        let dist_limit_mm = if self.distance_limit_m > 0 {
            self.distance_limit_m.wrapping_mul(1000)
        } else {
            i32::MAX
        };
        let time_limit = if self.time_limit > 0 {
            self.time_limit
        } else {
            i32::MAX
        };
        let net = self.net;
        while let Some((_, id)) = self.queue.poll() {
            let s0 = self.states[id as usize].clone();
            if s0.back_edge >= 0
                && !self
                    .at_edge
                    .get(&(s0.back_edge as u32))
                    .is_some_and(|l| l.contains(&id))
            {
                continue;
            }
            if s0.dur > self.best_at_dest {
                break;
            }
            if let Some((vertex_stop, min_time)) = self.stop_search {
                let stop = vertex_stop[s0.vertex as usize];
                if stop >= 0 && s0.dur >= min_time {
                    let e = self.stops.entry(stop as u32).or_insert(i32::MAX);
                    if *e > s0.dur {
                        *e = s0.dur;
                    }
                }
                if self.stops.len() >= MAX_ACCESS_STOPS {
                    self.queue.clear();
                    break;
                }
            }
            if let Some(d) = self.dest {
                if s0.vertex == d.vertex0 || s0.vertex == d.vertex1 {
                    if let Some(at) = self.state_at_split(&d) {
                        if self.best_at_dest > at.dur {
                            self.best_at_dest = at.dur;
                        }
                    }
                }
            }
            let v = s0.vertex as usize;
            let edges = if self.reverse {
                net.in_edges(v)
            } else {
                net.out_edges(v)
            };
            for &e in edges {
                let Some(mut s1) = self.traverse_with(&s0, id, e as usize, self.req) else {
                    continue;
                };
                if s1.dist <= dist_limit_mm && s1.dur < time_limit && !self.is_dominated(&s1) {
                    s1.heuristic = self.calc_heuristic(&s1);
                    let nid = self.push_state(s1);
                    self.put_at_edge(e, nid);
                    self.queue.offer((self.key(nid), nid));
                }
            }
        }
    }

    /// The best state at the end of edge `e` (`getStateAtEdge`: the least
    /// time, ties to the later state).
    fn state_at_edge(&self, e: u32) -> Option<u32> {
        let list = self.at_edge.get(&e)?;
        let mut best: Option<u32> = None;
        for &id in list {
            best = match best {
                Some(b) if self.states[b as usize].dur < self.states[id as usize].dur => Some(b),
                _ => Some(id),
            };
        }
        best
    }

    /// `getStateAtVertex(v)`: over the edges into `v` (out of it in a
    /// reverse search), the least time, ties to the first edge.
    pub(crate) fn state_at_vertex(&self, v: u32) -> Option<u32> {
        let net = self.net;
        let edges = if self.reverse {
            net.out_edges(v as usize)
        } else {
            net.in_edges(v as usize)
        };
        let mut ret: Option<u32> = None;
        for &e in edges {
            let Some(id) = self.state_at_edge(e) else {
                continue;
            };
            ret = match ret {
                Some(r) if self.states[r as usize].dur <= self.states[id as usize].dur => Some(r),
                _ => Some(id),
            };
        }
        ret
    }

    /// The candidate states of `getState(split)` (forward search), in R5's
    /// order: from the edges into `vertex0` onto the split edge, then from
    /// the edges into `vertex1` onto its reverse.
    fn split_candidates(&mut self, split: &Split) -> Vec<St> {
        let net = self.net;
        let mut out = Vec::new();
        for (edge, from_v, dist) in [
            (split.edge, split.vertex0, split.distance0_mm),
            (split.edge + 1, split.vertex1, split.distance1_mm),
        ] {
            for &inc in net.in_edges(from_v as usize) {
                let Some(list) = self.at_edge.get(&inc).cloned() else {
                    continue;
                };
                for id in list {
                    let s = self.states[id as usize].clone();
                    if self.can_turn_from(&s, edge).is_none() {
                        continue;
                    }
                    let turn = if s.mode == Mode::Car && s.back_edge >= 0 {
                        car_turn_cost(
                            net.out_angle(s.back_edge as usize),
                            net.in_angle(edge as usize),
                        )
                    } else {
                        0
                    };
                    let speed = self.speed_on(edge as usize, s.mode);
                    let trav = (f64::from(dist) / 1000.0 / f64::from(speed) + 0.5).floor() as i32;
                    out.push(St {
                        vertex: u32::MAX,
                        back_edge: edge as i32,
                        dur: s.dur.wrapping_add(turn + trav),
                        dist: s.dist.wrapping_add(dist),
                        mode: s.mode,
                        back: id,
                        tr: NONE,
                        heuristic: 0,
                    });
                }
            }
        }
        out
    }

    /// `getState(split)` without keeping the state.
    fn state_at_split(&mut self, split: &Split) -> Option<St> {
        let mut best: Option<St> = None;
        for c in self.split_candidates(split) {
            best = match best {
                Some(b) if b.dur < c.dur => Some(b),
                _ => Some(c),
            };
        }
        best
    }

    /// `getState(split)` after the search, kept in the arena: the id of
    /// the state at the destination, or `None` when unreachable.
    pub(crate) fn final_state(&mut self, split: &Split) -> Option<u32> {
        let s = self.state_at_split(split)?;
        Some(self.push_state(s))
    }

    /// The states of the path ending at `id`, first to last.
    pub(crate) fn path(&self, id: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cur = id;
        while cur != NONE {
            out.push(cur);
            cur = self.states[cur as usize].back;
        }
        out.reverse();
        out
    }

    /// The stops a stop search reached, by seconds.
    pub(crate) fn reached_stops(&self) -> &HashMap<u32, i32> {
        &self.stops
    }
}
