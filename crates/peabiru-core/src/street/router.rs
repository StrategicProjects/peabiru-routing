//! Street-only travel times (R5 `StreetRouter` + `LinkedPointSet.eval`).
//!
//! The search runs over directed-edge states: the time at the end of each
//! edge, so car turn costs can depend on the edge a vehicle arrives by.

use super::flags::*;
use super::labels::cms_to_ms;
use super::link::{find_split, Split};
use super::StreetNetwork;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Street mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Walk,
    Bicycle,
    Car,
}

/// Car turn costs in seconds (R5 `BasicTraversalTimeCalculator`, driving on
/// the right).
const STRAIGHT_ON: i32 = 0;
const RIGHT_TURN: i32 = 10;
const LEFT_TURN: i32 = 30;
const U_TURN: i32 = 90;

/// Routing parameters, with r5r's defaults.
#[derive(Debug, Clone)]
pub struct RouteParams {
    /// Walking speed, km/h.
    pub walk_speed_kmh: f64,
    /// Cycling speed, km/h.
    pub bike_speed_kmh: f64,
    /// Highest level of traffic stress a cyclist rides on (1-4).
    pub max_lts: i32,
    /// Trip limit in whole minutes.
    pub max_trip_duration_min: i32,
    /// Worker threads; 0 means all cores.
    pub n_threads: usize,
}

/// Invalid routing parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamError(pub String);

impl std::fmt::Display for ParamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParamError {}

impl RouteParams {
    /// Check the parameters: speeds must give at least 1 mm/s, the trip limit
    /// must be a positive number of minutes that fits in seconds.
    pub fn validate(&self) -> Result<(), ParamError> {
        for (name, v) in [
            ("walk_speed_kmh", self.walk_speed_kmh),
            ("bike_speed_kmh", self.bike_speed_kmh),
        ] {
            // (int)(speed m/s * 1000) is a divisor, so it must be >= 1
            if !v.is_finite() || ((v * 5.0 / 18.0) as f32 * 1000.0f32) < 1.0 || v > 1000.0 {
                return Err(ParamError(format!(
                    "{name} must be between 0.0036 and 1000 km/h, got {v}"
                )));
            }
        }
        if !(1..=1_000_000).contains(&self.max_trip_duration_min) {
            return Err(ParamError(format!(
                "max_trip_duration_min must be between 1 and 1000000, got {}",
                self.max_trip_duration_min
            )));
        }
        Ok(())
    }
}

impl Default for RouteParams {
    fn default() -> Self {
        RouteParams {
            walk_speed_kmh: 3.6,
            bike_speed_kmh: 12.0,
            max_lts: 2,
            max_trip_duration_min: 120,
            n_threads: 0,
        }
    }
}

/// Speeds as R5 holds them: km/h to m/s in double, then `(float)`.
struct Speeds {
    walk: f32,
    bike: f32,
    /// `(int)(walkSpeed * 1000)`, mm/s, the off-street speed.
    walk_mms: i32,
    /// `(int)(speed * 1000)` for walk/bike destination edges, mm/s.
    on_street_mms: i32,
}

impl Speeds {
    fn new(p: &RouteParams, mode: Mode) -> Self {
        let walk = (p.walk_speed_kmh * 5.0 / 18.0) as f32;
        let bike = (p.bike_speed_kmh * 5.0 / 18.0) as f32;
        let on = match mode {
            Mode::Walk => walk,
            Mode::Bicycle => bike,
            Mode::Car => 2.22f32, // R5's unused ProfileRequest.carSpeed default
        };
        Speeds {
            walk,
            bike,
            walk_mms: (walk * 1000.0f32) as i32,
            on_street_mms: (on * 1000.0f32) as i32,
        }
    }
}

/// Binary min-heap on time with exactly the sift rules of
/// `java.util.PriorityQueue`, so states with equal times leave the queue in
/// the same order as in R5. That order matters: among states carrying the
/// same turn-restriction progress on an edge, R5 keeps the first one created.
#[derive(Default)]
struct JavaHeap {
    a: Vec<(i32, u32, u32)>,
}

impl JavaHeap {
    fn clear(&mut self) {
        self.a.clear();
    }

    #[inline]
    fn offer(&mut self, x: (i32, u32, u32)) {
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

    #[inline]
    fn poll(&mut self) -> Option<(i32, u32, u32)> {
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

/// Restriction progress carried by a car state: `(restriction, position)`
/// sorted by restriction, where position 1 means "on the from edge".
type RMap = Vec<(u32, u32)>;

/// Heap tag for a state without restriction progress.
const CLEAN: u32 = u32::MAX;

/// Reusable per-thread search state.
struct Search {
    /// Best time at the end of each directed edge, over states without
    /// restriction progress.
    edge_time: Vec<i32>,
    /// Best time at each vertex (min over all incoming edge states).
    vertex_time: Vec<i32>,
    touched_edges: Vec<u32>,
    touched_vertices: Vec<u32>,
    /// States in the middle of turn restrictions: `(edge, progress)`. R5
    /// keeps them apart: they neither dominate nor are dominated by plain
    /// states, and the first state with a given progress on an edge wins.
    restricted: Vec<(u32, RMap)>,
    restricted_at: std::collections::HashMap<u32, Vec<u32>>,
    /// Queue of `(time, edge, restricted state id or CLEAN)`.
    heap: JavaHeap,
}

impl Search {
    fn new(net: &StreetNetwork) -> Self {
        Search {
            edge_time: vec![i32::MAX; net.n_edges()],
            vertex_time: vec![i32::MAX; net.n_vertices()],
            touched_edges: Vec::new(),
            touched_vertices: Vec::new(),
            restricted: Vec::new(),
            restricted_at: std::collections::HashMap::new(),
            heap: JavaHeap::default(),
        }
    }

    fn reset(&mut self) {
        for &e in &self.touched_edges {
            self.edge_time[e as usize] = i32::MAX;
        }
        for &v in &self.touched_vertices {
            self.vertex_time[v as usize] = i32::MAX;
        }
        self.touched_edges.clear();
        self.touched_vertices.clear();
        self.restricted.clear();
        self.restricted_at.clear();
        self.heap.clear();
    }

    #[inline]
    fn push(&mut self, t: i32, e: u32, tag: u32) {
        self.heap.offer((t, e, tag));
    }

    #[inline]
    fn pop(&mut self) -> Option<(i32, u32, u32)> {
        self.heap.poll()
    }

    #[inline]
    fn touch_vertex(&mut self, v: usize, t: i32) {
        if t < self.vertex_time[v] {
            if self.vertex_time[v] == i32::MAX {
                self.touched_vertices.push(v as u32);
            }
            self.vertex_time[v] = t;
        }
    }

    /// Record a state at the end of edge `e`; returns its heap tag, or `None`
    /// when an existing state dominates it (R5 `isDominated`).
    #[inline]
    fn add_state(&mut self, net: &StreetNetwork, e: usize, t: i32, map: RMap) -> Option<u32> {
        if map.is_empty() {
            if t >= self.edge_time[e] {
                return None; // ties keep the existing state
            }
            if self.edge_time[e] == i32::MAX {
                self.touched_edges.push(e as u32);
            }
            self.edge_time[e] = t;
            self.touch_vertex(net.edge_to(e), t);
            return Some(CLEAN);
        }
        let ids = self.restricted_at.entry(e as u32).or_default();
        if ids.iter().any(|&id| self.restricted[id as usize].1 == map) {
            return None;
        }
        let id = self.restricted.len() as u32;
        ids.push(id);
        self.restricted.push((e as u32, map));
        self.touch_vertex(net.edge_to(e), t);
        Some(id)
    }
}

/// Carry restriction progress from the current edge onto edge `f`
/// (R5 `canTurnFrom`); `None` when the turn is forbidden.
fn advance_restrictions(net: &StreetNetwork, map: &RMap, f: u32) -> Option<RMap> {
    let mut out = Vec::with_capacity(map.len());
    for &(r, pos) in map {
        let rest = &net.restrictions[r as usize];
        let p = (pos - 1) as usize;
        if p < rest.via_edges.len() {
            if f != rest.via_edges[p] {
                if rest.only {
                    return None;
                }
                // left a "no" restriction: forget it
            } else {
                out.push((r, pos + 1));
            }
        } else if f != rest.to_edge {
            if rest.only {
                return None;
            }
        } else if !rest.only {
            return None;
        }
        // reached the end of an "only" restriction: done with it
    }
    Some(out)
}

/// Start the restrictions whose from edge is `f` (R5 `startTurnRestriction`).
#[inline]
fn start_restrictions(net: &StreetNetwork, map: &mut RMap, f: u32) {
    if net.edge_flags[f as usize] & RESTRICTION_START == 0 {
        return;
    }
    for r in net.restrictions_starting_at(f) {
        match map.binary_search_by_key(&r, |&(k, _)| k) {
            Ok(i) => map[i].1 = 1,
            Err(i) => map.insert(i, (r, 1)),
        }
    }
}

/// Turn cost from edge `from` onto edge `to` for a car (R5 `turnTimeSeconds`).
#[inline]
fn car_turn_cost(net: &StreetNetwork, from: usize, to: usize) -> i32 {
    let mut out = net.out_angle(from);
    let inn = net.in_angle(to);
    if out < inn {
        out += 360;
    }
    let a = out - inn;
    if a < 27 {
        STRAIGHT_ON
    } else if a < 153 {
        LEFT_TURN
    } else if a < 207 {
        U_TURN
    } else if a < 333 {
        RIGHT_TURN
    } else {
        STRAIGHT_ON
    }
}

/// `(int) ceil(lengthM / speed)` with R5's double/float mix.
#[inline]
fn traversal_seconds(length_mm: i32, speed: f32) -> i32 {
    (f64::from(length_mm) / 1000.0 / f64::from(speed)).ceil() as i32
}

/// Run the street search from an origin split; fills `s.vertex_time`.
fn route(
    net: &StreetNetwork,
    origin: &Split,
    mode: Mode,
    sp: &Speeds,
    max_lts: i32,
    limit: i32,
    s: &mut Search,
) {
    s.reset();
    let fwd = origin.edge as usize;
    let bwd = fwd + 1;
    let off = origin.distance_to_edge_mm / sp.walk_mms;
    // Start states carry the split edge as back edge so turn costs (and turn
    // restrictions) apply when leaving it. Their times are not checked
    // against the limit.
    let start_speed = |e: usize| match mode {
        Mode::Walk => sp.walk,
        Mode::Bicycle => sp.bike,
        Mode::Car => cms_to_ms(net.edge_speed_cms[e]),
    };
    let t1 = ((origin.distance1_mm / 1000) as f32 / start_speed(fwd)) as i32 + off;
    let t0 = ((origin.distance0_mm / 1000) as f32 / start_speed(bwd)) as i32 + off;
    for (e, t) in [(bwd, t0), (fwd, t1)] {
        let mut map = RMap::new();
        if mode == Mode::Car {
            start_restrictions(net, &mut map, e as u32);
        }
        if let Some(tag) = s.add_state(net, e, t, map) {
            s.push(t, e as u32, tag);
        }
    }
    let mut map = RMap::new();
    while let Some((t, e, tag)) = s.pop() {
        let e = e as usize;
        map.clear();
        if tag == CLEAN {
            if t > s.edge_time[e] {
                continue; // dominated since it was queued
            }
        } else {
            map.extend_from_slice(&s.restricted[tag as usize].1);
        }
        let v = net.edge_to(e);
        for &f in net.out_edges(v) {
            let next_map = if mode != Mode::Car {
                RMap::new()
            } else {
                let mut m = if map.is_empty() {
                    RMap::new()
                } else {
                    match advance_restrictions(net, &map, f) {
                        Some(m) => m,
                        None => continue,
                    }
                };
                start_restrictions(net, &mut m, f);
                m
            };
            let f = f as usize;
            let flags = net.edge_flags[f];
            if flags & LINK != 0 && net.edge_flags[e] & LINK != 0 {
                continue; // two connectors in a row would be a shortcut
            }
            let len = net.pair_length_mm[f / 2];
            let step = match mode {
                Mode::Walk => {
                    if flags & ALLOWS_PEDESTRIAN == 0 {
                        continue;
                    }
                    traversal_seconds(len, sp.walk)
                }
                Mode::Bicycle => {
                    let mut walk_bike = flags & ALLOWS_BIKE == 0;
                    if max_lts > 0
                        && max_lts < 4
                        && (flags & BIKE_LTS_4 != 0
                            || (max_lts < 3 && flags & BIKE_LTS_3 != 0)
                            || (max_lts < 2 && flags & BIKE_LTS_2 != 0))
                    {
                        walk_bike = true;
                    }
                    if walk_bike {
                        if flags & ALLOWS_PEDESTRIAN == 0 {
                            continue;
                        }
                        traversal_seconds(len, sp.walk)
                    } else {
                        traversal_seconds(len, sp.bike)
                    }
                }
                Mode::Car => {
                    if flags & ALLOWS_CAR == 0 {
                        continue;
                    }
                    traversal_seconds(len, cms_to_ms(net.edge_speed_cms[f]))
                        + car_turn_cost(net, e, f)
                }
            };
            let mut t1 = t.saturating_add(step);
            if t1 == t {
                t1 += 1;
            }
            if t1 < limit {
                if let Some(tag) = s.add_state(net, f, t1, next_map) {
                    s.push(t1, f as u32, tag);
                }
            }
        }
    }
}

/// Time in seconds from the searched origin to a linked destination
/// (R5 `LinkedPointSet.eval`), or `None` when unreachable.
fn time_to_point(
    net: &StreetNetwork,
    origin: &Split,
    dest: &Split,
    mode: Mode,
    sp: &Speeds,
    s: &Search,
) -> Option<i64> {
    let on = match mode {
        Mode::Car => (cms_to_ms(net.edge_speed_cms[dest.edge as usize]) * 1000.0f32) as i32,
        _ => sp.on_street_mms,
    };
    let off = sp.walk_mms;
    if dest.edge == origin.edge {
        let along = (origin.distance0_mm - dest.distance0_mm).abs();
        return Some(i64::from(
            origin.distance_to_edge_mm / off + along / on + dest.distance_to_edge_mm / off,
        ));
    }
    let t0 = s.vertex_time[dest.vertex0 as usize];
    let t1 = s.vertex_time[dest.vertex1 as usize];
    if t0 == i32::MAX && t1 == i32::MAX {
        return None;
    }
    let off_t = i64::from(dest.distance_to_edge_mm / off);
    let via = |t: i32, d: i32| {
        if t == i32::MAX {
            i64::MAX
        } else {
            i64::from(t) + i64::from(d / on) + off_t
        }
    };
    Some(via(t0, dest.distance0_mm).min(via(t1, dest.distance1_mm)))
}

/// Seconds by street from one point to another with r5r's defaults, before
/// rounding to minutes; `None` if either point is unlinked or the
/// destination is unreachable. For tests and diagnostics.
pub fn street_seconds(
    net: &StreetNetwork,
    from: (f64, f64),
    to: (f64, f64),
    mode: Mode,
) -> Option<i64> {
    let params = RouteParams::default();
    let sp = Speeds::new(&params, mode);
    let mut scratch = Vec::new();
    let o = find_split(net, from.0, from.1, mode, &mut scratch)?;
    let d = find_split(net, to.0, to.1, mode, &mut scratch)?;
    let mut search = Search::new(net);
    route(
        net,
        &o,
        mode,
        &sp,
        params.max_lts,
        params.max_trip_duration_min * 60,
        &mut search,
    );
    time_to_point(net, &o, &d, mode, &sp, &search)
}

/// Travel-time matrix in whole minutes, row-major `origins x destinations`;
/// `None` where r5r reports no time (unlinked, unreachable or beyond
/// `max_trip_duration_min`). Points are `(lat, lon)` degrees.
pub fn travel_time_matrix(
    net: &StreetNetwork,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    mode: Mode,
    params: &RouteParams,
) -> Result<Vec<Option<u32>>, ParamError> {
    params.validate()?;
    let sp = Speeds::new(params, mode);
    let m = params.max_trip_duration_min;
    let limit = m * 60;
    let mut scratch = Vec::new();
    let dest_splits: Vec<Option<Split>> = destinations
        .iter()
        .map(|&(lat, lon)| find_split(net, lat, lon, mode, &mut scratch))
        .collect();
    let nd = destinations.len();
    let mut out = vec![None; origins.len() * nd];
    let n_threads = if params.n_threads > 0 {
        params.n_threads
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get())
    }
    .clamp(1, origins.len().max(1));

    let next = AtomicUsize::new(0);
    let chunks: Vec<Vec<(usize, Vec<Option<u32>>)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..n_threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut search = Search::new(net);
                    let mut scratch = Vec::new();
                    let mut rows = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= origins.len() {
                            break;
                        }
                        let (lat, lon) = origins[i];
                        let mut row = vec![None; nd];
                        if let Some(o) = find_split(net, lat, lon, mode, &mut scratch) {
                            route(net, &o, mode, &sp, params.max_lts, limit, &mut search);
                            for (j, d) in dest_splits.iter().enumerate() {
                                let Some(d) = d else { continue };
                                if let Some(secs) = time_to_point(net, &o, d, mode, &sp, &search) {
                                    let minutes = secs / 60;
                                    if minutes <= i64::from(m) {
                                        row[j] = Some(minutes as u32);
                                    }
                                }
                            }
                        }
                        rows.push((i, row));
                    }
                    rows
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("routing thread panicked"))
            .collect()
    });
    for (i, row) in chunks.into_iter().flatten() {
        out[i * nd..(i + 1) * nd].copy_from_slice(&row);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_parameters_are_rejected() {
        let net = StreetNetwork::default();
        let bad = [
            RouteParams {
                walk_speed_kmh: 0.0,
                ..RouteParams::default()
            },
            RouteParams {
                bike_speed_kmh: f64::NAN,
                ..RouteParams::default()
            },
            RouteParams {
                walk_speed_kmh: -3.6,
                ..RouteParams::default()
            },
            RouteParams {
                max_trip_duration_min: 0,
                ..RouteParams::default()
            },
            RouteParams {
                max_trip_duration_min: i32::MAX,
                ..RouteParams::default()
            },
        ];
        for p in bad {
            assert!(
                travel_time_matrix(&net, &[(0.0, 0.0)], &[(0.0, 0.0)], Mode::Walk, &p).is_err()
            );
        }
        assert!(RouteParams::default().validate().is_ok());
    }
}
