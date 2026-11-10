//! Street travel-time matrix by contraction hierarchy (prototype).
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

use super::link::{find_split, Split};
use super::router::{car_turn_cost, time_to_point, Mode, ModeGraph, RouteParams, Search};
use super::StreetNetwork;
use contract::Hierarchy;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Mutex;
use std::time::Instant;

mod contract;

/// No node, or no time.
const NONE: u32 = u32::MAX;

/// Origins swept together.
const LANES: usize = 8;

/// Which search computes the street travel-time matrix: the env variable
/// `PEABIRU_STREET_MATRIX` = `dijkstra`, `ch` or `auto` (the default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Engine {
    Auto,
    Dijkstra,
    Ch,
}

#[cfg(test)]
thread_local! {
    /// Tests choose the engine per thread.
    pub(crate) static ENGINE: std::cell::Cell<Option<Engine>> = const { std::cell::Cell::new(None) };
}

fn engine() -> Engine {
    #[cfg(test)]
    if let Some(e) = ENGINE.with(|e| e.get()) {
        return e;
    }
    static E: std::sync::OnceLock<Engine> = std::sync::OnceLock::new();
    *E.get_or_init(|| match std::env::var("PEABIRU_STREET_MATRIX").as_deref() {
        Ok("dijkstra") => Engine::Dijkstra,
        Ok("ch") => Engine::Ch,
        _ => Engine::Auto,
    })
}

/// Print the phases' times to stderr (`PEABIRU_CH_TIMING` set).
fn timing() -> bool {
    static T: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *T.get_or_init(|| std::env::var_os("PEABIRU_CH_TIMING").is_some())
}

/// Whether [`travel_time_matrix`] should compute this matrix: never for
/// cars on a network with turn restrictions (R5's restriction handling
/// depends on the order equal times leave the queue, so that search stays
/// as it is), and by default only for enough origins to pay for the
/// preprocessing. On the Porto Alegre sample the plain search costs about
/// as much from 60 (cycling) to 310 (car) origins at one thread, and from
/// 140 to 600 at four, where it gains more from the threads than the
/// contraction does; the thresholds below follow that.
pub(super) fn applies(g: &ModeGraph, n_origins: usize, n_threads: usize) -> bool {
    if g.java_order {
        return false;
    }
    match engine() {
        Engine::Dijkstra => false,
        Engine::Ch => true,
        Engine::Auto => {
            let threads = super::router::worker_threads(n_threads, n_origins) as f64;
            let at_one_thread = if g.mode == Mode::Car { 320.0 } else { 150.0 };
            n_origins as f64 >= at_one_thread * threads.sqrt()
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

/// The plain search's two start states from an origin split, as
/// `(edge, seconds)`; mirrors the start of `router::route_impl` (the tests
/// check the two agree).
type Starts = [(usize, i32); 2];

fn start_states(net: &StreetNetwork, g: &ModeGraph, origin: &Split) -> Starts {
    let (mode, sp) = (g.mode, &g.sp);
    let fwd = origin.edge as usize;
    let bwd = fwd + 1;
    let off = origin.distance_to_edge_mm / sp.walk_mms;
    let start_speed = |e: usize| match mode {
        Mode::Walk => sp.walk,
        Mode::Bicycle => sp.bike,
        Mode::Car => super::labels::cms_to_ms(net.edge_speed_cms[e]),
    };
    let t1 = ((origin.distance1_mm / 1000) as f32 / start_speed(fwd)) as i32 + off;
    let t0 = ((origin.distance0_mm / 1000) as f32 / start_speed(bwd)) as i32 + off;
    [(bwd, t0), (fwd, t1)]
}

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

/// A hierarchy for one mode and one set of parameters, restricted to the
/// times of some vertices.
struct Prepared<'a> {
    net: &'a StreetNetwork,
    g: &'a ModeGraph,
    problem: Problem,
    h: Hierarchy,
    r: Restricted,
    /// The target vertices, and the restricted nodes holding each one's
    /// time: `local[node_off[i]..node_off[i + 1]]` for `vertices[i]`.
    vertices: Vec<u32>,
    node_off: Vec<u32>,
    local: Vec<u32>,
    t_graph: f64,
    t_contract: f64,
    t_restrict: f64,
}

impl<'a> Prepared<'a> {
    fn new(
        net: &'a StreetNetwork,
        g: &'a ModeGraph,
        mut vertices: Vec<u32>,
        n_threads: usize,
    ) -> Self {
        let t0 = Instant::now();
        let problem = Problem::new(net, g);
        let t_graph = t0.elapsed().as_secs_f64();
        let h = Hierarchy::build(problem.n, &problem.arcs, n_threads);
        let t_contract = t0.elapsed().as_secs_f64() - t_graph;
        vertices.sort_unstable();
        vertices.dedup();
        let mut node_off = vec![0u32];
        let mut nodes = Vec::new();
        for &v in &vertices {
            problem.vertex_nodes(net, v as usize, &mut nodes);
            node_off.push(nodes.len() as u32);
        }
        let seeds: Vec<u32> = nodes.iter().map(|&x| h.id[x as usize]).collect();
        let r = Restricted::new(&h, &seeds);
        let local = seeds.iter().map(|&s| r.local[s as usize]).collect();
        let t_restrict = t0.elapsed().as_secs_f64() - t_graph - t_contract;
        Prepared {
            net,
            g,
            problem,
            h,
            r,
            vertices,
            node_off,
            local,
            t_graph,
            t_contract,
            t_restrict,
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
                    self.h.id[self.problem.node_of_edge(self.net, e) as usize],
                    t as u32,
                )
            });
            q.upward(&self.h, &self.r, &nodes, limit, k);
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

/// The street travel-time matrix of [`super::router::travel_time_matrix`],
/// by contraction hierarchy. Callers check [`applies`] first.
pub(super) fn travel_time_matrix(
    net: &StreetNetwork,
    g: &ModeGraph,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    params: &RouteParams,
) -> Vec<Option<u32>> {
    let mode = g.mode;
    let m = params.max_trip_duration_min;
    let limit = (m * 60) as u32;
    let nd = destinations.len();
    let mut out = vec![None; origins.len() * nd];
    if out.is_empty() {
        return out;
    }
    let t0 = Instant::now();
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
    let threads = super::router::worker_threads(params.n_threads, usize::MAX);
    let p = Prepared::new(net, g, vertices, threads);
    let t_query = Instant::now();
    let n_threads = super::router::worker_threads(params.n_threads, origins.len().div_ceil(LANES));
    // Each worker takes the next LANES origins' rows of `out`.
    let batches = Mutex::new(out.chunks_mut(nd * LANES).enumerate());
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..n_threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut q = Query::new(&p.h, &p.r);
                    let mut search = Search::new(net);
                    let mut scratch = Vec::new();
                    let mut splits = [None; LANES];
                    loop {
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
    if timing() {
        eprintln!(
            "ch: nodes {} arcs {} shortcuts {} rounds {} restricted {} restricted arcs {} | \
             graph {:.3}s contract {:.3}s restrict {:.3}s query {:.3}s (total {:.3}s)",
            p.problem.n,
            p.problem.arcs.len(),
            p.h.shortcuts,
            p.h.rounds,
            p.r.m(),
            p.r.arcs.len(),
            p.t_graph,
            p.t_contract,
            p.t_restrict,
            t_query.elapsed().as_secs_f64(),
            t0.elapsed().as_secs_f64()
        );
    }
    out
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
                let prep = Prepared::new(net, &g, all.clone(), 1 + si % 3);
                if mode != Mode::Car {
                    link_nodes += prep.problem.n - net.n_vertices();
                }
                let mut q = Query::new(&prep.h, &prep.r);
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

    /// The whole matrix, minutes and empty cells, equals the plain
    /// search's: the sample's points and random ones, origins apart from
    /// destinations, every mode and setting, limits from 1 minute, 1 and
    /// 3 threads.
    #[test]
    fn matrix_matches_the_plain_search() {
        let net = build_network(format!("{SPO}/spo_osm.pbf"), &BuildOptions::default()).unwrap();
        let mut origins = sample_points();
        origins.extend(random_points(&net, 40, 11));
        let mut destinations = random_points(&net, 60, 13);
        destinations.extend(sample_points().into_iter().step_by(2));
        let mut cells = 0usize;
        let mut filled = 0usize;
        for (mode, p) in settings() {
            for minutes in [1, 4, 15, 120] {
                for n_threads in [1, 3] {
                    let p = RouteParams {
                        max_trip_duration_min: minutes,
                        n_threads,
                        ..p.clone()
                    };
                    let g = ModeGraph::new(&net, mode, Speeds::new(&p, mode), p.max_lts);
                    let got = travel_time_matrix(&net, &g, &origins, &destinations, &p);
                    ENGINE.with(|e| e.set(Some(Engine::Dijkstra)));
                    let want = crate::street::router::travel_time_matrix(
                        &net,
                        &origins,
                        &destinations,
                        mode,
                        &p,
                    )
                    .unwrap();
                    ENGINE.with(|e| e.set(None));
                    assert_eq!(got, want, "{mode:?} {p:?}");
                    cells += got.len();
                    filled += got.iter().filter(|c| c.is_some()).count();
                }
            }
        }
        assert!(filled > cells / 8 && filled < cells, "{filled} of {cells}");
    }

    /// The public matrix takes this path for a large matrix, and the plain
    /// search for a small one or when told to.
    #[test]
    fn dispatch() {
        let net = build_network(format!("{SPO}/spo_osm.pbf"), &BuildOptions::default()).unwrap();
        let p = RouteParams::default();
        let g = ModeGraph::new(&net, Mode::Walk, Speeds::new(&p, Mode::Walk), p.max_lts);
        ENGINE.with(|e| e.set(Some(Engine::Auto)));
        assert!(applies(&g, 1000, 1));
        assert!(applies(&g, 1000, 4));
        assert!(!applies(&g, 200, 4));
        assert!(!applies(&g, 10, 1));
        ENGINE.with(|e| e.set(Some(Engine::Dijkstra)));
        assert!(!applies(&g, 1000, 1));
        ENGINE.with(|e| e.set(Some(Engine::Ch)));
        assert!(applies(&g, 1, 4));
        ENGINE.with(|e| e.set(None));
    }
}
