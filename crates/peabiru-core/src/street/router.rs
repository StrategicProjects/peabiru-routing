//! Street-only travel times (R5 `StreetRouter` + `LinkedPointSet.eval`).
//!
//! The search runs over directed-edge states: the time at the end of each
//! edge, so car turn costs can depend on the edge a vehicle arrives by.

use super::flags::*;
use super::labels::cms_to_ms;
use super::link::{find_split, link_points, Split};
use super::StreetNetwork;
use crate::{CancelToken, RouteError};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Street mode, as r5r's `mode` for street-only routing. Non-exhaustive:
/// R5 has further street modes (bike or car rental and parking) that a
/// later version may add.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Mode {
    /// `WALK`, at [`RouteParams::walk_speed_kmh`].
    Walk,
    /// `BICYCLE`, at [`RouteParams::bike_speed_kmh`] on streets up to
    /// [`RouteParams::max_lts`], walking where cycling is not allowed.
    Bicycle,
    /// `CAR`, at the speed limits of OpenStreetMap (or R5's defaults by
    /// road type), with turn costs and turn restrictions.
    Car,
}

impl Mode {
    /// The name R5 and r5r give the mode (`WALK`, `BICYCLE`, `CAR`).
    pub(crate) fn r5_name(self) -> &'static str {
        match self {
            Mode::Walk => "WALK",
            Mode::Bicycle => "BICYCLE",
            Mode::Car => "CAR",
        }
    }
}

/// Car turn costs in seconds (R5 `BasicTraversalTimeCalculator`, driving on
/// the right).
const STRAIGHT_ON: i32 = 0;
const RIGHT_TURN: i32 = 10;
const LEFT_TURN: i32 = 30;
const U_TURN: i32 = 90;

/// How [`street_travel_time_matrix`] computes a street matrix. Every choice gives
/// the same matrix; they differ only in speed and memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum StreetEngine {
    /// The faster of the two for the matrix at hand: the contraction
    /// hierarchy when the network already holds one for the mode and
    /// speeds, or when there are enough origins to pay for building it;
    /// otherwise the search.
    #[default]
    Auto,
    /// A street search from every origin, as R5 does.
    Search,
    /// A contraction hierarchy of the mode's street graph, built on first
    /// use and kept with the network for later calls. Cars on a network
    /// with turn restrictions always take the search.
    Hierarchy,
}

/// Street routing parameters, with r5r's defaults. Fill it in by changing
/// the fields of [`RouteParams::default()`]:
///
/// ```
/// let mut p = peabiru_core::RouteParams::default();
/// p.bike_speed_kmh = 15.0;
/// p.max_lts = 3;
/// assert!(p.validate().is_ok());
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RouteParams {
    /// Walking speed, km/h (default 3.6).
    pub walk_speed_kmh: f64,
    /// Cycling speed, km/h (default 12).
    pub bike_speed_kmh: f64,
    /// Highest level of traffic stress a cyclist rides on, 1 to 4
    /// (default 2).
    pub max_lts: i32,
    /// Trip limit in whole minutes (default 120).
    pub max_trip_duration_min: i32,
    /// Worker threads; 0 means all cores, and more than the cores
    /// available run as many as there are cores.
    pub n_threads: usize,
    /// Stops the computation when cancelled, from any thread, through a
    /// clone of it (see [`CancelToken`]); the default is a fresh token,
    /// which nothing cancels unless a clone of it is kept.
    pub cancel: CancelToken,
    /// How a street travel-time matrix is computed (see [`StreetEngine`]);
    /// it changes the speed of [`street_travel_time_matrix`], never its result.
    /// A call with a [`Scenario`](crate::Scenario) always takes the search.
    pub engine: StreetEngine,
    /// Car speeds and levels of traffic stress changed for this call
    /// (r5r's `new_carspeeds`, `carspeed_scale`, `new_lts`); the default
    /// changes nothing.
    pub scenario: crate::Scenario,
}

/// A number for an error message: as written when short, else in
/// scientific notation (`1e300`, not 301 digits).
pub(crate) fn compact(v: f64) -> String {
    let s = format!("{v}");
    if s.len() <= 16 {
        s
    } else {
        format!("{v:e}")
    }
}

/// Check a speed in km/h as R5 does: finite, and at least 1 mm/s once
/// converted to `(int)(m/s as float * 1000)`, which is a divisor
/// (`TravelTimeComputer`: "Speed of access mode must be greater than 0").
pub(crate) fn check_speed(name: &str, v: f64) -> Result<(), String> {
    if !v.is_finite() || ((v * 5.0 / 18.0) as f32 * 1000.0f32) < 1.0 {
        return Err(format!(
            "{name} must be at least 0.0036 km/h (1 mm/s), got {}",
            compact(v)
        ));
    }
    Ok(())
}

impl RouteParams {
    /// Check the parameters: speeds must give at least 1 mm/s (R5's
    /// bound), `max_lts` must be 1 to 4 (r5r's bound), the trip limit must
    /// be a positive number of minutes that fits R5's `int` seconds. Every
    /// street routing function calls it first; [`RouteError::Invalid`]
    /// says what is wrong.
    pub fn validate(&self) -> Result<(), RouteError> {
        check_speed("walk_speed_kmh", self.walk_speed_kmh)?;
        check_speed("bike_speed_kmh", self.bike_speed_kmh)?;
        // r5r `set_max_lts`: 1 to 4 (R5 treats anything else as 4)
        if !(1..=4).contains(&self.max_lts) {
            return Err(RouteError::Invalid(format!(
                "max_lts must be from 1 to 4 (the highest level of traffic stress), got {}",
                self.max_lts
            )));
        }
        let max_minutes = i32::MAX / 60;
        if !(1..=max_minutes).contains(&self.max_trip_duration_min) {
            return Err(RouteError::Invalid(format!(
                "max_trip_duration must be from 1 to {max_minutes} minutes, got {}",
                self.max_trip_duration_min
            )));
        }
        self.scenario.check().map_err(RouteError::Invalid)
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
            cancel: CancelToken::default(),
            engine: StreetEngine::Auto,
            scenario: crate::Scenario::default(),
        }
    }
}

/// Worker threads for `n_items` origins when `requested` were asked for
/// (0 = all cores): never more than the cores available, since each worker
/// holds its own search state and more threads than cores only add memory,
/// and never more than the origins. Results never depend on it: origins
/// are handed out one at a time and stored by index.
pub(crate) fn worker_threads(requested: usize, n_items: usize) -> usize {
    let cores = crate::sys::cores();
    let n = if requested == 0 {
        cores
    } else {
        requested.min(cores)
    };
    n.clamp(1, n_items.max(1))
}

/// Speeds as R5 holds them: km/h to m/s in double, then `(float)`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Speeds {
    pub(super) walk: f32,
    pub(super) bike: f32,
    /// `(int)(walkSpeed * 1000)`, mm/s, the off-street speed.
    pub(super) walk_mms: i32,
    /// `(int)(speed * 1000)` for walk/bike destination edges, mm/s.
    on_street_mms: i32,
}

impl Speeds {
    pub(super) fn new(p: &RouteParams, mode: Mode) -> Self {
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
pub(crate) struct RadixHeap {
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

    pub(crate) fn clear(&mut self) {
        for b in &mut self.buckets {
            b.clear();
        }
        self.last = 0;
        self.len = 0;
    }

    #[inline]
    pub(crate) fn push(&mut self, t: i32, e: u32) {
        // A key below the last popped one cannot come from a search with
        // non-negative steps; clamping keeps the queue consistent anyway.
        let k = Self::key(t).max(self.last);
        let b = self.bucket(k);
        self.buckets[b].push((k, e));
        self.len += 1;
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Option<(i32, u32)> {
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
pub(super) struct Arc {
    pub(super) edge: u32,
    pub(super) to: u32,
    /// Traversal seconds for the mode (before the car turn cost); negative
    /// when the mode cannot use the edge.
    pub(super) step: i32,
    /// Car: bearing entering the edge, whole degrees (R5 `getInAngle`).
    pub(super) in_deg: i16,
    pub(super) link: bool,
}

/// The street graph as one mode's search reads it: every vertex's outgoing
/// edges with their traversal times precomputed and laid out contiguously
/// (`arcs[out_offsets[v]..out_offsets[v + 1]]`, in `out_edges` order), so
/// the inner loop of the search reads one array. Built once per query.
pub(crate) struct ModeGraph {
    pub(super) mode: Mode,
    pub(super) sp: Speeds,
    arcs: Vec<Arc>,
    /// The search must pop equal times in `java.util.PriorityQueue` order:
    /// car searches on a network with turn restrictions.
    pub(super) java_order: bool,
}

impl ModeGraph {
    pub(super) fn new(net: &StreetNetwork, mode: Mode, sp: Speeds, max_lts: i32) -> Self {
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
    pub(super) fn arcs(&self, net: &StreetNetwork, v: usize) -> &[Arc] {
        &self.arcs[net.out_offsets[v] as usize..net.out_offsets[v + 1] as usize]
    }
}

/// Reusable per-thread search state.
pub(crate) struct Search {
    /// Best time at the end of each directed edge, over states without
    /// restriction progress.
    edge_time: Vec<i32>,
    /// Best time at each vertex (min over all incoming edge states).
    pub(super) vertex_time: Vec<i32>,
    touched_edges: Vec<u32>,
    touched_vertices: Vec<u32>,
    /// States in the middle of turn restrictions: `(edge, progress)`. R5
    /// keeps them apart: they neither dominate nor are dominated by plain
    /// states, and the first state with a given progress on an edge wins.
    restricted: Vec<(u32, RMap)>,
    /// Parallel to `restricted`: the state's time, and whether it was
    /// made on foot by [`continue_on_foot`] (R5's `State.streetMode` is
    /// then `WALK`, and its restriction progress is never carried on).
    restricted_time: Vec<i32>,
    restricted_walk: Vec<bool>,
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
            restricted_time: Vec::new(),
            restricted_walk: Vec::new(),
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
        self.restricted_time.clear();
        self.restricted_walk.clear();
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

    /// [`Search::add_state`] for a state without restriction progress:
    /// whether it was kept.
    #[inline(always)]
    fn add_plain(&mut self, e: usize, to: usize, t: i32) -> bool {
        let cur = self.edge_time[e];
        if t >= cur {
            return false; // ties keep the existing state
        }
        if cur == i32::MAX {
            self.touched_edges.push(e as u32);
        }
        self.edge_time[e] = t;
        self.touch_vertex(to, t);
        true
    }

    /// Record a state at the end of edge `e` (whose end vertex is `to`);
    /// returns its heap tag, or `None` when an existing state dominates it
    /// (R5 `isDominated`).
    #[inline]
    fn add_state(&mut self, e: usize, to: usize, t: i32, map: RMap, walk: bool) -> Option<u32> {
        if map.is_empty() {
            return self.add_plain(e, to, t).then_some(CLEAN);
        }
        let ids = self.restricted_at.entry(e as u32).or_default();
        if ids.iter().any(|&id| self.restricted[id as usize].1 == map) {
            return None;
        }
        let id = self.restricted.len() as u32;
        ids.push(id);
        self.restricted.push((e as u32, map));
        self.restricted_time.push(t);
        self.restricted_walk.push(walk);
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
pub(super) fn car_turn_cost(out: i32, inn: i32) -> i32 {
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
pub(crate) fn route(
    net: &StreetNetwork,
    g: &ModeGraph,
    origin: &Split,
    limit: i32,
    s: &mut Search,
) {
    if g.java_order {
        route_impl::<true>(net, g, origin, limit, s);
    } else {
        route_impl::<false>(net, g, origin, limit, s);
    }
}

/// The two start states of a search from an origin split, as `(edge,
/// seconds)`: the backward edge, which ends at the split's first vertex,
/// then the forward edge. They carry the split edge as back edge so turn
/// costs (and turn restrictions) apply when leaving it, and their times are
/// not checked against the limit.
pub(super) fn start_states(
    net: &StreetNetwork,
    g: &ModeGraph,
    origin: &Split,
) -> [(usize, i32); 2] {
    let (mode, sp) = (g.mode, &g.sp);
    let fwd = origin.edge as usize;
    let bwd = fwd + 1;
    let off = origin.distance_to_edge_mm / sp.walk_mms;
    let start_speed = |e: usize| match mode {
        Mode::Walk => sp.walk,
        Mode::Bicycle => sp.bike,
        Mode::Car => cms_to_ms(net.edge_speed_cms[e]),
    };
    let t1 = ((origin.distance1_mm / 1000) as f32 / start_speed(fwd)) as i32 + off;
    let t0 = ((origin.distance0_mm / 1000) as f32 / start_speed(bwd)) as i32 + off;
    [(bwd, t0), (fwd, t1)]
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
    let mode = g.mode;
    for (e, t) in start_states(net, g, origin) {
        let mut map = RMap::new();
        if JAVA {
            start_restrictions(net, &mut map, e as u32);
        }
        if let Some(tag) = s.add_state(e, net.edge_to(e), t, map, false) {
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
                if !JAVA {
                    if s.add_plain(a.edge as usize, a.to as usize, t1) {
                        s.push::<JAVA>(t1, a.edge, CLEAN);
                    }
                } else if let Some(tag) =
                    s.add_state(a.edge as usize, a.to as usize, t1, next_map, false)
                {
                    s.push::<JAVA>(t1, a.edge, tag);
                }
            }
        }
    }
}

/// Heap tag of a plain state made on foot by [`continue_on_foot`] (plain
/// states of the first search are [`CLEAN`]).
const CLEAN_WALK: u32 = u32::MAX - 1;

/// Carry a finished bicycle or car search on on foot, as R5's
/// `StreetRouter.keepRoutingOnFoot` does before reading the stops reached
/// for public transport access: every state the search kept goes back into
/// the queue, and the search goes on by `foot` (the walking graph) within
/// the same `limit`, adding states to the same tables. The vertex times
/// after it are the least over states by the first mode and on foot.
///
/// R5 keeps the mode of each state: a step on foot out of a car state still
/// checks that car state's turn restrictions and starts those of the edge
/// it walks onto (`canTurnFrom` and `startTurnRestriction` look at the
/// mode of the state they leave), so such a walking state can carry
/// restriction progress, which makes it incomparable to plain states and
/// beaten by an earlier state with the same progress on its edge. A step
/// out of a walking state carries none. Equal times may leave the queue in
/// any order here: the states on one edge with the same progress are made
/// by steps of the same length out of states that leave the queue in order
/// of time, so the first one made is also the earliest.
pub(crate) fn continue_on_foot(
    net: &StreetNetwork,
    first: &ModeGraph,
    foot: &ModeGraph,
    limit: i32,
    s: &mut Search,
) {
    debug_assert_eq!(foot.mode, Mode::Walk);
    if !first.java_order {
        // no state of the first search is a car state on a network with
        // turn restrictions
        continue_on_foot_plain(net, foot, limit, s);
        return;
    }
    s.heap.clear();
    for i in 0..s.touched_edges.len() {
        let e = s.touched_edges[i];
        let t = s.edge_time[e as usize];
        if t < i32::MAX {
            s.heap.offer((t, e, CLEAN));
        }
    }
    for id in 0..s.restricted.len() {
        s.heap
            .offer((s.restricted_time[id], s.restricted[id].0, id as u32));
    }
    let mut map = RMap::new();
    while let Some((t, e, tag)) = s.heap.poll() {
        let e = e as usize;
        map.clear();
        // whether the state left is a car state, whose restrictions apply
        let by_car = match tag {
            CLEAN | CLEAN_WALK => {
                if t > s.edge_time[e] {
                    continue; // dominated since it was queued
                }
                tag == CLEAN
            }
            id => {
                let id = id as usize;
                if s.restricted_walk[id] {
                    false
                } else {
                    map.extend_from_slice(&s.restricted[id].1);
                    true
                }
            }
        };
        let e_link = net.edge_flags[e] & LINK != 0;
        for a in foot.arcs(net, net.edge_to(e)) {
            if a.step < 0 || (a.link && e_link) {
                continue;
            }
            let next_map = if by_car {
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
            let mut t1 = t.saturating_add(a.step);
            if t1 == t {
                t1 += 1;
            }
            if t1 < limit {
                if let Some(tag) = s.add_state(a.edge as usize, a.to as usize, t1, next_map, true) {
                    let tag = if tag == CLEAN { CLEAN_WALK } else { tag };
                    s.heap.offer((t1, a.edge, tag));
                }
            }
        }
    }
}

/// [`continue_on_foot`] when no step can carry restriction progress (after
/// a bicycle search, or a car search on a network without restrictions):
/// every state is plain and the mode of the state left changes nothing.
fn continue_on_foot_plain(net: &StreetNetwork, foot: &ModeGraph, limit: i32, s: &mut Search) {
    s.radix.clear();
    for i in 0..s.touched_edges.len() {
        let e = s.touched_edges[i];
        let t = s.edge_time[e as usize];
        if t < i32::MAX {
            s.radix.push(t, e);
        }
    }
    while let Some((t, e)) = s.radix.pop() {
        let e = e as usize;
        if t > s.edge_time[e] {
            continue;
        }
        let e_link = net.edge_flags[e] & LINK != 0;
        for a in foot.arcs(net, net.edge_to(e)) {
            if a.step < 0 || (a.link && e_link) {
                continue;
            }
            let mut t1 = t.saturating_add(a.step);
            if t1 == t {
                t1 += 1;
            }
            if t1 < limit && s.add_plain(a.edge as usize, a.to as usize, t1) {
                s.radix.push(t1, a.edge);
            }
        }
    }
}

/// Time in seconds from the searched origin to a linked destination
/// (R5 `LinkedPointSet.eval`), or `None` when unreachable.
pub(super) fn time_to_point(
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

#[cfg(test)]
/// Seconds by street from one point to another with r5r's defaults, before
/// rounding to minutes; `None` if either point is unlinked or the
/// destination is unreachable. For tests and diagnostics.
pub(crate) fn street_seconds(
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

#[cfg(test)]
/// One street search from a linked origin: the time at every vertex
/// (`i32::MAX` where unreached; states kept only below `limit` seconds) and
/// the direct time in seconds to each linked destination (`i32::MAX` where
/// unreachable), as R5's `TravelTimeComputer` uses for access and for the
/// non-transit option. An error when `walk_speed_kmh` is not a valid speed
/// (see [`RouteParams::validate`]).
pub(crate) fn street_vertex_times(
    net: &StreetNetwork,
    origin: &Split,
    destinations: &[Option<Split>],
    mode: Mode,
    walk_speed_kmh: f64,
    limit: i32,
) -> Result<(Vec<i32>, Vec<i32>), RouteError> {
    RouteParams {
        walk_speed_kmh,
        ..RouteParams::default()
    }
    .validate()?;
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
    Ok((search.vertex_time, direct))
}

#[cfg(test)]
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

impl ModeGraph {
    /// The graph of `mode` at these speeds (km/h) and highest level of
    /// traffic stress, as a street search of r5r with them reads it.
    pub(crate) fn with_speeds(
        net: &StreetNetwork,
        mode: Mode,
        walk_speed_kmh: f64,
        bike_speed_kmh: f64,
        max_lts: i32,
    ) -> Self {
        let params = RouteParams {
            walk_speed_kmh,
            bike_speed_kmh,
            max_lts,
            ..RouteParams::default()
        };
        ModeGraph::new(net, mode, Speeds::new(&params, mode), max_lts)
    }

    /// The graph of `mode` for a street search with the defaults of R5's
    /// `ProfileRequest` (walking 1.3 m/s, cycling 4 m/s, any level of
    /// traffic stress), as the searches R5 runs without a request, from
    /// the stops of an `EgressCostTable`, read it.
    pub(crate) fn r5_defaults(net: &StreetNetwork, mode: Mode) -> Self {
        let (walk, bike) = (1.3f32, 4.0f32);
        let on = match mode {
            Mode::Walk => walk,
            Mode::Bicycle => bike,
            Mode::Car => 2.22f32,
        };
        let sp = Speeds {
            walk,
            bike,
            walk_mms: (walk * 1000.0f32) as i32,
            on_street_mms: (on * 1000.0f32) as i32,
        };
        ModeGraph::new(net, mode, sp, 4)
    }

    /// The street mode this graph is searched by.
    pub(crate) fn mode(&self) -> Mode {
        self.mode
    }
}

/// The access search of R5's `TravelTimeComputer` for public transport:
/// [`search_from`] by `g`, carried on on foot by `foot` when given (a
/// bicycle or car access, see [`continue_on_foot`]) before the direct
/// times to `destinations` are read, as R5 reads them after
/// `keepRoutingOnFoot`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn access_search_from(
    net: &StreetNetwork,
    g: &ModeGraph,
    foot: Option<&ModeGraph>,
    origin: &Split,
    destinations: &[Option<Split>],
    limit: i32,
    search: &mut Search,
    direct: &mut Vec<i32>,
) {
    route(net, g, origin, limit, search);
    if let Some(foot) = foot {
        continue_on_foot(net, g, foot, limit, search);
    }
    direct.clear();
    direct.extend(destinations.iter().map(|d| {
        d.and_then(|d| time_to_point(net, g, origin, &d, search))
            .map_or(i32::MAX, |t| t.min(i64::from(i32::MAX - 1)) as i32)
    }));
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

/// A street travel-time matrix: whole minutes from each origin to each
/// destination, `None` where r5r reports no time (an origin or destination
/// that cannot be linked to the streets, an unreachable destination, or a
/// time beyond `max_trip_duration_min`).
///
/// The minutes are held in one buffer, row by row (all destinations of the
/// first origin, then of the second, ...), which
/// [`into_vec`](StreetMatrix::into_vec) hands over without copying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreetMatrix {
    n_origins: usize,
    n_destinations: usize,
    /// `minutes[origin * n_destinations + destination]`.
    minutes: Vec<Option<u32>>,
}

impl StreetMatrix {
    pub(crate) fn new(n_origins: usize, n_destinations: usize, minutes: Vec<Option<u32>>) -> Self {
        debug_assert_eq!(minutes.len(), n_origins * n_destinations);
        StreetMatrix {
            n_origins,
            n_destinations,
            minutes,
        }
    }

    /// Number of origins (rows).
    pub fn n_origins(&self) -> usize {
        self.n_origins
    }

    /// Number of destinations (columns).
    pub fn n_destinations(&self) -> usize {
        self.n_destinations
    }

    /// Minutes from origin `origin` to destination `destination` (indices
    /// in the order the points were given). Panics when either is out of
    /// range.
    pub fn get(&self, origin: usize, destination: usize) -> Option<u32> {
        assert!(
            origin < self.n_origins && destination < self.n_destinations,
            "({origin}, {destination}) is outside a {} x {} matrix",
            self.n_origins,
            self.n_destinations
        );
        self.minutes[origin * self.n_destinations + destination]
    }

    /// The minutes from origin `origin` to every destination. Panics when
    /// `origin` is out of range.
    pub fn row(&self, origin: usize) -> &[Option<u32>] {
        assert!(origin < self.n_origins, "origin {origin} out of range");
        let nd = self.n_destinations;
        &self.minutes[origin * nd..(origin + 1) * nd]
    }

    /// The rows in origin order, as [`row`](StreetMatrix::row) gives them.
    pub fn rows(&self) -> impl ExactSizeIterator<Item = &[Option<u32>]> + '_ {
        (0..self.n_origins).map(move |i| self.row(i))
    }

    /// The whole matrix as one slice, row by row.
    pub fn as_slice(&self) -> &[Option<u32>] {
        &self.minutes
    }

    /// The whole matrix as one vector, row by row, without copying:
    /// the value for `(origin, destination)` is at
    /// `origin * n_destinations + destination`.
    pub fn into_vec(self) -> Vec<Option<u32>> {
        self.minutes
    }
}

/// Street travel-time matrix in whole minutes by walking, cycling or
/// driving, as r5r's `travel_time_matrix()` with a single street mode.
/// Points are `(lat, lon)` degrees. [`RouteError::Invalid`] when the
/// parameters are invalid (see [`RouteParams::validate`]),
/// [`RouteError::Cancelled`] when `params.cancel` is cancelled before the
/// matrix is complete.
///
/// `params.engine` chooses how the matrix is computed (see
/// [`StreetEngine`]); the result is the same either way. A contraction
/// hierarchy built here is kept with the network and makes later matrices
/// of the same mode and speeds faster.
pub fn street_travel_time_matrix(
    net: &StreetNetwork,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    mode: Mode,
    params: &RouteParams,
) -> Result<StreetMatrix, RouteError> {
    let minutes = matrix_minutes(net, origins, destinations, mode, params)?;
    Ok(StreetMatrix::new(
        origins.len(),
        destinations.len(),
        minutes,
    ))
}

/// The minutes of [`street_travel_time_matrix`], row by row.
fn matrix_minutes(
    net: &StreetNetwork,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    mode: Mode,
    params: &RouteParams,
) -> Result<Vec<Option<u32>>, RouteError> {
    params.validate()?;
    // a scenario: the searches and the origins' links on its network, the
    // destinations linked on `net` (R5 copies the base linkage)
    let scen = super::scenario::scenario_network(net, &params.scenario, mode == Mode::Car)?;
    let street = scen.as_ref().unwrap_or(net);
    let g = ModeGraph::new(street, mode, Speeds::new(params, mode), params.max_lts);
    let m = params.max_trip_duration_min;
    if scen.is_none() {
        if let Some(m) = super::ch::travel_time_matrix(net, &g, origins, destinations, params) {
            return m;
        }
    }
    let limit = m * 60;
    let dest_splits = link_points(
        net,
        destinations.len(),
        |j| destinations[j],
        mode,
        params.n_threads,
        &params.cancel,
    )
    .ok_or(RouteError::Cancelled)?;
    let nd = destinations.len();
    let mut out = vec![None; origins.len() * nd];
    if out.is_empty() {
        params.cancel.check()?;
        return Ok(out);
    }
    let cancel = &params.cancel;
    let n_threads = worker_threads(params.n_threads, origins.len());
    // Each worker takes the next origin's row of `out` and writes it in
    // place: the rows are disjoint, so the matrix is never held twice.
    let rows = Mutex::new(out.chunks_mut(nd).enumerate());
    crate::sys::scoped(0..n_threads, |_| {
        let mut search = Search::new(street);
        let mut scratch = Vec::new();
        while !cancel.is_cancelled() {
            let next = rows.lock().expect("row queue").next();
            let Some((i, row)) = next else { break };
            let (lat, lon) = origins[i];
            if let Some(o) = find_split(street, lat, lon, mode, &mut scratch) {
                route(street, &g, &o, limit, &mut search);
                for (j, d) in dest_splits.iter().enumerate() {
                    let Some(d) = d else { continue };
                    if let Some(secs) = time_to_point(street, &g, &o, d, &search) {
                        let minutes = secs / 60;
                        if minutes <= i64::from(m) {
                            row[j] = Some(minutes as u32);
                        }
                    }
                }
            }
        }
    });
    cancel.check()?;
    Ok(out)
}

/// Street-only travel times in seconds, before any rounding, reduced per
/// origin: `reduce(i, Some(times))` gets origin `i`'s time to every
/// destination (`i32::MAX` where the destination is unlinked or
/// unreachable), and `reduce(i, None)` an origin that cannot be linked.
/// The search is the one behind [`street_travel_time_matrix`], so a time can
/// exceed `max_trip_duration_min` by the partial edges at both ends, as in
/// R5's `LinkedPointSet.eval`. Results are in origin order;
/// [`RouteError::Cancelled`] when `params.cancel` is cancelled before every
/// origin is done.
pub(crate) fn travel_time_seconds<T: Send>(
    net: &StreetNetwork,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    mode: Mode,
    params: &RouteParams,
    reduce: impl Fn(usize, Option<&[i32]>) -> T + Sync,
) -> Result<Vec<T>, RouteError> {
    params.validate()?;
    let dest_splits = link_points(
        net,
        destinations.len(),
        |j| destinations[j],
        mode,
        params.n_threads,
        &params.cancel,
    )
    .ok_or(RouteError::Cancelled)?;
    travel_time_seconds_linked(net, origins, &dest_splits, mode, params, reduce)
}

/// [`travel_time_seconds`] to destinations already linked by `mode`
/// (`dest_splits`, `None` for one that cannot be linked) on `net`; with a
/// scenario, the searches run on the scenario's network.
pub(crate) fn travel_time_seconds_linked<T: Send>(
    net: &StreetNetwork,
    origins: &[(f64, f64)],
    dest_splits: &[Option<Split>],
    mode: Mode,
    params: &RouteParams,
    reduce: impl Fn(usize, Option<&[i32]>) -> T + Sync,
) -> Result<Vec<T>, RouteError> {
    params.validate()?;
    let scen = super::scenario::scenario_network(net, &params.scenario, mode == Mode::Car)?;
    let net = scen.as_ref().unwrap_or(net);
    let g = ModeGraph::new(net, mode, Speeds::new(params, mode), params.max_lts);
    let limit = params.max_trip_duration_min * 60;
    let n_threads = worker_threads(params.n_threads, origins.len());
    let cancel = &params.cancel;
    let next = AtomicUsize::new(0);
    let chunks: Vec<Vec<(usize, T)>> = crate::sys::scoped(0..n_threads, |_| {
        let mut search = Search::new(net);
        let mut scratch = Vec::new();
        let mut times = Vec::with_capacity(dest_splits.len());
        let mut rows = Vec::new();
        while !cancel.is_cancelled() {
            let i = next.fetch_add(1, Ordering::Relaxed);
            if i >= origins.len() {
                break;
            }
            let (lat, lon) = origins[i];
            let Some(o) = find_split(net, lat, lon, mode, &mut scratch) else {
                rows.push((i, reduce(i, None)));
                continue;
            };
            search_from(net, &g, &o, dest_splits, limit, &mut search, &mut times);
            rows.push((i, reduce(i, Some(&times))));
        }
        rows
    });
    cancel.check()?;
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
pub(crate) mod tests {
    use super::*;

    #[test]
    fn worker_threads_are_capped_by_cores_and_origins() {
        let cores = crate::sys::cores();
        assert_eq!(worker_threads(0, 1_000_000), cores);
        assert_eq!(worker_threads(5041, 1_000_000), cores);
        assert_eq!(worker_threads(1, 1_000_000), 1);
        assert_eq!(worker_threads(5041, 3), cores.min(3));
        assert_eq!(worker_threads(4, 0), 1);
    }

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
            RouteParams {
                max_lts: 0,
                ..RouteParams::default()
            },
            RouteParams {
                max_lts: 7,
                ..RouteParams::default()
            },
        ];
        for p in bad {
            assert!(
                street_travel_time_matrix(&net, &[(0.0, 0.0)], &[(0.0, 0.0)], Mode::Walk, &p)
                    .is_err()
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
    pub(crate) fn jittered_grid() -> StreetNetwork {
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

    /// Rows written in place by the workers: every row lands at its
    /// origin's offset whatever the thread count, and each cell is the
    /// single-origin search's time.
    #[test]
    fn matrix_rows_are_in_origin_order_for_any_thread_count() {
        let net = jittered_grid();
        let pts: Vec<(f64, f64)> = (0..40)
            .map(|k| {
                (
                    (k % 7) as f64 * 9.7e-4 + 1e-5,
                    (k / 7) as f64 * 9.3e-4 + 2e-5,
                )
            })
            .collect();
        let dest = &pts[..13];
        for mode in [Mode::Walk, Mode::Car] {
            let one = RouteParams {
                n_threads: 1,
                ..RouteParams::default()
            };
            let m = street_travel_time_matrix(&net, &pts, dest, mode, &one).unwrap();
            assert_eq!((m.n_origins(), m.n_destinations()), (pts.len(), dest.len()));
            for (i, row) in m.rows().enumerate() {
                assert_eq!(row, m.row(i));
                for (j, &cell) in row.iter().enumerate() {
                    assert_eq!(cell, m.get(i, j));
                }
            }
            let m1 = m.into_vec();
            assert_eq!(m1.len(), pts.len() * dest.len());
            assert!(m1.iter().filter(|c| c.is_some()).count() > 100);
            for n_threads in [0, 3, 64] {
                let p = RouteParams {
                    n_threads,
                    ..RouteParams::default()
                };
                let m = street_travel_time_matrix(&net, &pts, dest, mode, &p).unwrap();
                assert_eq!(m.as_slice(), m1);
            }
            for (i, &o) in pts.iter().enumerate() {
                let row = street_travel_time_matrix(&net, &[o], dest, mode, &one).unwrap();
                assert_eq!(
                    row.as_slice(),
                    &m1[i * dest.len()..(i + 1) * dest.len()],
                    "{i}"
                );
            }
            let empty = street_travel_time_matrix(&net, &pts, &[], mode, &one).unwrap();
            assert!(empty.as_slice().is_empty());
            assert_eq!(empty.rows().len(), pts.len());
            assert!(empty.rows().all(<[_]>::is_empty));
        }
    }

    /// The walk continuation of a bicycle or car search (R5
    /// `keepRoutingOnFoot`) against plain Bellman-Ford relaxation on foot
    /// from every edge state the first search kept: same vertex times, and
    /// never later than the first search alone.
    #[test]
    fn continuing_on_foot_matches_relaxation() {
        let net = jittered_grid();
        let foot = ModeGraph::with_speeds(&net, Mode::Walk, 3.6, 12.0, 2);
        let mut scratch = Vec::new();
        let (mut checked, mut improved) = (0, 0);
        for mode in [Mode::Bicycle, Mode::Car] {
            let g = ModeGraph::with_speeds(&net, mode, 3.6, 12.0, 1);
            assert!(!g.java_order);
            for k in 0..40 {
                let (lat, lon) = (f64::from(k % 11) * 0.001 + 0.0003, f64::from(k / 4) * 0.001);
                let Some(o) = find_split(&net, lat, lon, mode, &mut scratch) else {
                    continue;
                };
                for limit in [120, 400, 3600] {
                    let mut s = Search::new(&net);
                    route(&net, &g, &o, limit, &mut s);
                    let first = s.vertex_time.clone();
                    let mut best = s.edge_time.clone();
                    continue_on_foot(&net, &g, &foot, limit, &mut s);
                    let mut changed = true;
                    while changed {
                        changed = false;
                        for e in 0..net.n_edges() {
                            let t = best[e];
                            if t == i32::MAX {
                                continue;
                            }
                            let e_link = net.edge_flags[e] & LINK != 0;
                            for a in foot.arcs(&net, net.edge_to(e)) {
                                if a.step < 0 || (a.link && e_link) {
                                    continue;
                                }
                                let mut t1 = t + a.step;
                                if t1 == t {
                                    t1 += 1;
                                }
                                if t1 < limit && t1 < best[a.edge as usize] {
                                    best[a.edge as usize] = t1;
                                    changed = true;
                                }
                            }
                        }
                    }
                    let mut want = vec![i32::MAX; net.n_vertices()];
                    for (e, &t) in best.iter().enumerate() {
                        let v = net.edge_to(e);
                        want[v] = want[v].min(t);
                    }
                    assert_eq!(s.vertex_time, want, "{mode:?} origin {k} limit {limit}");
                    assert!(first.iter().zip(&want).all(|(a, b)| b <= a));
                    checked += 1;
                    improved += usize::from(first != want);
                }
            }
        }
        assert!(checked > 100, "{checked}");
        // walking reaches footways and goes against one-way streets
        assert!(improved > checked / 5, "{improved} of {checked}");
    }

    /// Diagnostic, not a test: the car search's time at every vertex from
    /// one origin, as `vertex,seconds`, for comparison with R5's
    /// `StreetRouter.getReachedVertices` on the same network.
    /// `PEABIRU_DUMP=dir,lat,lon,limit_s,out.csv cargo test --release dump_car -- --ignored`
    #[test]
    #[ignore]
    fn dump_car_vertex_times() {
        let spec = std::env::var("PEABIRU_DUMP").expect("PEABIRU_DUMP");
        let a: Vec<&str> = spec.split(',').collect();
        let (net, _) = crate::build_from_dir(a[0], &crate::BuildOptions::default()).unwrap();
        let (lat, lon): (f64, f64) = (a[1].parse().unwrap(), a[2].parse().unwrap());
        let g = ModeGraph::with_speeds(&net, Mode::Car, 3.6, 12.0, 2);
        let o = find_split(&net, lat, lon, Mode::Car, &mut Vec::new()).unwrap();
        let mut s = Search::new(&net);
        route(&net, &g, &o, a[3].parse().unwrap(), &mut s);
        let mut out = String::from("vertex,seconds\n");
        for (v, &t) in s.vertex_time.iter().enumerate() {
            if t < i32::MAX {
                out.push_str(&format!("{v},{t}\n"));
            }
        }
        std::fs::write(a[4], out).unwrap();
        // the incoming edges of the vertices listed after the file name
        for v in a[5..].iter().map(|v| v.parse::<usize>().unwrap()) {
            for e in 0..net.n_edges() {
                if net.edge_to(e) == v {
                    let u = net.edge_from(e);
                    eprintln!(
                        "v {v} t {} <- e {e} from {u} ({},{}) t {} flags {:#x} len {} speed {} in {} out {} restr_from {:?}",
                        s.vertex_time[v],
                        net.v_lat[u],
                        net.v_lon[u],
                        s.vertex_time[u],
                        net.edge_flags[e],
                        net.pair_length_mm[e / 2],
                        net.edge_speed_cms[e],
                        net.in_angle(e),
                        net.out_angle(e),
                        net.restrictions_starting_at(e as u32).collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}
