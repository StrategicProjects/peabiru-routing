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
use crate::street::link::{find_split, Split};
use crate::street::router::{search_from, worker_threads, Mode, ModeGraph, Search};
use crate::street::StreetNetwork;
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
pub struct Breakdown {
    pub access_time: f64,
    pub wait_time: f64,
    pub ride_time: f64,
    pub transfer_time: f64,
    pub egress_time: f64,
    /// Route ids joined by `|`, or `[WALK]` for a walk-only trip.
    pub routes: String,
    pub n_rides: u32,
    pub total_time: f64,
}

/// One output row.
#[derive(Debug, Clone, PartialEq)]
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
    /// Indexed by minute of the window (0 = first departure).
    by_minute: Vec<Vec<PathBreakdown>>,
    walk: Vec<PathBreakdown>,
}

impl DestinationPaths {
    fn is_empty(&self) -> bool {
        self.walk.is_empty() && self.by_minute.iter().all(Vec::is_empty)
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
    ) -> Result<Self, String> {
        validate(layer, destinations.len(), p)?;
        let active = layer.active_services(p.date, weekday(p.date));
        let filtered = Filtered::new(layer, &active)?;
        let walk_speed = (p.walk_speed_kmh * 5.0 / 18.0) as f32;
        let walk_mms = (walk_speed * 1000.0f32) as i32;
        let mut scratch = Vec::new();
        let dest_splits: Vec<Option<Split>> = destinations
            .iter()
            .map(|&(lat, lon)| find_split(net, lat, lon, Mode::Walk, &mut scratch))
            .collect();
        let egress = walk::egress_tables(layer, &data.trees, net, destinations, &dest_splits);
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
        mut on_destination: impl FnMut(usize, &DestinationPaths) -> Result<(), String>,
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
        let clear = |dp: &mut DestinationPaths| {
            dp.by_minute.iter_mut().for_each(Vec::clear);
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
                on_destination(j, dp)?;
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
            &mut sc.times,
            &mut sc.arena,
        );
        let (times, paths) = (&sc.times, &sc.arena);
        let n_stops = self.layer.stop_ids.len();
        let per_minute = req.iterations_per_minute(self.layer.has_frequencies)?;
        let n_iter = times.len() / n_stops.max(1);
        let to_time = p.departure_time + p.time_window * 60;
        let departure_of = |i: usize| to_time - 60 * (1 + (i / per_minute) as i32);
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
            best.iter_mut().for_each(|t| *t = d);
            best_path.iter_mut().for_each(|x| *x = NO_PATH);
            propagate(
                &self.egress[j],
                times,
                n_stops,
                paths,
                self.walk_mms,
                egress_limit,
                max_t,
                &mut best,
                &mut best_path,
                &mut best_egress,
            );
            // `recordPathsForTarget` + `extractPathResults`.
            clear(dp);
            for i in 0..n_iter {
                let dep = departure_of(i);
                match best_path[i] {
                    NO_PATH if best[i] < UNREACHED => {
                        dp.walk
                            .push(PathBreakdown::walk(&self.walk_routes, minutes_f32(best[i])));
                    }
                    NO_PATH => {}
                    k => {
                        let path = paths.legs(k);
                        let route_list = routes[k as usize]
                            .get_or_insert_with(|| route_list(self.layer, path))
                            .clone();
                        let pb = transit_breakdown(
                            path,
                            route_list,
                            dep,
                            &access_time,
                            best_egress[i],
                            best[i],
                        )?;
                        let minute = ((dep - p.departure_time) / 60) as usize;
                        dp.by_minute[minute].push(pb);
                    }
                }
            }
            for list in &mut dp.by_minute {
                list.sort_by(path_order); // stable, as `List.sort`
            }
            dp.walk.sort_by(path_order);
            on_destination(j, dp)?;
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
}

/// `PerTargetPropagater.propagateTransit` for one destination, keeping the
/// path (an index into `paths.paths`) and egress time behind each
/// iteration's best time. Only a strict improvement replaces them, so equal
/// times keep the stop met first.
#[allow(clippy::too_many_arguments)]
fn propagate(
    egress: &[(u32, i32)],
    times: &[i32],
    n_stops: usize,
    paths: &PathArena,
    walk_mms: i32,
    egress_limit: i32,
    max_t: i32,
    best: &mut [i32],
    best_path: &mut [u32],
    best_egress: &mut [i32],
) {
    for &(stop, mm) in egress {
        let e = mm / walk_mms;
        if e >= egress_limit {
            continue;
        }
        for (i, t) in best.iter_mut().enumerate() {
            let ts = times[i * n_stops + stop as usize];
            if ts >= max_t || ts >= *t {
                continue;
            }
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
    p.validate()?;
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

/// Run `f` for every origin on `p.n_threads` threads, results in origin
/// order.
fn per_origin<T: Send>(
    origins: &[(f64, f64)],
    n_threads: usize,
    f: impl Fn(f64, f64, &mut OriginScratch) -> Result<T, String> + Sync,
) -> Result<Vec<T>, String> {
    let n_threads = worker_threads(n_threads, origins.len());
    let next = AtomicUsize::new(0);
    let done: Vec<Vec<(usize, Result<T, String>)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..n_threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    let mut scratch = OriginScratch::default();
                    loop {
                        let i = next.fetch_add(1, AtomicOrdering::Relaxed);
                        if i >= origins.len() {
                            break;
                        }
                        let (lat, lon) = origins[i];
                        done.push((i, f(lat, lon, &mut scratch)));
                    }
                    done
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("transit routing thread panicked"))
            .collect()
    });
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
/// destination with no record at all has no rows.
pub fn expanded_travel_time_matrix(
    net: &StreetNetwork,
    layer: &TransitLayer,
    data: &TransitData,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
) -> Result<Vec<Vec<ExpandedRow>>, String> {
    let ctx = Context::new(net, layer, data, destinations, p)?;
    let dpm = ctx.draws_per_minute;
    let m = p.max_trip_duration;
    per_origin(origins, p.n_threads, |lat, lon, scratch| {
        let mut rows = Vec::new();
        ctx.origin_paths(lat, lon, scratch, |j, dp| {
            if dp.is_empty() {
                return Ok(());
            }
            for (k, list) in dp.by_minute.iter().enumerate() {
                let departure = p.departure_time + 60 * k as i32;
                let mut draw = 0;
                for pb in list {
                    draw += 1;
                    rows.push(ExpandedRow {
                        to: j as u32,
                        departure,
                        draw,
                        trip: pb.report(m),
                    });
                }
                if (draw as usize) < dpm {
                    let filler = dp
                        .walk
                        .first()
                        .cloned()
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
) -> Result<(u32, i32), String> {
    validate_date(date)?;
    if max_trip_duration < 1 {
        return Err(format!(
            "max_trip_duration must be at least 1, got {max_trip_duration}"
        ));
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
/// destination.
pub fn arrival_travel_time_matrix(
    net: &StreetNetwork,
    layer: &TransitLayer,
    data: &TransitData,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
) -> Result<Vec<Vec<ExpandedRow>>, String> {
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
    per_origin(origins, p.n_threads, |lat, lon, scratch| {
        let mut rows = Vec::new();
        ctx.origin_paths(lat, lon, scratch, |j, dp| {
            if dp.is_empty() {
                return Ok(());
            }
            for k in (0..p.time_window as usize).rev() {
                let departure = p.departure_time + 60 * k as i32;
                let mut count = 0;
                for pb in &dp.by_minute[k] {
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
                    if let Some(pb) = dp.walk.first() {
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
