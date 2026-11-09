//! Street-only travel times (R5 `StreetRouter` + `LinkedPointSet.eval`).
//!
//! The search runs over directed-edge states: the time at the end of each
//! edge, so car turn costs can depend on the edge a vehicle arrives by.

use super::flags::*;
use super::labels::cms_to_ms;
use super::link::{find_split, Split};
use super::StreetNetwork;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
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

/// Reusable per-thread search state.
struct Search {
    /// Best time at the end of each directed edge.
    edge_time: Vec<i32>,
    /// Best time at each vertex (min over incoming edge states).
    vertex_time: Vec<i32>,
    touched_edges: Vec<u32>,
    touched_vertices: Vec<u32>,
    heap: BinaryHeap<Reverse<(i32, u32)>>,
}

impl Search {
    fn new(net: &StreetNetwork) -> Self {
        Search {
            edge_time: vec![i32::MAX; net.n_edges()],
            vertex_time: vec![i32::MAX; net.n_vertices()],
            touched_edges: Vec::new(),
            touched_vertices: Vec::new(),
            heap: BinaryHeap::new(),
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
        self.heap.clear();
    }

    /// Record a state; `true` if it improves the edge (ties keep the old one).
    #[inline]
    fn improve(&mut self, net: &StreetNetwork, e: usize, t: i32) -> bool {
        if t >= self.edge_time[e] {
            return false;
        }
        if self.edge_time[e] == i32::MAX {
            self.touched_edges.push(e as u32);
        }
        self.edge_time[e] = t;
        let v = net.edge_to(e);
        if t < self.vertex_time[v] {
            if self.vertex_time[v] == i32::MAX {
                self.touched_vertices.push(v as u32);
            }
            self.vertex_time[v] = t;
        }
        true
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
    // Start states carry the split edge as back edge so turn costs apply when
    // leaving it. Their times are not checked against the limit.
    let start_speed = |e: usize| match mode {
        Mode::Walk => sp.walk,
        Mode::Bicycle => sp.bike,
        Mode::Car => cms_to_ms(net.edge_speed_cms[e]),
    };
    let t1 = ((origin.distance1_mm / 1000) as f32 / start_speed(fwd)) as i32 + off;
    let t0 = ((origin.distance0_mm / 1000) as f32 / start_speed(bwd)) as i32 + off;
    for (e, t) in [(bwd, t0), (fwd, t1)] {
        if s.improve(net, e, t) {
            s.heap.push(Reverse((t, e as u32)));
        }
    }
    while let Some(Reverse((t, e))) = s.heap.pop() {
        let e = e as usize;
        if t > s.edge_time[e] {
            continue; // dominated since it was queued
        }
        let v = net.edge_to(e);
        for &f in net.out_edges(v) {
            let f = f as usize;
            let flags = net.edge_flags[f];
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
            if t1 < limit && s.improve(net, f, t1) {
                s.heap.push(Reverse((t1, f as u32)));
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
