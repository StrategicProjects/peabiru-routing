//! Street travel-time matrix by contraction hierarchy.
//!
//! The street search of [`super::router`] is a Dijkstra over directed-edge
//! states with integer costs: leaving edge `e` by arc `a` costs
//! `max(1, a.step + turn(e, a))` seconds (the turn cost only for cars), and
//! two LINK edges in a row are not allowed. Without turn restrictions that
//! is a plain shortest-path problem, so a contraction hierarchy (CH) gives
//! the same times exactly:
//!
//! * cars search the edge graph itself (one node per directed edge, one arc
//!   per allowed turn), because the turn cost depends on the pair of edges;
//! * walking and cycling costs do not depend on the edge arrived by, so they
//!   search the vertices, with a second node `v'` for a vertex reached by a
//!   LINK edge, from which only non-LINK edges leave (the R5 rule against
//!   two connectors in a row).
//!
//! The query is RPHAST: the hierarchy restricted to what the destinations'
//! vertices can be reached from, an upward search from each origin, then a
//! sweep of the restricted nodes from the highest rank down, several
//! origins at once.
//!
//! The plain search keeps a state only below the time limit, except the two
//! start states. Times only grow along a path (every step costs at least 1
//! second), so the vertex times it ends with are those of the unlimited
//! search where below the limit, and otherwise only the start state that
//! ends at that vertex (see [`vertex_time`]).
//!
//! A hierarchy depends only on its graph, so the network keeps the ones it
//! builds ([`Cache`]): the car one, which depends on nothing but the
//! network, and a few for walking and cycling, whose graphs also depend on
//! the speeds (and, cycling, on `max_lts`). A network's arrays do not
//! change once anything can route on it (see the invariant on
//! [`StreetNetwork`]'s fields), so a kept hierarchy is never stale; the
//! one function that changes a built network, `TransitLayer::link_stops`,
//! drops the kept hierarchies all the same.

use super::link::{find_split, Split};
use super::router::{
    car_turn_cost, start_states, time_to_point, worker_threads, Mode, ModeGraph, RouteParams,
    Search, StreetEngine,
};
use super::StreetNetwork;
use crate::{CancelToken, RouteError};
use contract::Hierarchy;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::{Arc, Mutex, MutexGuard};

mod contract;

/// No node, or no time.
const NONE: u32 = u32::MAX;

/// Origins swept together.
const LANES: usize = 8;

/// Hierarchies kept for walking and cycling (different speeds or
/// `max_lts`), besides the one for cars.
const KEPT_OTHER: usize = 3;

/// What a mode's graph depends on besides the network: the speeds as the
/// search holds them (`f32` bits) and, cycling, `max_lts`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Key {
    Car,
    Walk { walk: u32 },
    Bicycle { walk: u32, bike: u32, max_lts: i32 },
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ms = f32::from_bits;
        match *self {
            Key::Car => write!(f, "car"),
            Key::Walk { walk } => write!(f, "walk {} m/s", ms(walk)),
            Key::Bicycle {
                walk,
                bike,
                max_lts,
            } => write!(
                f,
                "bicycle {} m/s, walk {} m/s, max_lts {max_lts}",
                ms(bike),
                ms(walk)
            ),
        }
    }
}

impl Key {
    fn of(g: &ModeGraph, max_lts: i32) -> Self {
        match g.mode {
            Mode::Car => Key::Car,
            Mode::Walk => Key::Walk {
                walk: g.sp.walk.to_bits(),
            },
            Mode::Bicycle => Key::Bicycle {
                walk: g.sp.walk.to_bits(),
                bike: g.sp.bike.to_bits(),
                max_lts,
            },
        }
    }
}

/// A hierarchy, and how origins and destinations map onto its nodes.
struct Entry {
    key: Key,
    /// The graph contracted, its arcs dropped once contracted.
    problem: Problem,
    n_arcs: usize,
    h: Hierarchy,
    /// The network's, to choose between this and the plain search.
    extent: Extent,
}

impl Entry {
    /// Contract `problem`; `None` when cancelled first.
    fn build(
        key: Key,
        mut problem: Problem,
        extent: Extent,
        n_threads: usize,
        cancel: &CancelToken,
    ) -> Option<Self> {
        let h = Hierarchy::build(problem.n, &problem.arcs, n_threads, cancel)?;
        let n_arcs = problem.arcs.len();
        problem.arcs = Vec::new();
        Some(Entry {
            key,
            problem,
            n_arcs,
            h,
            extent,
        })
    }

    /// Memory held, in bytes.
    fn bytes(&self) -> usize {
        4 * self.problem.link_node.len() + self.h.bytes()
    }
}

/// The size of a network, for guessing how much of it one search reaches.
#[derive(Debug, Clone, Copy)]
struct Extent {
    /// Area of the bounding box of the vertices, square metres.
    area: f64,
    /// Top car speed, m/s.
    car_speed: f64,
}

impl Extent {
    fn of(net: &StreetNetwork) -> Self {
        let car_speed = net
            .edge_speed_cms
            .iter()
            .map(|&s| f64::from(s) / 100.0)
            .fold(0.0, f64::max);
        let (Some(lat0), Some(lat1), Some(lon0), Some(lon1)) = (
            net.v_lat.iter().min(),
            net.v_lat.iter().max(),
            net.v_lon.iter().min(),
            net.v_lon.iter().max(),
        ) else {
            return Extent {
                area: 0.0,
                car_speed,
            };
        };
        const M_PER_UNIT: f64 = 111_195.0e-7; // metres per 1e-7 degree of latitude
        let mid = (f64::from(*lat0) + f64::from(*lat1)) * 0.5e-7;
        let height = (f64::from(*lat1) - f64::from(*lat0)) * M_PER_UNIT;
        let width = (f64::from(*lon1) - f64::from(*lon0)) * M_PER_UNIT * mid.to_radians().cos();
        Extent {
            area: height * width,
            car_speed,
        }
    }

    /// A rough share of the network one search within `limit_s` seconds
    /// reaches, at most 1: a circle of the farthest the mode can go against
    /// the bounding box.
    fn share(&self, g: &ModeGraph, limit_s: f64) -> f64 {
        let speed = match g.mode {
            Mode::Walk => f64::from(g.sp.walk),
            Mode::Bicycle => f64::from(g.sp.bike.max(g.sp.walk)),
            Mode::Car => self.car_speed,
        };
        let r = speed * limit_s;
        let share = std::f64::consts::PI * r * r / self.area;
        if share.is_finite() {
            share.min(1.0)
        } else {
            1.0
        }
    }
}

/// What a [`Cache`] holds: the hierarchies, most recently used first.
#[derive(Default)]
struct Kept {
    entries: Vec<Arc<Entry>>,
}

/// The hierarchies a [`StreetNetwork`] keeps. They are built from the
/// network's arrays, which never change once the network can be routed on
/// (see the invariant on [`StreetNetwork`]'s fields), so a kept hierarchy
/// stays valid for the network's whole life. Clones share the hierarchies,
/// which never change once built.
#[derive(Default)]
pub(crate) struct Cache(Mutex<Kept>);

impl Cache {
    fn lock(&self) -> MutexGuard<'_, Kept> {
        // nothing panics while holding the lock, but a poisoned list is
        // still a valid one
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The hierarchy kept for `key`, marked the most recently used.
    fn get(&self, key: Key) -> Option<Arc<Entry>> {
        let mut k = self.lock();
        let i = k.entries.iter().position(|e| e.key == key)?;
        let e = k.entries.remove(i);
        k.entries.insert(0, Arc::clone(&e));
        Some(e)
    }

    /// Keep `e` in place of any other for its key, dropping the least
    /// recently used walking and cycling ones beyond [`KEPT_OTHER`].
    fn put(&self, e: Arc<Entry>) {
        let mut k = self.lock();
        k.entries.retain(|x| x.key != e.key);
        k.entries.insert(0, e);
        let mut other = 0;
        k.entries.retain(|x| {
            if x.key == Key::Car {
                return true;
            }
            other += 1;
            other <= KEPT_OTHER
        });
    }

    #[cfg(test)]
    fn keys(&self) -> Vec<Key> {
        self.lock().entries.iter().map(|e| e.key).collect()
    }
}

impl Clone for Cache {
    fn clone(&self) -> Self {
        let k = self.lock();
        Cache(Mutex::new(Kept {
            entries: k.entries.clone(),
        }))
    }
}

impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let k = self.lock();
        let mut l = f.debug_list();
        for e in &k.entries {
            l.entry(&format_args!(
                "{:?}: {} nodes, {} arcs, {} shortcuts, {} rounds, {} bytes",
                e.key,
                e.h.n(),
                e.n_arcs,
                e.h.shortcuts,
                e.h.rounds,
                e.bytes()
            ));
        }
        l.finish()
    }
}

/// Whether to compute a matrix by hierarchy, given whether one is kept for
/// its graph and the rough `share` of the network one search reaches
/// ([`Extent::share`]).
///
/// One is built only for enough origins to pay for the contraction in that
/// call. On the Porto Alegre sample, where a search reaches most of the
/// network, the plain search costs about as much from 60 (cycling) to 310
/// (car) origins at one thread, and from 140 to 600 at four, where it gains
/// more from the threads than the contraction does; the thresholds follow
/// that. A search that reaches a smaller share costs that much less, while
/// the contraction does not, so the thresholds grow by `1 / share`.
///
/// A kept one costs a query only, a fixed part (a few tenths of a
/// millisecond on the whole of Porto Alegre) plus a sweep per eight
/// origins, and is faster than the plain search unless the searches are
/// very short: a few origins, or a few minutes on foot or by bike on a
/// large network (on the whole city, all 1,536 points with a 10-minute
/// walk took 1.3 to 1.5 times as long). It is used when a search reaches
/// at least 5% of the network, or the origins together twice the network.
fn wanted(
    engine: StreetEngine,
    mode: Mode,
    kept: bool,
    n_origins: usize,
    n_threads: usize,
    share: impl FnOnce() -> f64,
) -> bool {
    match engine {
        StreetEngine::Search => false,
        StreetEngine::Hierarchy => true,
        StreetEngine::Auto => {
            let share = share();
            if kept {
                return share >= 0.05 || n_origins as f64 * share >= 2.0;
            }
            let threads = worker_threads(n_threads, n_origins) as f64;
            let at_one_thread = if mode == Mode::Car { 320.0 } else { 150.0 };
            n_origins as f64 * share >= at_one_thread * threads.sqrt()
        }
    }
}

/// The searched graph: nodes, arcs `(from, to, seconds)`, and how origins
/// and destinations map onto nodes.
struct Problem {
    n: usize,
    arcs: Vec<(u32, u32, u32)>,
    /// Vertex mode: the LINK copy `v'` of each vertex, or `NONE`.
    link_node: Vec<u32>,
    car: bool,
}

impl Problem {
    fn new(net: &StreetNetwork, g: &ModeGraph) -> Self {
        if g.mode == Mode::Car {
            Self::edges(net, g)
        } else {
            Self::vertices(net, g)
        }
    }

    /// One node per directed edge; an arc per turn the search may take.
    fn edges(net: &StreetNetwork, g: &ModeGraph) -> Self {
        let mut arcs = Vec::new();
        for e in 0..net.n_edges() {
            let e_link = net.edge_flags[e] & super::flags::LINK != 0;
            let out_deg = net.out_angle(e);
            for a in g.arcs(net, net.edge_to(e)) {
                if a.step < 0 || (a.link && e_link) {
                    continue;
                }
                let w = (a.step + car_turn_cost(out_deg, i32::from(a.in_deg))).max(1);
                arcs.push((e as u32, a.edge, w as u32));
            }
        }
        Problem {
            n: net.n_edges(),
            arcs,
            link_node: Vec::new(),
            car: true,
        }
    }

    /// One node per vertex, plus `v'` for a vertex some usable LINK edge
    /// enters: LINK edges lead to `v'`, and only non-LINK edges leave it.
    fn vertices(net: &StreetNetwork, g: &ModeGraph) -> Self {
        let nv = net.n_vertices();
        let mut link_node = vec![NONE; nv];
        let mut n = nv;
        for v in 0..nv {
            for a in g.arcs(net, v) {
                if a.step >= 0 && a.link && link_node[a.to as usize] == NONE {
                    link_node[a.to as usize] = n as u32;
                    n += 1;
                }
            }
        }
        let mut arcs = Vec::new();
        for v in 0..nv {
            for a in g.arcs(net, v) {
                if a.step < 0 {
                    continue;
                }
                let w = a.step.max(1) as u32;
                let to = if a.link {
                    link_node[a.to as usize]
                } else {
                    a.to
                };
                arcs.push((v as u32, to, w));
                if !a.link && link_node[v] != NONE {
                    arcs.push((link_node[v], to, w));
                }
            }
        }
        Problem {
            n,
            arcs,
            link_node,
            car: false,
        }
    }

    /// The node holding the time at the end of edge `e`.
    fn node_of_edge(&self, net: &StreetNetwork, e: usize) -> u32 {
        if self.car {
            return e as u32;
        }
        let v = net.edge_to(e);
        if net.edge_flags[e] & super::flags::LINK != 0 && self.link_node[v] != NONE {
            self.link_node[v]
        } else {
            v as u32
        }
    }

    /// The nodes whose least time is a vertex's time.
    fn vertex_nodes(&self, net: &StreetNetwork, v: usize, out: &mut Vec<u32>) {
        if self.car {
            out.extend_from_slice(net.in_edges(v));
        } else {
            out.push(v as u32);
            if self.link_node[v] != NONE {
                out.push(self.link_node[v]);
            }
        }
    }
}

/// The hierarchy restricted to the nodes some destination node can be
/// reached from going down (RPHAST target selection), numbered from 0 in
/// sweep order.
struct Restricted {
    /// Hierarchy id to restricted index, or `NONE`.
    local: Vec<u32>,
    off: Vec<u32>,
    /// Downward arcs into each restricted node: `(restricted from, seconds)`.
    arcs: Vec<(u32, u32)>,
}

impl Restricted {
    fn new(h: &Hierarchy, seeds: &[u32]) -> Self {
        let n = h.n();
        let mut mark = vec![false; n];
        let mut stack: Vec<u32> = Vec::new();
        for &s in seeds {
            if !mark[s as usize] {
                mark[s as usize] = true;
                stack.push(s);
            }
        }
        while let Some(v) = stack.pop() {
            let v = v as usize;
            for &(u, _) in &h.down[h.down_off[v] as usize..h.down_off[v + 1] as usize] {
                if !mark[u as usize] {
                    mark[u as usize] = true;
                    stack.push(u);
                }
            }
        }
        let mut local = vec![NONE; n];
        let mut m = 0u32;
        for (i, &k) in mark.iter().enumerate() {
            if k {
                local[i] = m;
                m += 1;
            }
        }
        let mut off = Vec::with_capacity(m as usize + 1);
        off.push(0);
        let mut arcs = Vec::new();
        for (v, &k) in mark.iter().enumerate() {
            if k {
                for &(u, w) in &h.down[h.down_off[v] as usize..h.down_off[v + 1] as usize] {
                    arcs.push((local[u as usize], w));
                }
                off.push(arcs.len() as u32);
            }
        }
        Restricted { local, off, arcs }
    }

    fn m(&self) -> usize {
        self.off.len() - 1
    }
}

/// Per-thread query scratch.
struct Query {
    /// Upward search: time per hierarchy id.
    dist: Vec<u32>,
    touched: Vec<u32>,
    heap: BinaryHeap<Reverse<(u32, u32)>>,
    /// Sweep: time per restricted node, one lane per origin.
    lanes: Vec<[u32; LANES]>,
}

impl Query {
    fn new(h: &Hierarchy, r: &Restricted) -> Self {
        Query {
            dist: vec![NONE; h.n()],
            touched: Vec::new(),
            heap: BinaryHeap::new(),
            lanes: vec![[NONE; LANES]; r.m()],
        }
    }

    /// Upward search from the start nodes (hierarchy ids), states kept
    /// below `limit` only; writes the times reached into `lane`.
    fn upward(
        &mut self,
        h: &Hierarchy,
        r: &Restricted,
        starts: &[(u32, u32)],
        limit: u32,
        lane: usize,
    ) {
        for &v in &self.touched {
            self.dist[v as usize] = NONE;
        }
        self.touched.clear();
        self.heap.clear();
        for &(s, t) in starts {
            if t < limit && t < self.dist[s as usize] {
                if self.dist[s as usize] == NONE {
                    self.touched.push(s);
                }
                self.dist[s as usize] = t;
                self.heap.push(Reverse((t, s)));
            }
        }
        while let Some(Reverse((d, u))) = self.heap.pop() {
            let uu = u as usize;
            if d > self.dist[uu] {
                continue;
            }
            let l = r.local[uu];
            if l != NONE {
                self.lanes[l as usize][lane] = d;
            }
            for &(v, w) in &h.up[h.up_off[uu] as usize..h.up_off[uu + 1] as usize] {
                let nd = d.saturating_add(w);
                if nd < limit && nd < self.dist[v as usize] {
                    if self.dist[v as usize] == NONE {
                        self.touched.push(v);
                    }
                    self.dist[v as usize] = nd;
                    self.heap.push(Reverse((nd, v)));
                }
            }
        }
    }

    /// Down the restricted hierarchy, highest rank first: every node takes
    /// the least of its own time and those of the arcs into it.
    fn sweep(&mut self, r: &Restricted) {
        for i in 0..r.m() {
            let mut d = self.lanes[i];
            for &(j, w) in &r.arcs[r.off[i] as usize..r.off[i + 1] as usize] {
                let dj = self.lanes[j as usize];
                for k in 0..LANES {
                    d[k] = d[k].min(dj[k].saturating_add(w));
                }
            }
            self.lanes[i] = d;
        }
    }
}

/// The plain search's two start states from an origin split.
type Starts = [(usize, i32); 2];

/// A vertex's time as the plain search leaves it: `best`, the least time
/// of the nodes ending at it in the unlimited search, when below `limit`;
/// otherwise only a start state ending at it, whose time is never checked
/// against the limit (and which nothing below the limit improved).
fn vertex_time(net: &StreetNetwork, v: usize, best: u32, limit: u32, starts: &Starts) -> i32 {
    let mut t = if best < limit { best as i32 } else { i32::MAX };
    for &(e, s) in starts {
        if net.edge_to(e) == v {
            t = t.min(s);
        }
    }
    t
}

/// A hierarchy restricted to the times of some vertices.
struct Prepared<'a> {
    net: &'a StreetNetwork,
    g: &'a ModeGraph,
    e: &'a Entry,
    r: Restricted,
    /// The target vertices, and the restricted nodes holding each one's
    /// time: `local[node_off[i]..node_off[i + 1]]` for `vertices[i]`.
    vertices: Vec<u32>,
    node_off: Vec<u32>,
    local: Vec<u32>,
}

impl<'a> Prepared<'a> {
    fn new(net: &'a StreetNetwork, g: &'a ModeGraph, e: &'a Entry, mut vertices: Vec<u32>) -> Self {
        vertices.sort_unstable();
        vertices.dedup();
        let mut node_off = vec![0u32];
        let mut nodes = Vec::new();
        for &v in &vertices {
            e.problem.vertex_nodes(net, v as usize, &mut nodes);
            node_off.push(nodes.len() as u32);
        }
        let seeds: Vec<u32> = nodes.iter().map(|&x| e.h.id[x as usize]).collect();
        let r = Restricted::new(&e.h, &seeds);
        let local = seeds.iter().map(|&s| r.local[s as usize]).collect();
        Prepared {
            net,
            g,
            e,
            r,
            vertices,
            node_off,
            local,
        }
    }

    /// Search from up to [`LANES`] origins at once; afterwards
    /// [`Prepared::time`] reads each one's vertex times.
    fn search(
        &self,
        q: &mut Query,
        origins: &[Option<Split>],
        limit: u32,
    ) -> [Option<Starts>; LANES] {
        let mut starts = [None; LANES];
        for lane in q.lanes.iter_mut() {
            *lane = [NONE; LANES];
        }
        for (k, o) in origins.iter().enumerate() {
            let Some(o) = o else { continue };
            let st = start_states(self.net, self.g, o);
            // never negative: lengths and speeds are not
            debug_assert!(st.iter().all(|&(_, t)| t >= 0));
            let nodes = st.map(|(e, t)| {
                (
                    self.e.h.id[self.e.problem.node_of_edge(self.net, e) as usize],
                    t as u32,
                )
            });
            q.upward(&self.e.h, &self.r, &nodes, limit, k);
            starts[k] = Some(st);
        }
        q.sweep(&self.r);
        starts
    }

    /// Time at target vertex `i` from the origin in `lane`, as the plain
    /// search would leave it in `vertex_time`.
    fn time(&self, q: &Query, lane: usize, i: usize, limit: u32, starts: &Starts) -> i32 {
        let mut best = NONE;
        for &l in &self.local[self.node_off[i] as usize..self.node_off[i + 1] as usize] {
            best = best.min(q.lanes[l as usize][lane]);
        }
        vertex_time(self.net, self.vertices[i] as usize, best, limit, starts)
    }
}

/// The hierarchy for `g` kept with the network or, when there is none
/// and `wanted` says so, a new one, kept for later calls. `Ok(None)`:
/// compute this matrix by plain search.
fn hierarchy(
    net: &StreetNetwork,
    g: &ModeGraph,
    n_origins: usize,
    params: &RouteParams,
) -> Result<Option<Arc<Entry>>, RouteError> {
    // R5's turn-restriction handling depends on the order equal times
    // leave its queue: that search stays as it is.
    if g.java_order || params.engine == StreetEngine::Search {
        return Ok(None);
    }
    let key = Key::of(g, params.max_lts);
    let kept = net.hierarchies.get(key);
    let mut extent = kept.as_ref().map(|e| e.extent);
    let limit_s = f64::from(params.max_trip_duration_min) * 60.0;
    let use_it = wanted(
        params.engine,
        g.mode,
        kept.is_some(),
        n_origins,
        params.n_threads,
        || {
            extent
                .get_or_insert_with(|| Extent::of(net))
                .share(g, limit_s)
        },
    );
    if kept.is_some() || !use_it {
        return Ok(kept.filter(|_| use_it));
    }
    let threads = worker_threads(params.n_threads, usize::MAX);
    let problem = Problem::new(net, g);
    let extent = extent.unwrap_or_else(|| Extent::of(net));
    let e =
        Entry::build(key, problem, extent, threads, &params.cancel).ok_or(RouteError::Cancelled)?;
    let e = Arc::new(e);
    net.hierarchies.put(Arc::clone(&e));
    Ok(Some(e))
}

/// The street travel-time matrix of
/// [`super::router::street_travel_time_matrix`]
/// by contraction hierarchy, or `None` when the plain search should
/// compute it (see [`wanted`]).
pub(super) fn travel_time_matrix(
    net: &StreetNetwork,
    g: &ModeGraph,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    params: &RouteParams,
) -> Option<Result<Vec<Option<u32>>, RouteError>> {
    let nd = destinations.len();
    if origins.is_empty() || nd == 0 {
        return None; // nothing to compute
    }
    let e = match hierarchy(net, g, origins.len(), params) {
        Ok(Some(e)) => e,
        Ok(None) => return None,
        Err(err) => return Some(Err(err)),
    };
    Some(matrix(net, g, &e, origins, destinations, params))
}

/// The matrix by the hierarchy `e` of `g`.
fn matrix(
    net: &StreetNetwork,
    g: &ModeGraph,
    e: &Entry,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    params: &RouteParams,
) -> Result<Vec<Option<u32>>, RouteError> {
    let mode = g.mode;
    let m = params.max_trip_duration_min;
    let limit = (m * 60) as u32;
    let nd = destinations.len();
    let mut out = vec![None; origins.len() * nd];
    let cancel = &params.cancel;
    let mut scratch = Vec::new();
    let dest_splits: Vec<Option<Split>> = destinations
        .iter()
        .map(|&(lat, lon)| find_split(net, lat, lon, mode, &mut scratch))
        .collect();
    let vertices = dest_splits
        .iter()
        .flatten()
        .flat_map(|d| [d.vertex0, d.vertex1])
        .collect();
    let p = Prepared::new(net, g, e, vertices);
    cancel.check()?;
    let n_threads = worker_threads(params.n_threads, origins.len().div_ceil(LANES));
    // Each worker takes the next LANES origins' rows of `out`.
    let batches = Mutex::new(out.chunks_mut(nd * LANES).enumerate());
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..n_threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut q = Query::new(&p.e.h, &p.r);
                    let mut search = Search::new(net);
                    let mut scratch = Vec::new();
                    let mut splits = [None; LANES];
                    while !cancel.is_cancelled() {
                        let next = batches.lock().expect("batch queue").next();
                        let Some((b, rows)) = next else { break };
                        for (k, s) in splits.iter_mut().enumerate() {
                            *s = origins.get(b * LANES + k).and_then(|&(lat, lon)| {
                                find_split(net, lat, lon, mode, &mut scratch)
                            });
                        }
                        let starts = p.search(&mut q, &splits, limit);
                        for (k, row) in rows.chunks_mut(nd).enumerate() {
                            let (Some(o), Some(st)) = (&splits[k], &starts[k]) else {
                                continue;
                            };
                            // The destination times read `vertex_time` as
                            // after a plain search from this origin.
                            for (i, &v) in p.vertices.iter().enumerate() {
                                search.vertex_time[v as usize] = p.time(&q, k, i, limit, st);
                            }
                            for (j, d) in dest_splits.iter().enumerate() {
                                let Some(d) = d else { continue };
                                if let Some(secs) = time_to_point(net, g, o, d, &search) {
                                    let minutes = secs / 60;
                                    if minutes <= i64::from(m) {
                                        row[j] = Some(minutes as u32);
                                    }
                                }
                            }
                            for &v in &p.vertices {
                                search.vertex_time[v as usize] = i32::MAX;
                            }
                        }
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("routing thread panicked");
        }
    });
    cancel.check()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::street::router::{route, Speeds};
    use crate::street::{build_network, BuildOptions};

    const SPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../r/inst/extdata/spo");

    /// Deterministic pseudo-random numbers (xorshift).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        /// Uniform in `[a, b)`.
        fn uniform(&mut self, a: f64, b: f64) -> f64 {
            a + (b - a) * (self.next() >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// The sample's own points, `(lat, lon)`.
    fn sample_points() -> Vec<(f64, f64)> {
        let csv = std::fs::read_to_string(format!("{SPO}/spo_points.csv")).unwrap();
        csv.lines()
            .skip(1)
            .map(|l| {
                let f: Vec<&str> = l.split(',').collect();
                (f[2].parse().unwrap(), f[1].parse().unwrap())
            })
            .collect()
    }

    /// Points at random over the network's bounding box, a few beyond it.
    fn random_points(net: &StreetNetwork, n: usize, seed: u64) -> Vec<(f64, f64)> {
        let deg = |v: &[i32], f: fn(&i32, &i32) -> bool| {
            let mut x = v[0];
            for &y in v {
                if f(&y, &x) {
                    x = y;
                }
            }
            f64::from(x) * 1e-7
        };
        let (lat0, lat1) = (deg(&net.v_lat, |a, b| a < b), deg(&net.v_lat, |a, b| a > b));
        let (lon0, lon1) = (deg(&net.v_lon, |a, b| a < b), deg(&net.v_lon, |a, b| a > b));
        let mut rng = Rng(seed);
        (0..n)
            .map(|_| {
                (
                    rng.uniform(lat0 - 0.01, lat1 + 0.01),
                    rng.uniform(lon0 - 0.01, lon1 + 0.01),
                )
            })
            .collect()
    }

    fn params(walk: f64, bike: f64, max_lts: i32, minutes: i32, n_threads: usize) -> RouteParams {
        RouteParams {
            walk_speed_kmh: walk,
            bike_speed_kmh: bike,
            max_lts,
            max_trip_duration_min: minutes,
            n_threads,
            ..RouteParams::default()
        }
    }

    /// The parameter sets the tests run: every mode, several speeds, every
    /// `max_lts`.
    fn settings() -> Vec<(Mode, RouteParams)> {
        let mut v = Vec::new();
        for (walk, bike) in [(3.6, 12.0), (5.0, 18.0), (0.9, 4.0)] {
            v.push((Mode::Walk, params(walk, bike, 2, 120, 1)));
            for lts in 1..=4 {
                v.push((Mode::Bicycle, params(walk, bike, lts, 120, 1)));
            }
        }
        v.push((Mode::Car, params(3.6, 12.0, 2, 120, 1)));
        v.push((Mode::Car, params(1.2, 12.0, 2, 120, 1)));
        v
    }

    /// From random origins, the time at every vertex of the network equals
    /// the plain search's, in seconds, for time limits from 1 second up:
    /// on the street network alone and with the transit stops' LINK edges.
    #[test]
    fn vertex_times_match_the_plain_search() {
        let (with_stops, _) =
            crate::transit::build_from_dir(SPO, &BuildOptions::default()).unwrap();
        let alone = build_network(format!("{SPO}/spo_osm.pbf"), &BuildOptions::default()).unwrap();
        let mut compared = 0u64;
        let mut reached = 0u64;
        let mut link_nodes = 0;
        for (ni, net) in [&alone, &with_stops].into_iter().enumerate() {
            let all: Vec<u32> = (0..net.n_vertices() as u32).collect();
            // over the streets: the stops spread far beyond them
            let points = random_points(&alone, 48, 0x5eed + ni as u64);
            let mut search = Search::new(net);
            for (si, (mode, p)) in settings().into_iter().enumerate() {
                let g = ModeGraph::new(net, mode, Speeds::new(&p, mode), p.max_lts);
                assert!(!g.java_order);
                let e = Entry::build(
                    Key::of(&g, p.max_lts),
                    Problem::new(net, &g),
                    Extent::of(net),
                    1 + si % 3,
                    &CancelToken::new(),
                )
                .unwrap();
                let prep = Prepared::new(net, &g, &e, all.clone());
                if mode != Mode::Car {
                    link_nodes += e.problem.n - net.n_vertices();
                }
                let mut q = Query::new(&e.h, &prep.r);
                let mut scratch = Vec::new();
                let splits: Vec<Option<Split>> = points
                    .iter()
                    .map(|&(lat, lon)| find_split(net, lat, lon, mode, &mut scratch))
                    .collect();
                assert!(
                    splits.iter().flatten().count() > 12,
                    "{mode:?}: {}",
                    splits.iter().flatten().count()
                );
                for limit in [1u32, 45, 300, 1200, 7200, i32::MAX as u32] {
                    for chunk in splits.chunks(LANES) {
                        let starts = prep.search(&mut q, chunk, limit);
                        for (k, o) in chunk.iter().enumerate() {
                            let Some(o) = o else { continue };
                            let st = starts[k].as_ref().unwrap();
                            route(net, &g, o, limit as i32, &mut search);
                            for (i, &v) in prep.vertices.iter().enumerate() {
                                let want = search.vertex_time()[v as usize];
                                let got = prep.time(&q, k, i, limit, st);
                                assert_eq!(
                                    got, want,
                                    "net {ni} {mode:?} {p:?} limit {limit} vertex {v}"
                                );
                                compared += 1;
                                reached += u64::from(want != i32::MAX);
                            }
                        }
                    }
                }
            }
        }
        // the LINK rule was exercised, and both reached and unreached
        // vertices were compared
        assert!(link_nodes > 0);
        assert!(
            reached > compared / 4 && reached < compared,
            "{reached} of {compared}"
        );
    }

    /// The start states mirror those of the plain search: from an origin
    /// on a long edge with a 1-second limit, the plain search keeps only
    /// them, so the vertex times at the edge's ends are theirs.
    #[test]
    fn start_states_match_the_plain_search() {
        let net = build_network(format!("{SPO}/spo_osm.pbf"), &BuildOptions::default()).unwrap();
        let mut search = Search::new(&net);
        let mut scratch = Vec::new();
        let mut checked = 0;
        for (mode, p) in settings() {
            let g = ModeGraph::new(&net, mode, Speeds::new(&p, mode), p.max_lts);
            for (lat, lon) in random_points(&net, 30, 7) {
                let Some(o) = find_split(&net, lat, lon, mode, &mut scratch) else {
                    continue;
                };
                route(&net, &g, &o, 1, &mut search);
                let st = start_states(&net, &g, &o);
                for (e, _) in st {
                    let v = net.edge_to(e);
                    // both ends are the same vertex on a loop
                    let want = st
                        .iter()
                        .filter(|s| net.edge_to(s.0) == v)
                        .map(|s| s.1)
                        .min();
                    assert_eq!(Some(search.vertex_time()[v]), want, "{mode:?} {o:?}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 200);
    }

    /// `p` with another engine.
    fn with(p: &RouteParams, engine: StreetEngine) -> RouteParams {
        RouteParams {
            engine,
            ..p.clone()
        }
    }

    /// The public matrix.
    fn matrix_of(
        net: &StreetNetwork,
        o: &[(f64, f64)],
        d: &[(f64, f64)],
        mode: Mode,
        p: &RouteParams,
    ) -> Result<Vec<Option<u32>>, RouteError> {
        crate::street::router::street_travel_time_matrix(net, o, d, mode, p).map(|m| m.into_vec())
    }

    /// The matrix by the plain search.
    fn plain(
        net: &StreetNetwork,
        o: &[(f64, f64)],
        d: &[(f64, f64)],
        mode: Mode,
        p: &RouteParams,
    ) -> Vec<Option<u32>> {
        matrix_of(net, o, d, mode, &with(p, StreetEngine::Search)).unwrap()
    }

    fn spo() -> StreetNetwork {
        build_network(format!("{SPO}/spo_osm.pbf"), &BuildOptions::default()).unwrap()
    }

    /// The whole matrix, minutes and empty cells, equals the plain
    /// search's: the sample's points and random ones, origins apart from
    /// destinations, every mode and setting, limits from 1 minute, 1 and
    /// 3 threads. All on one network, twice over, so the kept hierarchies
    /// are reused, and the walking and cycling ones dropped and built
    /// again as the settings alternate.
    #[test]
    fn matrix_matches_the_plain_search() {
        let net = spo();
        let mut origins = sample_points();
        origins.extend(random_points(&net, 40, 11));
        let mut destinations = random_points(&net, 60, 13);
        destinations.extend(sample_points().into_iter().step_by(2));
        let mut cells = 0usize;
        let mut filled = 0usize;
        for _pass in 0..2 {
            for (mode, p) in settings() {
                let g = ModeGraph::new(&net, mode, Speeds::new(&p, mode), p.max_lts);
                for minutes in [1, 4, 15, 120] {
                    for n_threads in [1, 3] {
                        let p = RouteParams {
                            max_trip_duration_min: minutes,
                            n_threads,
                            engine: StreetEngine::Hierarchy,
                            ..p.clone()
                        };
                        let got = matrix_of(&net, &origins, &destinations, mode, &p).unwrap();
                        let want = plain(&net, &origins, &destinations, mode, &p);
                        assert_eq!(got, want, "{mode:?} {p:?}");
                        cells += got.len();
                        filled += got.iter().filter(|c| c.is_some()).count();
                        let keys = net.hierarchies.keys();
                        assert_eq!(keys[0], Key::of(&g, p.max_lts));
                        assert!(keys.iter().filter(|&&k| k != Key::Car).count() <= KEPT_OTHER);
                    }
                }
            }
        }
        assert!(filled > cells / 8 && filled < cells, "{filled} of {cells}");
    }

    /// Once kept, a hierarchy serves later calls with other origins,
    /// destinations, limits and thread counts, small ones included,
    /// without being built again; a clone of the network shares it.
    #[test]
    fn kept_hierarchy_is_reused() {
        let net = spo();
        let p0 = RouteParams {
            n_threads: 1,
            ..RouteParams::default()
        };
        let g = ModeGraph::new(&net, Mode::Car, Speeds::new(&p0, Mode::Car), p0.max_lts);
        // a few origins: the search, and nothing kept
        let (few, dests) = (random_points(&net, 5, 3), random_points(&net, 30, 5));
        let got = matrix_of(&net, &few, &dests, Mode::Car, &p0).unwrap();
        assert_eq!(got, plain(&net, &few, &dests, Mode::Car, &p0));
        assert!(net.hierarchies.keys().is_empty());
        let forced = with(&p0, StreetEngine::Hierarchy);
        let first = hierarchy(&net, &g, 1, &forced).unwrap().unwrap();
        assert_eq!(net.hierarchies.keys(), [Key::Car]);
        for (i, &(minutes, n_threads)) in [(1, 1), (10, 2), (120, 4), (30, 0), (120, 1)]
            .iter()
            .enumerate()
        {
            let p = RouteParams {
                max_trip_duration_min: minutes,
                n_threads,
                ..p0.clone()
            };
            let o = random_points(&net, 1 + 9 * i, 100 + i as u64);
            let d = random_points(&net, 1 + 25 * i, 200 + i as u64);
            let got = matrix_of(&net, &o, &d, Mode::Car, &p).unwrap();
            assert_eq!(got, plain(&net, &o, &d, Mode::Car, &p), "{p:?}");
            let again = hierarchy(&net, &g, o.len(), &p).unwrap().unwrap();
            assert!(Arc::ptr_eq(&first, &again));
        }
        let copy = net.clone();
        assert!(Arc::ptr_eq(
            &first,
            &copy.hierarchies.get(Key::Car).unwrap()
        ));
        // the search when told to, kept hierarchy or not
        assert!(
            hierarchy(&net, &g, 10_000, &with(&p0, StreetEngine::Search))
                .unwrap()
                .is_none()
        );
    }

    /// A cancelled call keeps no hierarchy it did not finish, and leaves a
    /// kept one usable.
    #[test]
    fn cancelled_calls_keep_no_half_built_hierarchy() {
        let net = spo();
        let pts = sample_points();
        let p = RouteParams {
            n_threads: 2,
            engine: StreetEngine::Hierarchy,
            ..RouteParams::default()
        };
        let cancelled = |p: &RouteParams| {
            let c = CancelToken::new();
            c.cancel();
            RouteParams {
                cancel: c,
                ..p.clone()
            }
        };
        assert_eq!(
            matrix_of(&net, &pts, &pts, Mode::Car, &cancelled(&p)),
            Err(RouteError::Cancelled)
        );
        assert!(net.hierarchies.keys().is_empty());
        // cancelled from another thread while it contracts (or, on a fast
        // machine, after): either way nothing half-built is kept
        for delay_us in [0, 200, 1000, 5000] {
            let fresh = spo();
            let c = CancelToken::new();
            let q = RouteParams {
                cancel: c.clone(),
                ..p.clone()
            };
            let r = std::thread::scope(|s| {
                s.spawn(|| {
                    std::thread::sleep(std::time::Duration::from_micros(delay_us));
                    c.cancel();
                });
                matrix_of(&fresh, &pts, &pts, Mode::Car, &q)
            });
            assert!(matches!(r, Err(RouteError::Cancelled) | Ok(_)));
            let ok = matrix_of(&fresh, &pts, &pts, Mode::Car, &p).unwrap();
            assert_eq!(ok, plain(&fresh, &pts, &pts, Mode::Car, &p), "{delay_us}");
        }
        // a kept hierarchy survives a cancelled call
        let want = plain(&net, &pts, &pts, Mode::Car, &p);
        assert_eq!(matrix_of(&net, &pts, &pts, Mode::Car, &p).unwrap(), want);
        let kept = net.hierarchies.get(Key::Car).unwrap();
        let auto = with(&p, StreetEngine::Auto);
        assert_eq!(
            matrix_of(&net, &pts, &pts, Mode::Car, &cancelled(&auto)),
            Err(RouteError::Cancelled)
        );
        let now = net.hierarchies.get(Key::Car).unwrap();
        assert!(Arc::ptr_eq(&kept, &now));
        assert_eq!(matrix_of(&net, &pts, &pts, Mode::Car, &auto).unwrap(), want);
    }

    /// Kept hierarchies are not compared with the network (see the
    /// invariant on `StreetNetwork`'s fields): code that changes a
    /// network's arrays drops them with `forget_hierarchies`. A network so
    /// changed after hierarchies were kept gets the plain search's matrix
    /// of the changed network, the hierarchies built again when wanted; a
    /// clone made before the change keeps its own.
    #[test]
    fn a_changed_network_drops_its_hierarchies() {
        let mut net = spo();
        let pts = sample_points();
        let p = RouteParams {
            n_threads: 2,
            engine: StreetEngine::Hierarchy,
            ..RouteParams::default()
        };
        matrix_of(&net, &pts, &pts, Mode::Car, &p).unwrap();
        matrix_of(&net, &pts, &pts, Mode::Walk, &p).unwrap();
        let before = net.hierarchies.get(Key::Car).unwrap();
        let original = net.clone();
        for s in &mut net.edge_speed_cms {
            *s = (*s / 2).max(1);
        }
        for l in net.pair_length_mm.iter_mut().step_by(3) {
            *l += 5000;
        }
        net.forget_hierarchies();
        assert!(net.hierarchies.keys().is_empty());
        // a small call (Auto would search): no hierarchy is built
        let auto = with(&p, StreetEngine::Auto);
        let few = &pts[..3];
        let got = matrix_of(&net, few, &pts, Mode::Car, &auto).unwrap();
        assert_eq!(got, plain(&net, few, &pts, Mode::Car, &p));
        assert!(net.hierarchies.keys().is_empty());
        for mode in [Mode::Car, Mode::Walk] {
            let got = matrix_of(&net, &pts, &pts, mode, &p).unwrap();
            assert_eq!(got, plain(&net, &pts, &pts, mode, &p));
            assert_ne!(got, plain(&original, &pts, &pts, mode, &p));
        }
        let after = net.hierarchies.get(Key::Car).unwrap();
        assert!(!Arc::ptr_eq(&before, &after));
        // the unchanged clone kept its own
        let still = original.hierarchies.get(Key::Car).unwrap();
        assert!(Arc::ptr_eq(&before, &still));
        assert_eq!(original.hierarchies.keys().len(), 2);
    }

    /// The one change the crate makes to a built network, linking transit
    /// stops to it, drops the hierarchies kept with it: the matrices after
    /// it are the plain search's on the linked network, for every mode.
    #[test]
    fn linking_stops_drops_the_hierarchies() {
        use crate::transit::{gtfs_files_in_dir, TransitLayer};
        let mut net = spo();
        let pts = sample_points();
        let p = RouteParams {
            n_threads: 2,
            engine: StreetEngine::Hierarchy,
            ..RouteParams::default()
        };
        for mode in [Mode::Car, Mode::Walk, Mode::Bicycle] {
            matrix_of(&net, &pts, &pts, mode, &p).unwrap();
        }
        assert_eq!(net.hierarchies.keys().len(), 3);
        let before = net.n_vertices();
        let mut layer = TransitLayer::load(&gtfs_files_in_dir(SPO).unwrap()).unwrap();
        layer.link_stops(&mut net);
        assert!(net.n_vertices() > before, "the stops changed the network");
        assert!(net.hierarchies.keys().is_empty());
        for mode in [Mode::Car, Mode::Walk, Mode::Bicycle] {
            let got = matrix_of(&net, &pts, &pts, mode, &p).unwrap();
            assert_eq!(got, plain(&net, &pts, &pts, mode, &p), "{mode:?}");
        }
        assert_eq!(net.hierarchies.keys().len(), 3);
    }

    /// The contraction gives the same hierarchy at any thread count.
    #[test]
    fn hierarchy_does_not_depend_on_threads() {
        let net = spo();
        for mode in [Mode::Car, Mode::Walk] {
            let p = RouteParams::default();
            let g = ModeGraph::new(&net, mode, Speeds::new(&p, mode), p.max_lts);
            let build = |t| {
                let problem = Problem::new(&net, &g);
                let e = Entry::build(Key::Car, problem, Extent::of(&net), t, &CancelToken::new());
                e.unwrap().h
            };
            let one = build(1);
            for t in [2, 3, 8] {
                let h = build(t);
                assert_eq!(h.id, one.id, "{mode:?} {t}");
                assert_eq!((&h.up_off, &h.up), (&one.up_off, &one.up));
                assert_eq!((&h.down_off, &h.down), (&one.down_off, &one.down));
            }
        }
    }

    /// Which engine computes the matrix.
    #[test]
    fn dispatch() {
        use StreetEngine::*;
        assert!(wanted(Auto, Mode::Walk, false, 1000, 1, || 1.0));
        assert!(!wanted(Auto, Mode::Walk, false, 10, 1, || 1.0));
        assert!(!wanted(Auto, Mode::Car, false, 200, 1, || 1.0));
        // a search that reaches a tenth of the network is cheap
        assert!(wanted(Auto, Mode::Walk, false, 2000, 1, || 1.0));
        assert!(!wanted(Auto, Mode::Walk, false, 1000, 1, || 0.1));
        // kept: but for very short searches
        assert!(wanted(Auto, Mode::Car, true, 1, 1, || 1.0));
        assert!(wanted(Auto, Mode::Walk, true, 1, 8, || 0.05));
        assert!(wanted(Auto, Mode::Walk, true, 1000, 8, || 0.002));
        assert!(!wanted(Auto, Mode::Walk, true, 1, 8, || 0.04));
        assert!(!wanted(Auto, Mode::Walk, true, 900, 8, || 0.002));
        // told to
        let never = || -> f64 { unreachable!() };
        assert!(!wanted(Search, Mode::Car, true, 10_000, 1, never));
        assert!(wanted(Hierarchy, Mode::Car, false, 1, 4, never));
        let net = spo();
        let p = RouteParams::default();
        let extent = Extent::of(&net);
        let share = |mode, minutes: f64| {
            let g = ModeGraph::new(&net, mode, Speeds::new(&p, mode), p.max_lts);
            extent.share(&g, minutes * 60.0)
        };
        assert_eq!(share(Mode::Car, 120.0), 1.0);
        assert!(share(Mode::Walk, 1.0) < 0.01);
        assert!(share(Mode::Walk, 10.0) < share(Mode::Bicycle, 10.0));
        // cars on a network with turn restrictions always search
        let mut net = spo();
        net.restriction_starts.push((0, 0));
        let p = with(&RouteParams::default(), Hierarchy);
        let g = ModeGraph::new(&net, Mode::Car, Speeds::new(&p, Mode::Car), p.max_lts);
        assert!(g.java_order);
        assert!(hierarchy(&net, &g, 10_000, &p).unwrap().is_none());
        let g = ModeGraph::new(&net, Mode::Walk, Speeds::new(&p, Mode::Walk), p.max_lts);
        assert!(hierarchy(&net, &g, 1, &p).unwrap().is_some());
    }
}
