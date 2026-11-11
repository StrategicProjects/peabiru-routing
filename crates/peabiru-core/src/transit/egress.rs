//! Egress from public transport by bicycle or car (R5 `EgressCostTable`
//! for `BICYCLE` and `CAR`, then `destructivelyTransposeForPropagationAsNeeded`).
//!
//! Walking egress reads the walking trees kept with the network
//! ([`super::walk::egress_tables`]). For the other modes R5 keeps nothing:
//! for each destination point set it runs a street search from every
//! linked stop and extends it to the points around the stop. Those searches
//! carry no request: they run with the defaults of R5's `ProfileRequest`
//! (walking 1.3 m/s, any level of traffic stress), whatever the speeds and
//! limits of the query, which only apply when the costs are turned into
//! egress times.
//!
//! - Bicycle: a distance-minimising search, up to 5 km
//!   (`BICYCLE_DISTANCE_LINKING_LIMIT_METERS`), to the points within 5 km
//!   of the stop; the cost is millimetres.
//! - Car: a time-minimising search (turn costs and restrictions), below
//!   30 minutes (`CAR_TIME_LINKING_LIMIT_SECONDS`), to the points within
//!   30 min x 22 m/s of the stop; the cost is seconds, the destination
//!   edge driven at its car speed and the last stretch off the street at
//!   1.3 m/s (`OFF_STREET_SPEED_MILLIMETERS_PER_SECOND`).
//!
//! Each search starts from the coordinates of the stop's vertex, linked to
//! the streets by the mode (R5 links the stop again rather than starting
//! from its vertex, whose connector cars may not use).

use super::walk::{bike_distance_tree, egress_cost, stop_envelope_m, Envelope, TreeScratch};
use super::TransitLayer;
use crate::street::geom::FIXED_FACTOR;
use crate::street::labels::cms_to_ms;
use crate::street::link::{find_split, Split};
use crate::street::router::{route, worker_threads, Mode, ModeGraph, Search};
use crate::street::StreetNetwork;
use crate::CancelToken;
use std::sync::atomic::{AtomicUsize, Ordering};

/// `EgressCostTable.BICYCLE_DISTANCE_LINKING_LIMIT_METERS`.
const BIKE_LIMIT_M: i32 = 5000;
/// `EgressCostTable.CAR_TIME_LINKING_LIMIT_SECONDS`.
const CAR_LIMIT_S: i32 = 30 * 60;
/// `EgressCostTable.MAX_CAR_SPEED_METERS_PER_SECOND`.
const MAX_CAR_SPEED_MS: i32 = 22;
/// `LinkedPointSet.OFF_STREET_SPEED_MILLIMETERS_PER_SECOND`.
const OFF_STREET_MMS: i32 = 1300;

/// A stop and its `(target, cost)` pairs.
type StopTable = (usize, Vec<(u32, i32)>);

/// The destinations of an egress table.
#[derive(Clone, Copy)]
pub(crate) enum Targets<'p> {
    /// Points `(lat, lon)`: those strictly inside a stop's envelope
    /// (`FreeFormPointSet.getPointsInEnvelope`).
    Points(&'p [(f64, f64)]),
    /// The cells of a grid: every cell a stop's envelope overlaps
    /// (`WebMercatorGridPointSet.getPointsInEnvelope`).
    Grid(&'p crate::WebMercatorGrid),
}

impl Targets<'_> {
    fn len(&self) -> usize {
        match self {
            Targets::Points(p) => p.len(),
            Targets::Grid(g) => g.n_cells(),
        }
    }
}

/// The egress costs by `mode` (bicycle or car) from every linked stop to
/// the targets, per target `(stop, cost)` in increasing stop order: the
/// order R5 fills each target's map in. The cost is millimetres by bicycle
/// and seconds by car (see the module). `dest_splits` are the targets
/// linked to the streets by `mode`. The stops are shared out among up to
/// `n_threads` threads (0 = all cores); the result does not depend on it.
/// `None` when `cancel` is cancelled, which the workers check between
/// stops.
pub(crate) fn mode_egress_tables(
    net: &StreetNetwork,
    layer: &TransitLayer,
    mode: Mode,
    targets: Targets<'_>,
    dest_splits: &[Option<Split>],
    n_threads: usize,
    cancel: &CancelToken,
) -> Option<Vec<Vec<(u32, i32)>>> {
    debug_assert_ne!(mode, Mode::Walk);
    let radius_m = match mode {
        Mode::Bicycle => f64::from(BIKE_LIMIT_M),
        _ => f64::from(CAR_LIMIT_S * MAX_CAR_SPEED_MS),
    };
    // points sorted by latitude, to find those of an envelope by binary
    // search; a NaN latitude is inside no envelope
    let by_lat: Vec<(f64, u32)> = match targets {
        Targets::Points(points) => {
            let mut v: Vec<(f64, u32)> = points
                .iter()
                .enumerate()
                .filter(|(_, p)| !p.0.is_nan())
                .map(|(i, p)| (p.0, i as u32))
                .collect();
            v.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            v
        }
        Targets::Grid(_) => Vec::new(),
    };
    let car = (mode == Mode::Car).then(|| ModeGraph::r5_defaults(net, Mode::Car));
    let n_stops = layer.stop_ids.len();
    let next = AtomicUsize::new(0);
    let n_threads = worker_threads(n_threads, n_stops);
    let per_thread: Vec<Vec<StopTable>> = crate::sys::scoped(0..n_threads, |_| {
        let mut done = Vec::new();
        let mut split_scratch = Vec::new();
        let mut tree_scratch = (mode == Mode::Bicycle).then(|| TreeScratch::new(net));
        let mut dist = if mode == Mode::Bicycle {
            vec![i32::MAX; net.n_vertices()]
        } else {
            Vec::new()
        };
        let mut search = car.as_ref().map(|_| Search::new(net));
        while !cancel.is_cancelled() {
            let s = next.fetch_add(1, Ordering::Relaxed);
            if s >= n_stops {
                break;
            }
            let v = layer.stop_vertex[s];
            if v < 0 {
                continue;
            }
            let v = v as usize;
            let (lat, lon) = (
                f64::from(net.v_lat[v]) / FIXED_FACTOR,
                f64::from(net.v_lon[v]) / FIXED_FACTOR,
            );
            let Some(origin) = find_split(net, lat, lon, mode, &mut split_scratch) else {
                continue;
            };
            let env = stop_envelope_m(layer, net, s, radius_m);
            let mut table = Vec::new();
            match mode {
                Mode::Bicycle => {
                    let sc = tree_scratch.as_mut().expect("bicycle scratch");
                    let tree = bike_distance_tree(net, &origin, BIKE_LIMIT_M * 1000, sc);
                    for &(u, d) in &tree {
                        dist[u as usize] = d;
                    }
                    for_targets(targets, &by_lat, env, |p| {
                        if let Some(sp) = &dest_splits[p] {
                            let (d0, d1) = (dist[sp.vertex0 as usize], dist[sp.vertex1 as usize]);
                            if let Some(c) = egress_cost(d0, d1, sp) {
                                table.push((p as u32, c));
                            }
                        }
                    });
                    for &(u, _) in &tree {
                        dist[u as usize] = i32::MAX;
                    }
                }
                _ => {
                    let g = car.as_ref().expect("car graph");
                    let search = search.as_mut().expect("car search");
                    route(net, g, &origin, CAR_LIMIT_S, search);
                    let vt = search.vertex_time();
                    for_targets(targets, &by_lat, env, |p| {
                        if let Some(sp) = &dest_splits[p] {
                            if let Some(c) = car_cost(net, vt, sp) {
                                table.push((p as u32, c));
                            }
                        }
                    });
                }
            }
            if !table.is_empty() {
                done.push((s, table));
            }
        }
        done
    });
    if cancel.is_cancelled() {
        return None;
    }
    let mut by_stop: Vec<StopTable> = per_thread.into_iter().flatten().collect();
    by_stop.sort_unstable_by_key(|&(s, _)| s);
    let mut out = vec![Vec::new(); targets.len()];
    for (s, table) in by_stop {
        for (p, c) in table {
            out[p as usize].push((s as u32, c));
        }
    }
    Some(out)
}

/// Call `f(target)` for each target of a stop's envelope, in increasing
/// target order for a grid, by latitude for points (the order does not
/// matter: each target gets at most one cost per stop).
fn for_targets(
    targets: Targets<'_>,
    by_lat: &[(f64, u32)],
    env: Envelope,
    mut f: impl FnMut(usize),
) {
    let (south, north, west, east) = env;
    match targets {
        Targets::Points(points) => {
            let lo = by_lat.partition_point(|e| e.0 <= south);
            let hi = by_lat.partition_point(|e| e.0 < north);
            for &(_, p) in by_lat.get(lo..hi).unwrap_or(&[]) {
                let lon = points[p as usize].1;
                if lon < east && lon > west {
                    f(p as usize);
                }
            }
        }
        Targets::Grid(grid) => {
            let Some(((y0, y1), (x0, x1))) = grid.cells_in_envelope(env) else {
                return;
            };
            let width = grid.width() as usize;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    f(y * width + x);
                }
            }
        }
    }
}

/// Seconds by car from a stop to a linked point, given the stop's search
/// times at the vertices (`LinkedPointSet.timeToPoint` with the car speed
/// of the point's edge and 1.3 m/s off the street); `None` when neither
/// end of the edge was reached or the sum overflows R5's `int`.
fn car_cost(net: &StreetNetwork, vt: &[i32], sp: &Split) -> Option<i32> {
    let (t0, t1) = (vt[sp.vertex0 as usize], vt[sp.vertex1 as usize]);
    if t0 == i32::MAX && t1 == i32::MAX {
        return None;
    }
    let on = (cms_to_ms(net.edge_speed_cms[sp.edge as usize]) * 1000.0f32) as i32;
    let off = sp.distance_to_edge_mm / OFF_STREET_MMS;
    let via = |t: i32, d: i32| {
        let x = t.wrapping_add(d / on).wrapping_add(off);
        if x < 0 {
            i32::MAX
        } else {
            x
        }
    };
    let c = via(t0, sp.distance0_mm).min(via(t1, sp.distance1_mm));
    (c != i32::MAX).then_some(c)
}
