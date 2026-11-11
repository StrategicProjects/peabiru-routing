//! Per-departure travel times with the routes ridden, as r5r 2.5.0's
//! `expanded_travel_time_matrix()` and `arrival_travel_time_matrix()` with
//! `mode = c("WALK", "TRANSIT")`.
//!
//! Both are one R5 regional task per origin with `includePathResults`:
//! the RAPTOR search keeps a path to every stop at every iteration
//! (`FastRaptorWorker.retainPaths`), propagation keeps the path and egress
//! behind each destination's best time per iteration
//! (`PerTargetPropagater.propagateTransit`), and `TravelTimeReducer
//! .recordPathsForTarget` turns them into one record per iteration. r5r's
//! `TravelTimeMatrixComputer` (bytecode of the r5r 2.5.0 jar; offsets below)
//! then breaks each record down into minutes (`extractPathResults`), sorts
//! the records of each departure minute (`PATH_ORDER`) and numbers them as
//! draws (`populateTravelTimesBreakdown`), or for arrival keeps the latest
//! departure that arrives in time (`filterLatestBeforeArrivalTime`). The R
//! wrappers finally blank out rows whose `total_time` exceeds
//! `max_trip_duration`.

use super::matrix::{
    civil_from_days, days_from_civil, validate_date, weekday, TransitData, TransitParams,
};
use super::raptor::{self, Filtered, Leg, PathArena, RaptorRequest, NO_PATH, UNREACHED};
use super::walk;
use super::TransitLayer;
use crate::jcompat::TroveIntKeys;
use crate::street::link::{find_split, link_points, Split};
use crate::street::router::{search_from, worker_threads, Mode, ModeGraph, Search};
use crate::street::StreetNetwork;
use crate::{CancelToken, RouteError};
use std::cmp::Ordering;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;

/// R5 refuses path results for more destinations than this
/// (`PathResult.MAX_PATH_DESTINATIONS`).
pub const MAX_PATH_DESTINATIONS: usize = 5_000;

/// Bound on `iterations x stops` kept per origin (4 bytes each), so that a
/// huge `time_window` or `draws_per_minute` errors out instead of
/// exhausting memory.
const MAX_ITERATION_STOPS: u64 = 1 << 28;

/// One trip as r5r reports it, every time in minutes with one decimal.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Breakdown {
    /// Walking from the origin to the first stop (or the whole walk of a
    /// walk-only trip).
    pub access_time: f64,
    /// Waiting at stops, the first boarding included.
    pub wait_time: f64,
    /// In vehicles.
    pub ride_time: f64,
    /// Walking between stops.
    pub transfer_time: f64,
    /// Walking from the last stop to the destination.
    pub egress_time: f64,
    /// Route ids joined by `|`, or `[WALK]` for a walk-only trip.
    pub routes: String,
    /// Vehicles boarded.
    pub n_rides: u32,
    /// Total travel time: as r5r computes it, the sum of the five times
    /// above rounded to one decimal (when that sum is 0, the trip's own
    /// total, rounded).
    pub total_time: f64,
}

/// One row of an expanded or arrival matrix (the origin is the row's place
/// in [`ExpandedMatrix`]).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ExpandedRow {
    /// Destination index.
    pub to: u32,
    /// Departure, seconds after midnight of the service day (can exceed
    /// 24 h for an arrival search crossing midnight).
    pub departure: i32,
    /// 1-based rank of the trip among the departure minute's draws.
    pub draw: u32,
    /// `None` where r5r reports NA (no trip within `max_trip_duration`).
    pub trip: Option<Breakdown>,
}

/// The rows of an expanded or arrival matrix, origin by origin, each
/// origin's rows in r5r's order (see [`expanded_travel_time_matrix`] and
/// [`arrival_travel_time_matrix`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ExpandedMatrix(Vec<Vec<ExpandedRow>>);

impl ExpandedMatrix {
    /// Number of origins.
    pub fn n_origins(&self) -> usize {
        self.0.len()
    }

    /// Number of rows, all origins together.
    pub fn n_rows(&self) -> usize {
        self.0.iter().map(Vec::len).sum()
    }

    /// The rows of origin `origin` (an index in the order the origins were
    /// given). Panics when `origin` is out of range.
    pub fn origin(&self, origin: usize) -> &[ExpandedRow] {
        &self.0[origin]
    }

    /// Each origin's rows, in origin order.
    pub fn origins(&self) -> impl ExactSizeIterator<Item = &[ExpandedRow]> + '_ {
        self.0.iter().map(Vec::as_slice)
    }

    /// Each origin's rows, in origin order, without copying.
    pub fn into_vec(self) -> Vec<Vec<ExpandedRow>> {
        self.0
    }
}

/// `Math.round(double)` for the values met here: the nearest integer,
/// ties towards positive infinity.
fn java_round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// `PathBreakdown.roundTo1Place`: `Math.round(x * 10) / 10.0`.
fn round1(x: f64) -> f64 {
    java_round(x * 10.0) / 10.0
}

/// `(double)((float) seconds / 60f)`, the minutes conversion of
/// `extractPathResults` (offs 213-218 and alike).
fn minutes_f32(seconds: i32) -> f64 {
    f64::from(seconds as f32 / 60.0f32)
}

/// r5r's `TravelTimeMatrixComputer$PathBreakdown` with its raw (unrounded)
/// minute fields.
#[derive(Debug, Clone)]
struct PathBreakdown {
    access: f64,
    wait: f64,
    ride: f64,
    transfer: f64,
    egress: f64,
    total: f64,
    /// Shared between the records of one path.
    routes: Arc<str>,
    n_rides: u32,
}

impl PathBreakdown {
    /// A walk-only record: every component 0, `routes = directModes`
    /// (`walk_routes` holds `[WALK]`).
    fn walk(walk_routes: &Arc<str>, total: f64) -> Self {
        PathBreakdown {
            access: 0.0,
            wait: 0.0,
            ride: 0.0,
            transfer: 0.0,
            egress: 0.0,
            total,
            routes: walk_routes.clone(),
            n_rides: 0,
        }
    }

    /// `PathBreakdown.unreachable()`: total `Integer.MAX_VALUE` minutes.
    fn unreachable(walk_routes: &Arc<str>) -> Self {
        Self::walk(walk_routes, f64::from(i32::MAX))
    }

    /// The row r5r writes (`addPathToDataframe`): `total_time` is the
    /// rounded sum of the rounded components when positive
    /// (`getCombinedTravelTime`), else the rounded total; the R wrapper then
    /// sets the row to NA when `total_time > max_trip_duration`.
    fn report(&self, max_trip_min: i32) -> Option<Breakdown> {
        let (a, w, r, t, e) = (
            round1(self.access),
            round1(self.wait),
            round1(self.ride),
            round1(self.transfer),
            round1(self.egress),
        );
        let combined = round1(a + w + r + t + e);
        let total = if combined > 0.0 {
            combined
        } else {
            round1(self.total)
        };
        (total <= f64::from(max_trip_min)).then(|| Breakdown {
            access_time: a,
            wait_time: w,
            ride_time: r,
            transfer_time: t,
            egress_time: e,
            routes: self.routes.to_string(),
            n_rides: self.n_rides,
            total_time: total,
        })
    }
}

/// `PATH_ORDER`: total time, routes (Java `String.compareTo`, UTF-16 code
/// units), then access, wait, ride, transfer and egress (`Double.compare`).
fn path_order(a: &PathBreakdown, b: &PathBreakdown) -> Ordering {
    a.total
        .total_cmp(&b.total)
        .then_with(|| a.routes.encode_utf16().cmp(b.routes.encode_utf16()))
        .then_with(|| a.access.total_cmp(&b.access))
        .then_with(|| a.wait.total_cmp(&b.wait))
        .then_with(|| a.ride.total_cmp(&b.ride))
        .then_with(|| a.transfer.total_cmp(&b.transfer))
        .then_with(|| a.egress.total_cmp(&b.egress))
}

/// The records of one destination: transit records per departure minute
/// (sorted by `PATH_ORDER`) and walk-only records (`DIRECT_KEY = -1`).
#[derive(Default)]
struct DestinationPaths {
    /// Indexed by minute of the window (0 = first departure); a minute's
    /// list is filled when [`Records::minute`] first asks for it.
    by_minute: Vec<Vec<PathBreakdown>>,
    /// Whether `by_minute[k]` has been filled.
    built: Vec<bool>,
    walk: Vec<PathBreakdown>,
}

/// What the transit records of a destination are built from: the best
/// path and egress behind each iteration's time after propagation.
struct TransitSource<'a> {
    layer: &'a TransitLayer,
    paths: &'a PathArena,
    best: &'a [i32],
    best_path: &'a [u32],
    best_egress: &'a [i32],
    access_time: &'a [i32],
    /// The route list of each path, built once per path and origin.
    routes: &'a mut [Option<Arc<str>>],
    per_minute: usize,
    to_time: i32,
}

/// One destination's records as the callers of [`Context::origin_paths`]
/// read them: the walk-only records, sorted, and each minute's transit
/// records, built and sorted on first request. The arrival search usually
/// reads only the last minutes of its window, so the other minutes are
/// never built; the expanded matrix reads them all.
struct Records<'a> {
    dp: &'a mut DestinationPaths,
    src: Option<TransitSource<'a>>,
    /// Whether some iteration reached the destination by transit.
    has_transit: bool,
}

impl Records<'_> {
    fn is_empty(&self) -> bool {
        self.dp.walk.is_empty() && !self.has_transit
    }

    fn walk(&self) -> &[PathBreakdown] {
        &self.dp.walk
    }

    /// The transit records of departure minute `k` (0 = first departure)
    /// in `PATH_ORDER`: one per iteration of that minute with a transit
    /// path, in iteration order before the (stable) sort, as
    /// `extractPathResults` adds them.
    fn minute(&mut self, k: usize) -> Result<&[PathBreakdown], String> {
        if !self.dp.built[k] {
            let list = &mut self.dp.by_minute[k];
            list.clear();
            if let Some(s) = &mut self.src {
                // iterations run from the latest minute down
                let block = self.dp.built.len() - 1 - k;
                let dep = s.to_time - 60 * (1 + block as i32);
                let end = ((block + 1) * s.per_minute).min(s.best_path.len());
                for i in block * s.per_minute..end {
                    let path_k = s.best_path[i];
                    if path_k == NO_PATH {
                        continue;
                    }
                    let path = s.paths.legs(path_k);
                    let routes = s.routes[path_k as usize]
                        .get_or_insert_with(|| route_list(s.layer, path))
                        .clone();
                    list.push(transit_breakdown(
                        path,
                        routes,
                        dep,
                        s.access_time,
                        s.best_egress[i],
                        s.best[i],
                    )?);
                }
                list.sort_by(path_order); // stable, as `List.sort`
            }
            self.dp.built[k] = true;
        }
        Ok(&self.dp.by_minute[k])
    }
}

/// What every origin shares.
struct Context<'a> {
    net: &'a StreetNetwork,
    layer: &'a TransitLayer,
    data: &'a TransitData,
    filtered: Filtered,
    p: &'a TransitParams,
    dest_splits: Vec<Option<Split>>,
    /// Per destination, `(stop, mm)` in the iteration order of R5's
    /// per-point `TIntIntHashMap` (`EgressCostTable
    /// .destructivelyTransposeForPropagationAsNeeded`): ties between stops
    /// are won by the first one met.
    egress: Vec<Vec<(u32, i32)>>,
    /// Stops within egress reach of some destination.
    retain: Vec<bool>,
    walk_mms: i32,
    /// The walking graph of the access and direct search.
    walk: ModeGraph,
    /// `[WALK]`, the routes of a walk-only record.
    walk_routes: Arc<str>,
    /// r5r's draws per minute (`populateExpandedResults` offs 13-46):
    /// `numberOfMonteCarloDraws / timeWindowSize` with frequencies, else 1.
    draws_per_minute: usize,
}

impl<'a> Context<'a> {
    fn new(
        net: &'a StreetNetwork,
        layer: &'a TransitLayer,
        data: &'a TransitData,
        destinations: &[(f64, f64)],
        p: &'a TransitParams,
    ) -> Result<Self, RouteError> {
        validate(layer, destinations.len(), p)?;
        let active = layer.active_services(p.date, weekday(p.date));
        let filtered = Filtered::new(layer, &active)?;
        let walk_speed = (p.walk_speed_kmh * 5.0 / 18.0) as f32;
        let walk_mms = (walk_speed * 1000.0f32) as i32;
        let dest_splits = link_points(
            net,
            destinations.len(),
            |j| destinations[j],
            Mode::Walk,
            p.n_threads,
            &p.cancel,
        )
        .ok_or(RouteError::Cancelled)?;
        let egress = walk::egress_tables(
            layer,
            &data.trees,
            net,
            destinations,
            &dest_splits,
            p.n_threads,
            &p.cancel,
        )
        .ok_or(RouteError::Cancelled)?;
        let mut retain = vec![false; layer.stop_ids.len()];
        let egress = egress
            .into_iter()
            .map(|table| {
                // `table` is in increasing stop order, the order R5 puts the
                // stops into the map.
                let mut keys = TroveIntKeys::new();
                for &(s, _) in &table {
                    keys.insert(s as i32);
                    retain[s as usize] = true;
                }
                keys.iteration_order()
                    .map(|s| {
                        let i = table
                            .binary_search_by_key(&(s as u32), |&(t, _)| t)
                            .expect("stop in egress table");
                        table[i]
                    })
                    .collect()
            })
            .collect();
        let draws_per_minute = if layer.has_frequencies {
            ((p.time_window * p.draws_per_minute) / p.time_window) as usize
        } else {
            1
        };
        Ok(Context {
            net,
            layer,
            data,
            filtered,
            p,
            dest_splits,
            egress,
            retain,
            walk_mms,
            walk: ModeGraph::with_walk_speed(net, Mode::Walk, p.walk_speed_kmh),
            walk_routes: "[WALK]".into(),
            draws_per_minute,
        })
    }

    /// The records of every destination from origin `(lat, lon)`, handed to
    /// `on_destination(j, records)` in destination order; `false` when the
    /// origin is not linked (R5 then reports nothing).
    fn origin_paths(
        &self,
        lat: f64,
        lon: f64,
        sc: &mut OriginScratch,
        mut on_destination: impl FnMut(usize, &mut Records) -> Result<(), String>,
    ) -> Result<bool, String> {
        let p = self.p;
        let m = p.max_trip_duration;
        let max_t = m * 60;
        let tw = p.time_window as usize;
        let Some(origin) = find_split(self.net, lat, lon, Mode::Walk, &mut sc.split) else {
            return Ok(false);
        };
        let street_limit = m.min(p.max_walk_time) * 60;
        let egress_limit = p.max_walk_time * 60;
        let search = sc.search.get_or_insert_with(|| Search::new(self.net));
        search_from(
            self.net,
            &self.walk,
            &origin,
            &self.dest_splits,
            street_limit,
            search,
            &mut sc.direct,
        );
        let vertex_time = search.vertex_time();
        let access: Vec<(u32, i32)> = self
            .layer
            .stop_vertex
            .iter()
            .enumerate()
            .filter(|(_, &v)| v >= 0 && vertex_time[v as usize] < street_limit)
            .map(|(s, &v)| (s as u32, vertex_time[v as usize]))
            .collect();
        let direct = &sc.direct;
        let dp = &mut sc.records;
        dp.by_minute.resize_with(tw, Vec::new);
        dp.built.resize(tw, false);
        let clear = |dp: &mut DestinationPaths| {
            dp.built.iter_mut().for_each(|b| *b = false);
            dp.walk.clear();
        };
        if access.is_empty() {
            // No stop reached: R5 records the direct time as every percentile
            // and no path result (`TravelTimeComputer.java:304-315`); r5r then
            // emits a walk record of `percentile minutes` when <= M
            // (`extractPathResults` offs 528-604; totalTime is `(double)
            // minutes`).
            for (j, &d) in direct.iter().enumerate() {
                clear(dp);
                let minutes = if d == UNREACHED { UNREACHED } else { d / 60 };
                if minutes <= m {
                    dp.walk
                        .push(PathBreakdown::walk(&self.walk_routes, f64::from(minutes)));
                }
                let mut rec = Records {
                    dp: &mut *dp,
                    src: None,
                    has_transit: false,
                };
                on_destination(j, &mut rec)?;
            }
            return Ok(true);
        }
        let req = RaptorRequest {
            from_time: p.departure_time,
            time_window: p.time_window,
            max_rides: p.max_rides,
            max_duration: max_t,
            walk_mms: self.walk_mms,
            max_walk_min: p.max_walk_time,
            monte_carlo_draws: p.time_window * p.draws_per_minute,
            seed: p.seed,
            origin: (lat, lon),
        };
        raptor::route_paths_into(
            self.layer,
            &self.data.transfers,
            &self.filtered,
            &access,
            &req,
            &self.retain,
            &p.cancel,
            &mut sc.times,
            &mut sc.arena,
        );
        if p.cancel.is_cancelled() {
            // the search stopped early; the caller discards the origin
            return Ok(true);
        }
        let (times, paths) = (&sc.times, &sc.arena);
        let n_stops = self.layer.stop_ids.len();
        // Per stop, the least duration over the iterations: a lower bound
        // on every candidate through that stop.
        let stop_min = &mut sc.stop_min;
        stop_min.clear();
        stop_min.resize(n_stops, UNREACHED);
        for row in times.chunks_exact(n_stops.max(1)) {
            for (m, &t) in stop_min.iter_mut().zip(row) {
                *m = (*m).min(t);
            }
        }
        let stop_min = &*stop_min;
        let per_minute = req.iterations_per_minute(self.layer.has_frequencies)?;
        let n_iter = times.len() / n_stops.max(1);
        let to_time = p.departure_time + p.time_window * 60;
        let mut access_time = vec![-1; self.layer.stop_ids.len()];
        for &(s, t) in &access {
            access_time[s as usize] = t;
        }
        // The route list of each path, built once per path.
        let mut routes: Vec<Option<Arc<str>>> = vec![None; paths.len()];
        let mut best = vec![0; n_iter];
        let mut best_path: Vec<u32> = vec![NO_PATH; n_iter];
        let mut best_egress = vec![0; n_iter];
        for (j, &d) in direct.iter().enumerate() {
            if p.cancel.is_cancelled() {
                return Ok(true);
            }
            best.iter_mut().for_each(|t| *t = d);
            best_path.iter_mut().for_each(|x| *x = NO_PATH);
            propagate(
                &self.egress[j],
                times,
                stop_min,
                n_stops,
                paths,
                self.walk_mms,
                egress_limit,
                max_t,
                &mut best,
                &mut best_path,
                &mut best_egress,
            );
            // `recordPathsForTarget` + `extractPathResults`: the walk-only
            // records now, the transit ones per minute on request.
            clear(dp);
            for i in 0..n_iter {
                if best_path[i] == NO_PATH && best[i] < UNREACHED {
                    dp.walk
                        .push(PathBreakdown::walk(&self.walk_routes, minutes_f32(best[i])));
                }
            }
            dp.walk.sort_by(path_order);
            let mut rec = Records {
                dp: &mut *dp,
                has_transit: best_path.iter().any(|&k| k != NO_PATH),
                src: Some(TransitSource {
                    layer: self.layer,
                    paths,
                    best: &best,
                    best_path: &best_path,
                    best_egress: &best_egress,
                    access_time: &access_time,
                    routes: &mut routes,
                    per_minute,
                    to_time,
                }),
            };
            on_destination(j, &mut rec)?;
        }
        Ok(true)
    }
}

/// Per-thread buffers reused across origins.
#[derive(Default)]
struct OriginScratch {
    split: Vec<u32>,
    search: Option<Search>,
    direct: Vec<i32>,
    records: DestinationPaths,
    /// RAPTOR durations, iteration-major, and paths.
    times: Vec<i32>,
    arena: PathArena,
    /// Per stop, the least of `times` over the iterations.
    stop_min: Vec<i32>,
}

/// `PerTargetPropagater.propagateTransit` for one destination, keeping the
/// path (an index into `paths.paths`) and egress time behind each
/// iteration's best time. Only a strict improvement replaces them, so equal
/// times keep the stop met first. `stop_min[s]` is the least of stop `s`'s
/// durations over the iterations.
#[allow(clippy::too_many_arguments)]
fn propagate(
    egress: &[(u32, i32)],
    times: &[i32],
    stop_min: &[i32],
    n_stops: usize,
    paths: &PathArena,
    walk_mms: i32,
    egress_limit: i32,
    max_t: i32,
    best: &mut [i32],
    best_path: &mut [u32],
    best_egress: &mut [i32],
) {
    // No candidate at or above `bound` replaces anything: the smaller of
    // max_t and the worst time so far (only strict improvements count).
    let mut bound = best.iter().copied().max().map_or(max_t, |w| max_t.min(w));
    for &(stop, mm) in egress {
        let e = mm / walk_mms;
        if e >= egress_limit {
            continue;
        }
        // every iteration's candidate through this stop is at least its
        // least duration plus the egress walk
        if stop_min[stop as usize].saturating_add(e) >= bound {
            continue;
        }
        let mut worst = 0;
        for (i, t) in best.iter_mut().enumerate() {
            let ts = times[i * n_stops + stop as usize];
            if ts < max_t && ts < *t {
                // saturating: past i32::MAX is past max_t
                let c = ts.saturating_add(e);
                if c < max_t && c < *t {
                    *t = c;
                    if let Some(k) = paths.index(i, stop as usize) {
                        best_path[i] = k;
                        best_egress[i] = e;
                    }
                }
            }
            worst = worst.max(*t);
        }
        bound = max_t.min(worst);
    }
}

/// The routes of a path joined by `|`, as r5r reports them.
fn route_list(layer: &TransitLayer, path: &[Leg]) -> Arc<str> {
    path.iter()
        .map(|l| layer.patterns[l.pattern as usize].route_id.as_str())
        .collect::<Vec<_>>()
        .join("|")
        .into()
}

/// The breakdown of one transit iteration (`extractPathResults` offs
/// 162-393): access to the first boarding stop, waits derived from the
/// departure (`Path.computeWaitTimes`), rides, transfers and egress.
fn transit_breakdown(
    path: &[Leg],
    routes: Arc<str>,
    departure: i32,
    access_time: &[i32],
    egress: i32,
    total: i32,
) -> Result<PathBreakdown, String> {
    let access = path
        .first()
        .map(|l| access_time[l.board_stop as usize])
        .filter(|&t| t >= 0)
        .ok_or("path without access to its first boarding stop (R5 fails here too)")?;
    // `TransitPath::wait_times`, summed
    let mut ready = departure + access;
    let mut wait: i32 = 0;
    for l in path {
        ready += l.transfer_time;
        let w = l.board_time - ready;
        if w < 0 {
            return Err("path with a negative wait (R5 fails here too)".into());
        }
        wait += w;
        ready = l.board_time + l.ride_time;
    }
    Ok(PathBreakdown {
        access: minutes_f32(access),
        wait: minutes_f32(wait),
        ride: minutes_f32(path.iter().map(|l| l.ride_time).sum()),
        transfer: minutes_f32(path.iter().map(|l| l.transfer_time).sum()),
        egress: minutes_f32(egress),
        total: minutes_f32(total),
        routes,
        n_rides: path.len() as u32,
    })
}

fn validate(layer: &TransitLayer, n_destinations: usize, p: &TransitParams) -> Result<(), String> {
    if n_destinations > MAX_PATH_DESTINATIONS {
        return Err(format!(
            "at most {MAX_PATH_DESTINATIONS} destinations are supported with paths"
        ));
    }
    // the bounds shared with the plain matrix (they also keep the walk
    // speed a non-zero divisor); percentiles are not used here
    p.check()?;
    let per_minute = if layer.has_frequencies {
        p.draws_per_minute as u64
    } else {
        1
    };
    let cells = p.time_window as u64 * per_minute * (layer.stop_ids.len() as u64).max(1);
    if cells > MAX_ITERATION_STOPS {
        return Err("time_window x draws_per_minute is too large for this network".into());
    }
    Ok(())
}

/// Run `f` for every origin on `n_threads` threads, results in origin
/// order; `Cancelled` when `cancel` is, and then the workers stop before
/// their next origin.
fn per_origin<T: Send>(
    origins: &[(f64, f64)],
    n_threads: usize,
    cancel: &CancelToken,
    f: impl Fn(f64, f64, &mut OriginScratch) -> Result<T, String> + Sync,
) -> Result<Vec<T>, RouteError> {
    let n_threads = worker_threads(n_threads, origins.len());
    let next = AtomicUsize::new(0);
    let done: Vec<Vec<(usize, Result<T, String>)>> = crate::sys::scoped(0..n_threads, |_| {
        let mut done = Vec::new();
        let mut scratch = OriginScratch::default();
        while !cancel.is_cancelled() {
            let i = next.fetch_add(1, AtomicOrdering::Relaxed);
            if i >= origins.len() {
                break;
            }
            let (lat, lon) = origins[i];
            done.push((i, f(lat, lon, &mut scratch)));
        }
        done
    });
    cancel.check()?;
    let mut out: Vec<Option<T>> = (0..origins.len()).map(|_| None).collect();
    for (i, r) in done.into_iter().flatten() {
        out[i] = Some(r?);
    }
    Ok(out
        .into_iter()
        .map(|r| r.expect("every origin routed"))
        .collect())
}

/// r5r's `expanded_travel_time_matrix()`: per origin, the rows in r5r's
/// order (destination, departure minute, draw). For each departure minute
/// the transit records come first in `PATH_ORDER`, numbered 1.., then the
/// remaining draws repeat the walk-only record (or an unreachable, NA one)
/// up to the draws per minute (`populateTravelTimesBreakdown`). A
/// destination with no record at all has no rows. Points are `(lat, lon)`
/// degrees; `p.percentiles` are not used.
///
/// [`RouteError::Invalid`] when the parameters are invalid (see
/// [`TransitParams::validate`]) or there are more than 5,000 destinations
/// (R5's limit for paths); [`RouteError::Cancelled`] when `p.cancel` is
/// cancelled before every origin is done.
pub fn expanded_travel_time_matrix(
    net: &StreetNetwork,
    layer: &TransitLayer,
    data: &TransitData,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
) -> Result<ExpandedMatrix, RouteError> {
    let ctx = Context::new(net, layer, data, destinations, p)?;
    let dpm = ctx.draws_per_minute;
    let m = p.max_trip_duration;
    per_origin(origins, p.n_threads, &p.cancel, |lat, lon, scratch| {
        let mut rows = Vec::new();
        ctx.origin_paths(lat, lon, scratch, |j, dp| {
            if dp.is_empty() {
                return Ok(());
            }
            let walk_first = dp.walk().first().cloned();
            for k in 0..p.time_window as usize {
                let departure = p.departure_time + 60 * k as i32;
                let mut draw = 0;
                for pb in dp.minute(k)? {
                    draw += 1;
                    rows.push(ExpandedRow {
                        to: j as u32,
                        departure,
                        draw,
                        trip: pb.report(m),
                    });
                }
                if (draw as usize) < dpm {
                    let filler = walk_first
                        .clone()
                        .unwrap_or_else(|| PathBreakdown::unreachable(&ctx.walk_routes));
                    let trip = filler.report(m);
                    for d in draw as usize + 1..=dpm {
                        rows.push(ExpandedRow {
                            to: j as u32,
                            departure,
                            draw: d as u32,
                            trip: trip.clone(),
                        });
                    }
                }
            }
            Ok(())
        })?;
        Ok(rows)
    })
    .map(ExpandedMatrix)
}

/// The service date and departure of an arrival search: r5r subtracts
/// `max_trip_duration` from `arrival_datetime` and searches the window
/// `[departure, arrival)` minute by minute (`arrival_travel_time_matrix.R`).
/// Returns `(yyyymmdd, seconds after midnight)`; an earlier day when the
/// window starts before midnight. Daylight-saving shifts are not modelled.
/// An error when `date` is not a calendar date (see
/// [`validate_date`]), `max_trip_duration` is under 1 minute or the
/// departure falls before year 1.
pub fn arrival_departure(
    date: u32,
    arrival_time: i32,
    max_trip_duration: i32,
) -> Result<(u32, i32), RouteError> {
    validate_date(date)?;
    if max_trip_duration < 1 {
        return Err(RouteError::Invalid(format!(
            "max_trip_duration must be at least 1, got {max_trip_duration}"
        )));
    }
    let t = i64::from(arrival_time) - i64::from(max_trip_duration) * 60;
    if t >= 0 {
        return Ok((date, t as i32));
    }
    let days = t.div_euclid(86_400);
    let date = civil_from_days(days_from_civil(date) + days).ok_or_else(|| {
        format!("max_trip_duration {max_trip_duration} min reaches before year 1")
    })?;
    Ok((date, t.rem_euclid(86_400) as i32))
}

/// r5r's `arrival_travel_time_matrix()`. `p` must already hold the
/// departure side of the search: the service date and departure from
/// [`arrival_departure`] and `time_window = max_trip_duration`, as the R
/// wrapper sets them. For each destination the latest departure minute
/// with a record arriving by `departure + max_trip_duration` wins: its
/// first transit record in `PATH_ORDER`, else the walk-only record
/// (`filterLatestBeforeArrivalTime`, offs 31-266). At most one row per
/// destination. Errors as [`expanded_travel_time_matrix`], and
/// [`RouteError::Invalid`] when `p.time_window` is not
/// `p.max_trip_duration`.
pub fn arrival_travel_time_matrix(
    net: &StreetNetwork,
    layer: &TransitLayer,
    data: &TransitData,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
) -> Result<ExpandedMatrix, RouteError> {
    if p.time_window != p.max_trip_duration {
        return Err("an arrival search needs time_window = max_trip_duration".into());
    }
    let ctx = Context::new(net, layer, data, destinations, p)?;
    let dpm = ctx.draws_per_minute;
    let m = p.max_trip_duration;
    let arrival = p.departure_time + m * 60;
    // Arrival of a record leaving at `dep`: `dep + Math.round(totalTime * 60)`
    // (in i64: a long walk-only total plus a late departure can pass i32).
    let arrival = i64::from(arrival);
    let arrives =
        |dep: i32, pb: &PathBreakdown| i64::from(dep) + java_round(pb.total * 60.0) as i64;
    per_origin(origins, p.n_threads, &p.cancel, |lat, lon, scratch| {
        let mut rows = Vec::new();
        ctx.origin_paths(lat, lon, scratch, |j, dp| {
            if dp.is_empty() {
                return Ok(());
            }
            for k in (0..p.time_window as usize).rev() {
                let departure = p.departure_time + 60 * k as i32;
                let mut count = 0;
                for pb in dp.minute(k)? {
                    count += 1;
                    if arrives(departure, pb) <= arrival {
                        rows.push(ExpandedRow {
                            to: j as u32,
                            departure,
                            draw: count,
                            trip: pb.report(m),
                        });
                        return Ok(());
                    }
                }
                if (count as usize) < dpm {
                    if let Some(pb) = dp.walk().first() {
                        if arrives(departure, pb) <= arrival {
                            rows.push(ExpandedRow {
                                to: j as u32,
                                departure,
                                draw: 1,
                                trip: pb.report(m),
                            });
                            return Ok(());
                        }
                    }
                }
            }
            Ok(())
        })?;
        Ok(rows)
    })
    .map(ExpandedMatrix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_matches_java() {
        assert_eq!(java_round(2.5), 3.0);
        assert_eq!(java_round(2.4999), 2.0);
        assert_eq!(java_round(0.49999999999999994), 0.0);
        assert_eq!(round1(0.25), 0.3);
        // 27 s: (float) 0.45 is just below 0.45, so it rounds down.
        assert_eq!(round1(minutes_f32(27)), 0.4);
        assert_eq!(round1(minutes_f32(9)), 0.2);
        assert_eq!(round1(minutes_f32(126)), 2.1);
    }

    #[test]
    fn total_is_the_sum_of_rounded_components() {
        let pb = PathBreakdown {
            access: minutes_f32(366),
            wait: minutes_f32(504),
            ride: minutes_f32(1938),
            transfer: 0.0,
            egress: minutes_f32(1500),
            total: minutes_f32(4308),
            routes: "T4|262".into(),
            n_rides: 2,
        };
        let b = pb.report(120).unwrap();
        assert_eq!(
            (b.access_time, b.wait_time, b.ride_time, b.egress_time),
            (6.1, 8.4, 32.3, 25.0)
        );
        assert_eq!(b.total_time, 71.8);
        assert_eq!(pb.report(71), None);
    }

    #[test]
    fn walk_and_unreachable_records() {
        let wr: Arc<str> = "[WALK]".into();
        let w = PathBreakdown::walk(&wr, minutes_f32(126))
            .report(120)
            .unwrap();
        assert_eq!(
            (w.total_time, w.routes.as_str(), w.n_rides),
            (2.1, "[WALK]", 0)
        );
        // 7230 s = 120.5 min: over the limit, so NA.
        assert_eq!(
            PathBreakdown::walk(&wr, minutes_f32(7230)).report(120),
            None
        );
        assert_eq!(PathBreakdown::unreachable(&wr).report(120), None);
    }

    #[test]
    fn path_order_sorts_by_total_then_routes() {
        let mk = |total: i32, routes: &str, wait: i32| PathBreakdown {
            wait: minutes_f32(wait),
            total: minutes_f32(total),
            routes: routes.into(),
            ..PathBreakdown::walk(&"[WALK]".into(), 0.0)
        };
        let mut v = [
            mk(600, "b", 0),
            mk(600, "a", 5),
            mk(300, "z", 0),
            mk(600, "a", 1),
        ];
        v.sort_by(path_order);
        let got: Vec<(f64, &str, f64)> = v.iter().map(|p| (p.total, &*p.routes, p.wait)).collect();
        assert_eq!(
            got,
            [
                (5.0, "z", 0.0),
                (10.0, "a", minutes_f32(1)),
                (10.0, "a", minutes_f32(5)),
                (10.0, "b", 0.0)
            ]
        );
    }

    /// The bound in `propagate` skips only stops that cannot improve any
    /// iteration: the times match a propagation over every stop.
    #[test]
    fn propagate_skip_changes_nothing() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = |n: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % n
        };
        let (n_stops, n_iter, max_t) = (30usize, 40usize, 3600);
        let paths = PathArena::default();
        for _ in 0..200 {
            let times: Vec<i32> = (0..n_stops * n_iter)
                .map(|_| match next(4) {
                    0 => UNREACHED,
                    _ => next(5000) as i32,
                })
                .collect();
            let mut stop_min = vec![UNREACHED; n_stops];
            for row in times.chunks_exact(n_stops) {
                for (m, &t) in stop_min.iter_mut().zip(row) {
                    *m = (*m).min(t);
                }
            }
            let egress: Vec<(u32, i32)> = (0..12)
                .map(|_| (next(n_stops as u64) as u32, next(2_000_000) as i32))
                .collect();
            let direct = [UNREACHED, 900, 4000][next(3) as usize];
            let run = |stop_min: &[i32]| {
                let mut best = vec![direct; n_iter];
                let (mut bp, mut be) = (vec![NO_PATH; n_iter], vec![0; n_iter]);
                propagate(
                    &egress, &times, stop_min, n_stops, &paths, 1000, 1200, max_t, &mut best,
                    &mut bp, &mut be,
                );
                best
            };
            // a lower bound of i32::MIN never skips
            assert_eq!(run(&stop_min), run(&vec![i32::MIN; n_stops]));
        }
    }

    #[test]
    fn arrival_departure_crosses_midnight() {
        let ad = |d, t, m| arrival_departure(d, t, m).unwrap();
        assert_eq!(ad(20190513, 16 * 3600, 120), (20190513, 14 * 3600));
        assert_eq!(ad(20190513, 1800, 120), (20190512, 81000));
        assert_eq!(ad(20190301, 0, 60), (20190228, 82800));
        assert_eq!(ad(20200301, 0, 60), (20200229, 82800));
        assert_eq!(ad(20200101, 0, 60), (20191231, 82800));
        // exactly one day back, and several
        assert_eq!(ad(20190513, 0, 1440), (20190512, 0));
        assert_eq!(ad(20190513, 0, 3 * 1440 + 1), (20190509, 86340));
    }

    #[test]
    fn arrival_departure_rejects_what_it_cannot_compute() {
        assert!(arrival_departure(20190001, 0, 120).is_err());
        assert!(arrival_departure(0, 0, 120).is_err());
        assert!(arrival_departure(20190230, 0, 120).is_err());
        assert!(arrival_departure(20190513, 0, 0).is_err());
        assert!(arrival_departure(10101, 0, 1).is_err());
        // i32::MAX minutes is about 4083 years: still a date
        assert_eq!(
            arrival_departure(99991231, 0, i32::MAX).map(|(d, _)| d / 10000),
            Ok(5916)
        );
    }
}
