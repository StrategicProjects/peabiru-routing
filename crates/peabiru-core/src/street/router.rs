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
#[derive(Debug, Clone, Copy)]
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

/// Monotone integer priority queue (a radix heap) for searches whose
/// states carry no turn-restriction progress.
///
/// Such a search keeps, per directed edge, the least time of any state
/// reaching it, and every step adds a non-negative cost: the times it ends
/// with are shortest-path times, whatever order equal-time states leave the
/// queue in. Only the restricted car search depends on that order (see
/// [`JavaHeap`]), so only it needs the exact `java.util.PriorityQueue`
/// replica; every other search uses this cheaper queue and finds the same
/// times.
///
/// Keys must not decrease below the last one popped (true for a search
/// with non-negative steps). Keys are `i32` mapped to `u32` in order.
struct RadixHeap {
    /// Image of the last popped key; bucket `i > 0` holds keys whose highest
    /// bit differing from it is bit `i - 1`, bucket 0 keys equal to it.
    last: u32,
    len: usize,
    buckets: [Vec<(u32, u32)>; 33],
}

impl Default for RadixHeap {
    fn default() -> Self {
        RadixHeap {
            last: 0,
            len: 0,
            buckets: std::array::from_fn(|_| Vec::new()),
        }
    }
}

impl RadixHeap {
    #[inline]
    fn key(t: i32) -> u32 {
        (t as u32) ^ 0x8000_0000
    }

    #[inline]
    fn bucket(&self, k: u32) -> usize {
        (32 - (k ^ self.last).leading_zeros()) as usize
    }

    fn clear(&mut self) {
        for b in &mut self.buckets {
            b.clear();
        }
        self.last = 0;
        self.len = 0;
    }

    #[inline]
    fn push(&mut self, t: i32, e: u32) {
        // A key below the last popped one cannot come from a search with
        // non-negative steps; clamping keeps the queue consistent anyway.
        let k = Self::key(t).max(self.last);
        let b = self.bucket(k);
        self.buckets[b].push((k, e));
        self.len += 1;
    }

    #[inline]
    fn pop(&mut self) -> Option<(i32, u32)> {
        if self.len == 0 {
            return None;
        }
        if self.buckets[0].is_empty() {
            let i = (1..33)
                .find(|&i| !self.buckets[i].is_empty())
                .expect("a non-empty bucket");
            let mut moved = std::mem::take(&mut self.buckets[i]);
            self.last = moved.iter().map(|&(k, _)| k).min().expect("non-empty");
            for &(k, e) in &moved {
                let b = self.bucket(k);
                self.buckets[b].push((k, e));
            }
            moved.clear();
            self.buckets[i] = moved; // keep the allocation
        }
        let (k, e) = self.buckets[0].pop().expect("bucket 0 is non-empty");
        self.len -= 1;
        Some(((k ^ 0x8000_0000) as i32, e))
    }
}

/// Restriction progress carried by a car state: `(restriction, position)`
/// sorted by restriction, where position 1 means "on the from edge".
type RMap = Vec<(u32, u32)>;

/// Heap tag for a state without restriction progress.
const CLEAN: u32 = u32::MAX;

/// An outgoing edge as one mode's search sees it.
#[derive(Debug, Clone, Copy)]
struct Arc {
    edge: u32,
    to: u32,
    /// Traversal seconds for the mode (before the car turn cost); negative
    /// when the mode cannot use the edge.
    step: i32,
    /// Car: bearing entering the edge, whole degrees (R5 `getInAngle`).
    in_deg: i16,
    link: bool,
}

/// The street graph as one mode's search reads it: every vertex's outgoing
/// edges with their traversal times precomputed and laid out contiguously
/// (`arcs[out_offsets[v]..out_offsets[v + 1]]`, in `out_edges` order), so
/// the inner loop of the search reads one array. Built once per query.
pub(crate) struct ModeGraph {
    mode: Mode,
    sp: Speeds,
    arcs: Vec<Arc>,
    /// The search must pop equal times in `java.util.PriorityQueue` order:
    /// car searches on a network with turn restrictions.
    java_order: bool,
}

impl ModeGraph {
    fn new(net: &StreetNetwork, mode: Mode, sp: Speeds, max_lts: i32) -> Self {
        let step_of = |f: usize| -> i32 {
            let flags = net.edge_flags[f];
            let len = net.pair_length_mm[f / 2];
            match mode {
                Mode::Walk => {
                    if flags & ALLOWS_PEDESTRIAN == 0 {
                        return -1;
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
                            return -1;
                        }
                        traversal_seconds(len, sp.walk)
                    } else {
                        traversal_seconds(len, sp.bike)
                    }
                }
                Mode::Car => {
                    if flags & ALLOWS_CAR == 0 {
                        return -1;
                    }
                    traversal_seconds(len, cms_to_ms(net.edge_speed_cms[f]))
                }
            }
        };
        let arcs = net
            .out_edges
            .iter()
            .map(|&f| {
                let fu = f as usize;
                Arc {
                    edge: f,
                    to: net.edge_to(fu) as u32,
                    // a step never comes out negative for a usable edge
                    // (lengths >= 0, speeds > 0), so -1 is free as a marker
                    step: step_of(fu).max(-1),
                    in_deg: net.in_angle(fu) as i16,
                    link: net.edge_flags[fu] & LINK != 0,
                }
            })
            .collect();
        ModeGraph {
            mode,
            sp,
            arcs,
            java_order: mode == Mode::Car && !net.restriction_starts.is_empty(),
        }
    }

    #[inline]
    fn arcs(&self, net: &StreetNetwork, v: usize) -> &[Arc] {
        &self.arcs[net.out_offsets[v] as usize..net.out_offsets[v + 1] as usize]
    }
}

/// Reusable per-thread search state.
pub(crate) struct Search {
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
    /// Queue of `(time, edge, restricted state id or CLEAN)` for searches
    /// that need R5's exact order of equal times.
    heap: JavaHeap,
    /// Queue of `(time, edge)` for the others.
    radix: RadixHeap,
}

impl Search {
    pub(crate) fn new(net: &StreetNetwork) -> Self {
        Search {
            edge_time: vec![i32::MAX; net.n_edges()],
            vertex_time: vec![i32::MAX; net.n_vertices()],
            touched_edges: Vec::new(),
            touched_vertices: Vec::new(),
            restricted: Vec::new(),
            restricted_at: std::collections::HashMap::new(),
            heap: JavaHeap::default(),
            radix: RadixHeap::default(),
        }
    }

    /// Best time at each vertex after the last search (`i32::MAX` where
    /// unreached).
    pub(crate) fn vertex_time(&self) -> &[i32] {
        &self.vertex_time
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
        self.radix.clear();
    }

    #[inline]
    fn push<const JAVA: bool>(&mut self, t: i32, e: u32, tag: u32) {
        if JAVA {
            self.heap.offer((t, e, tag));
        } else {
            debug_assert_eq!(tag, CLEAN);
            self.radix.push(t, e);
        }
    }

    #[inline]
    fn pop<const JAVA: bool>(&mut self) -> Option<(i32, u32, u32)> {
        if JAVA {
            self.heap.poll()
        } else {
            self.radix.pop().map(|(t, e)| (t, e, CLEAN))
        }
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

    /// Record a state at the end of edge `e` (whose end vertex is `to`);
    /// returns its heap tag, or `None` when an existing state dominates it
    /// (R5 `isDominated`).
    #[inline]
    fn add_state(&mut self, e: usize, to: usize, t: i32, map: RMap) -> Option<u32> {
        if map.is_empty() {
            if t >= self.edge_time[e] {
                return None; // ties keep the existing state
            }
            if self.edge_time[e] == i32::MAX {
                self.touched_edges.push(e as u32);
            }
            self.edge_time[e] = t;
            self.touch_vertex(to, t);
            return Some(CLEAN);
        }
        let ids = self.restricted_at.entry(e as u32).or_default();
        if ids.iter().any(|&id| self.restricted[id as usize].1 == map) {
            return None;
        }
        let id = self.restricted.len() as u32;
        ids.push(id);
        self.restricted.push((e as u32, map));
        self.touch_vertex(to, t);
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

/// Turn cost for a car leaving an edge at bearing `out` (whole degrees) onto
/// one entered at bearing `inn` (R5 `turnTimeSeconds`).
#[inline]
fn car_turn_cost(out: i32, inn: i32) -> i32 {
    let mut out = out;
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
fn route(net: &StreetNetwork, g: &ModeGraph, origin: &Split, limit: i32, s: &mut Search) {
    if g.java_order {
        route_impl::<true>(net, g, origin, limit, s);
    } else {
        route_impl::<false>(net, g, origin, limit, s);
    }
}

/// [`route`] with the queue chosen at compile time: `JAVA` for the car
/// search with turn restrictions (states may carry restriction progress),
/// otherwise every state is plain.
fn route_impl<const JAVA: bool>(
    net: &StreetNetwork,
    g: &ModeGraph,
    origin: &Split,
    limit: i32,
    s: &mut Search,
) {
    s.reset();
    let (mode, sp) = (g.mode, &g.sp);
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
        if JAVA {
            start_restrictions(net, &mut map, e as u32);
        }
        if let Some(tag) = s.add_state(e, net.edge_to(e), t, map) {
            s.push::<JAVA>(t, e as u32, tag);
        }
    }
    let car = mode == Mode::Car;
    let mut map = RMap::new();
    while let Some((t, e, tag)) = s.pop::<JAVA>() {
        let e = e as usize;
        map.clear();
        if tag == CLEAN {
            if t > s.edge_time[e] {
                continue; // dominated since it was queued
            }
        } else {
            map.extend_from_slice(&s.restricted[tag as usize].1);
        }
        let e_link = net.edge_flags[e] & LINK != 0;
        let out_deg = if car { net.out_angle(e) } else { 0 };
        for a in g.arcs(net, net.edge_to(e)) {
            if a.step < 0 {
                continue; // the mode cannot use this edge
            }
            if a.link && e_link {
                continue; // two connectors in a row would be a shortcut
            }
            // Only the restricted car search carries restriction progress.
            let next_map = if JAVA {
                let mut m = if map.is_empty() {
                    RMap::new()
                } else {
                    match advance_restrictions(net, &map, a.edge) {
                        Some(m) => m,
                        None => continue,
                    }
                };
                start_restrictions(net, &mut m, a.edge);
                m
            } else {
                RMap::new()
            };
            let step = if car {
                a.step + car_turn_cost(out_deg, i32::from(a.in_deg))
            } else {
                a.step
            };
            let mut t1 = t.saturating_add(step);
            if t1 == t {
                t1 += 1;
            }
            if t1 < limit {
                if let Some(tag) = s.add_state(a.edge as usize, a.to as usize, t1, next_map) {
                    s.push::<JAVA>(t1, a.edge, tag);
                }
            }
        }
    }
}

/// Time in seconds from the searched origin to a linked destination
/// (R5 `LinkedPointSet.eval`), or `None` when unreachable.
fn time_to_point(
    net: &StreetNetwork,
    g: &ModeGraph,
    origin: &Split,
    dest: &Split,
    s: &Search,
) -> Option<i64> {
    let sp = &g.sp;
    let on = match g.mode {
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
    let g = ModeGraph::new(net, mode, Speeds::new(&params, mode), params.max_lts);
    let mut scratch = Vec::new();
    let o = find_split(net, from.0, from.1, mode, &mut scratch)?;
    let d = find_split(net, to.0, to.1, mode, &mut scratch)?;
    let mut search = Search::new(net);
    route(net, &g, &o, params.max_trip_duration_min * 60, &mut search);
    time_to_point(net, &g, &o, &d, &search)
}

/// One street search from a linked origin: the time at every vertex
/// (`i32::MAX` where unreached; states kept only below `limit` seconds) and
/// the direct time in seconds to each linked destination (`i32::MAX` where
/// unreachable), as R5's `TravelTimeComputer` uses for access and for the
/// non-transit option.
pub fn street_vertex_times(
    net: &StreetNetwork,
    origin: &Split,
    destinations: &[Option<Split>],
    mode: Mode,
    walk_speed_kmh: f64,
    limit: i32,
) -> (Vec<i32>, Vec<i32>) {
    let g = ModeGraph::with_walk_speed(net, mode, walk_speed_kmh);
    let mut search = Search::new(net);
    let mut direct = Vec::new();
    search_from(
        net,
        &g,
        origin,
        destinations,
        limit,
        &mut search,
        &mut direct,
    );
    (search.vertex_time, direct)
}

impl ModeGraph {
    /// The graph [`street_vertex_times`] searches: r5r's defaults but for
    /// the walking speed.
    pub(crate) fn with_walk_speed(net: &StreetNetwork, mode: Mode, walk_speed_kmh: f64) -> Self {
        let params = RouteParams {
            walk_speed_kmh,
            ..RouteParams::default()
        };
        ModeGraph::new(net, mode, Speeds::new(&params, mode), params.max_lts)
    }
}

/// [`street_vertex_times`] reusing a search: afterwards
/// `search.vertex_time()` holds the vertex times and `direct` the time to
/// each destination.
pub(crate) fn search_from(
    net: &StreetNetwork,
    g: &ModeGraph,
    origin: &Split,
    destinations: &[Option<Split>],
    limit: i32,
    search: &mut Search,
    direct: &mut Vec<i32>,
) {
    route(net, g, origin, limit, search);
    direct.clear();
    direct.extend(destinations.iter().map(|d| {
        d.and_then(|d| time_to_point(net, g, origin, &d, search))
            .map_or(i32::MAX, |t| t.min(i64::from(i32::MAX - 1)) as i32)
    }));
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
    let g = ModeGraph::new(net, mode, Speeds::new(params, mode), params.max_lts);
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
                            route(net, &g, &o, limit, &mut search);
                            for (j, d) in dest_splits.iter().enumerate() {
                                let Some(d) = d else { continue };
                                if let Some(secs) = time_to_point(net, &g, &o, d, &search) {
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

/// Street-only travel times in seconds, before any rounding, reduced per
/// origin: `reduce(i, Some(times))` gets origin `i`'s time to every
/// destination (`i32::MAX` where the destination is unlinked or
/// unreachable), and `reduce(i, None)` an origin that cannot be linked.
/// The search is the one behind [`travel_time_matrix`], so a time can
/// exceed `max_trip_duration_min` by the partial edges at both ends, as in
/// R5's `LinkedPointSet.eval`. Results are in origin order.
pub fn travel_time_seconds<T: Send>(
    net: &StreetNetwork,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    mode: Mode,
    params: &RouteParams,
    reduce: impl Fn(usize, Option<&[i32]>) -> T + Sync,
) -> Result<Vec<T>, ParamError> {
    params.validate()?;
    let g = ModeGraph::new(net, mode, Speeds::new(params, mode), params.max_lts);
    let limit = params.max_trip_duration_min * 60;
    let mut scratch = Vec::new();
    let dest_splits: Vec<Option<Split>> = destinations
        .iter()
        .map(|&(lat, lon)| find_split(net, lat, lon, mode, &mut scratch))
        .collect();
    let n_threads = if params.n_threads > 0 {
        params.n_threads
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get())
    }
    .clamp(1, origins.len().max(1));
    let next = AtomicUsize::new(0);
    let chunks: Vec<Vec<(usize, T)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..n_threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut search = Search::new(net);
                    let mut scratch = Vec::new();
                    let mut times = Vec::with_capacity(dest_splits.len());
                    let mut rows = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= origins.len() {
                            break;
                        }
                        let (lat, lon) = origins[i];
                        let Some(o) = find_split(net, lat, lon, mode, &mut scratch) else {
                            rows.push((i, reduce(i, None)));
                            continue;
                        };
                        search_from(net, &g, &o, &dest_splits, limit, &mut search, &mut times);
                        rows.push((i, reduce(i, Some(&times))));
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
    let mut slots: Vec<Option<T>> = (0..origins.len()).map(|_| None).collect();
    for (i, r) in chunks.into_iter().flatten() {
        slots[i] = Some(r);
    }
    Ok(slots
        .into_iter()
        .map(|r| r.expect("every origin is routed"))
        .collect())
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

    /// Deterministic pseudo-random numbers for the tests (xorshift).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    #[test]
    fn radix_heap_pops_in_key_order() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let mut h = RadixHeap::default();
        for round in 0..3 {
            h.clear();
            let mut reference: Vec<i32> = Vec::new();
            let mut last = i32::MIN;
            // keys before the first pop may be anything, negative included
            for k in 0..50 {
                let t = (rng.next() % 2000) as i32 - 1000;
                h.push(t, k);
                reference.push(t);
            }
            for i in 0..5000u32 {
                if rng.next() % 3 != 0 || reference.is_empty() {
                    // later keys never go below the last popped one
                    let base = if last == i32::MIN { 0 } else { last };
                    let t = base.saturating_add((rng.next() % (1 << (round * 8 + 4))) as i32);
                    h.push(t, i);
                    reference.push(t);
                } else {
                    let (t, _) = h.pop().expect("non-empty");
                    reference.sort_unstable_by(|a, b| b.cmp(a));
                    assert_eq!(t, reference.pop().unwrap());
                    assert!(t >= last);
                    last = t;
                }
            }
            reference.sort_unstable();
            for want in reference {
                assert_eq!(h.pop().map(|(t, _)| t), Some(want));
            }
            assert_eq!(h.pop(), None);
        }
    }

    /// A 12 x 12 grid of streets of mixed kinds with jittered nodes, so
    /// edge lengths, speeds, permissions and turn angles vary.
    fn jittered_grid() -> StreetNetwork {
        use crate::osm::pbf::{Extract, Tags, Way};
        const N: usize = 12;
        let mut rng = Rng(42);
        let kinds = [
            "residential",
            "primary",
            "footway",
            "cycleway",
            "secondary",
            "service",
        ];
        let node = |r: usize, c: usize| (1 + r * N + c) as i64;
        let mut ways = Vec::new();
        for r in 0..N {
            let kind = kinds[(rng.next() % kinds.len() as u64) as usize];
            let mut tags = vec![("highway".to_string(), kind.to_string())];
            if r % 5 == 1 {
                tags.push(("oneway".into(), "yes".into()));
            }
            ways.push(Way {
                id: 1000 + r as i64,
                tags: Tags(tags),
                nodes: (0..N).map(|c| node(r, c)).collect(),
            });
        }
        for c in 0..N {
            let kind = kinds[(rng.next() % kinds.len() as u64) as usize];
            ways.push(Way {
                id: 2000 + c as i64,
                tags: Tags(vec![("highway".to_string(), kind.to_string())]),
                nodes: (0..N).map(|r| node(r, c)).collect(),
            });
        }
        let node_ids: Vec<i64> = (0..N * N).map(|i| 1 + i as i64).collect();
        let jitter = |rng: &mut Rng| (rng.next() % 4000) as i32 - 2000;
        let mut lat = Vec::new();
        let mut lon = Vec::new();
        for i in 0..N * N {
            lat.push(((i / N) * 10_000) as i32 + jitter(&mut rng));
            lon.push(((i % N) * 10_000) as i32 + jitter(&mut rng));
        }
        crate::street::build::network_from_extract(Extract {
            ref_counts: vec![2; node_ids.len()],
            member_nodes_present: Vec::new(),
            member_ways_present: Vec::new(),
            node_ids,
            lat,
            lon,
            ways,
            relations: Vec::new(),
            node_tags: Vec::new(),
            pois: Vec::new(),
        })
    }

    /// Without turn restrictions every state is plain, and the radix queue
    /// must end with exactly the times of the `java.util.PriorityQueue`
    /// replica, from every origin, in every mode.
    #[test]
    fn radix_and_java_queues_give_the_same_times() {
        let net = jittered_grid();
        assert!(net.n_edges() > 400);
        let params = RouteParams::default();
        let mut scratch = Vec::new();
        let (mut a, mut b) = (Search::new(&net), Search::new(&net));
        for mode in [Mode::Walk, Mode::Bicycle, Mode::Car] {
            let g = ModeGraph::new(&net, mode, Speeds::new(&params, mode), params.max_lts);
            assert!(!g.java_order);
            let mut searched = 0;
            for r in 0..23 {
                for c in 0..23 {
                    let (lat, lon) = (r as f64 * 5.1e-4 + 1e-5, c as f64 * 4.9e-4 + 2e-5);
                    let Some(o) = find_split(&net, lat, lon, mode, &mut scratch) else {
                        continue;
                    };
                    for limit in [120, 7200] {
                        route_impl::<true>(&net, &g, &o, limit, &mut a);
                        route_impl::<false>(&net, &g, &o, limit, &mut b);
                        assert_eq!(a.vertex_time, b.vertex_time, "{mode:?} {r} {c} {limit}");
                        assert_eq!(a.edge_time, b.edge_time, "{mode:?} {r} {c} {limit}");
                        searched += 1;
                    }
                }
            }
            assert!(searched > 500, "{mode:?}: {searched}");
        }
    }
}
