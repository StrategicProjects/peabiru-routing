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

use super::matrix::{weekday, TransitData, TransitParams};
use super::raptor::{self, Filtered, PathResults, RaptorRequest, TransitPath, UNREACHED};
use super::walk;
use super::TransitLayer;
use crate::jcompat::TroveIntKeys;
use crate::street::link::{find_split, Split};
use crate::street::router::{street_vertex_times, Mode};
use crate::street::StreetNetwork;
use std::cmp::Ordering;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

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
    routes: String,
    n_rides: u32,
}

impl PathBreakdown {
    /// A walk-only record: every component 0, `routes = directModes`.
    fn walk(total: f64) -> Self {
        PathBreakdown {
            access: 0.0,
            wait: 0.0,
            ride: 0.0,
            transfer: 0.0,
            egress: 0.0,
            total,
            routes: "[WALK]".to_string(),
            n_rides: 0,
        }
    }

    /// `PathBreakdown.unreachable()`: total `Integer.MAX_VALUE` minutes.
    fn unreachable() -> Self {
        Self::walk(f64::from(i32::MAX))
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
            routes: self.routes.clone(),
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
            draws_per_minute,
        })
    }

    /// The records of every destination from origin `(lat, lon)`, or `None`
    /// when the origin is not linked (R5 then reports nothing).
    #[allow(clippy::type_complexity)]
    fn origin_paths(
        &self,
        lat: f64,
        lon: f64,
        scratch: &mut Vec<u32>,
    ) -> Result<Option<Vec<DestinationPaths>>, String> {
        let p = self.p;
        let m = p.max_trip_duration;
        let max_t = m * 60;
        let tw = p.time_window as usize;
        let Some(origin) = find_split(self.net, lat, lon, Mode::Walk, scratch) else {
            return Ok(None);
        };
        let street_limit = m.min(p.max_walk_time) * 60;
        let egress_limit = p.max_walk_time * 60;
        let (vertex_time, direct) = street_vertex_times(
            self.net,
            &origin,
            &self.dest_splits,
            Mode::Walk,
            p.walk_speed_kmh,
            street_limit,
        );
        let access: Vec<(u32, i32)> = self
            .layer
            .stop_vertex
            .iter()
            .enumerate()
            .filter(|(_, &v)| v >= 0 && vertex_time[v as usize] < street_limit)
            .map(|(s, &v)| (s as u32, vertex_time[v as usize]))
            .collect();
        let empty = || DestinationPaths {
            by_minute: vec![Vec::new(); tw],
            walk: Vec::new(),
        };
        if access.is_empty() {
            // No stop reached: R5 records the direct time as every percentile
            // and no path result (`TravelTimeComputer.java:304-315`); r5r then
            // emits a walk record of `percentile minutes` when <= M
            // (`extractPathResults` offs 528-604; totalTime is `(double)
            // minutes`).
            return Ok(Some(
                direct
                    .iter()
                    .map(|&d| {
                        let mut dp = empty();
                        let minutes = if d == UNREACHED { UNREACHED } else { d / 60 };
                        if minutes <= m {
                            dp.walk.push(PathBreakdown::walk(f64::from(minutes)));
                        }
                        dp
                    })
                    .collect(),
            ));
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
        let (times, paths) = raptor::route_with_paths(
            self.layer,
            &self.data.transfers,
            &self.filtered,
            &access,
            &req,
            &self.retain,
        );
        let per_minute = req.iterations_per_minute(self.layer.has_frequencies);
        let n_iter = times.len();
        let to_time = p.departure_time + p.time_window * 60;
        let departure_of = |i: usize| to_time - 60 * (1 + (i / per_minute) as i32);
        let mut access_time = vec![-1; self.layer.stop_ids.len()];
        for &(s, t) in &access {
            access_time[s as usize] = t;
        }
        let mut out = Vec::with_capacity(direct.len());
        let mut best = vec![0; n_iter];
        let mut best_path: Vec<Option<&TransitPath>> = vec![None; n_iter];
        let mut best_egress = vec![0; n_iter];
        for (j, &d) in direct.iter().enumerate() {
            best.iter_mut().for_each(|t| *t = d);
            best_path.iter_mut().for_each(|x| *x = None);
            propagate(
                &self.egress[j],
                &times,
                &paths,
                self.walk_mms,
                egress_limit,
                max_t,
                &mut best,
                &mut best_path,
                &mut best_egress,
            );
            // `recordPathsForTarget` + `extractPathResults`.
            let mut dp = empty();
            for i in 0..n_iter {
                let dep = departure_of(i);
                match best_path[i] {
                    Some(path) => {
                        let pb = transit_breakdown(
                            self.layer,
                            path,
                            dep,
                            &access_time,
                            best_egress[i],
                            best[i],
                        )?;
                        let minute = ((dep - p.departure_time) / 60) as usize;
                        dp.by_minute[minute].push(pb);
                    }
                    None if best[i] < UNREACHED => {
                        dp.walk.push(PathBreakdown::walk(minutes_f32(best[i])));
                    }
                    None => {}
                }
            }
            for list in &mut dp.by_minute {
                list.sort_by(path_order); // stable, as `List.sort`
            }
            dp.walk.sort_by(path_order);
            out.push(dp);
        }
        Ok(Some(out))
    }
}

/// `PerTargetPropagater.propagateTransit` for one destination, keeping the
/// path and egress time behind each iteration's best time. Only a strict
/// improvement replaces them, so equal times keep the stop met first.
#[allow(clippy::too_many_arguments)]
fn propagate<'p>(
    egress: &[(u32, i32)],
    times: &[Vec<i32>],
    paths: &'p PathResults,
    walk_mms: i32,
    egress_limit: i32,
    max_t: i32,
    best: &mut [i32],
    best_path: &mut [Option<&'p TransitPath>],
    best_egress: &mut [i32],
) {
    for &(stop, mm) in egress {
        let e = mm / walk_mms;
        if e >= egress_limit {
            continue;
        }
        for (i, t) in best.iter_mut().enumerate() {
            let ts = times[i][stop as usize];
            if ts >= max_t || ts >= *t {
                continue;
            }
            let c = ts + e;
            if c < max_t && c < *t {
                *t = c;
                if let Some(path) = paths.get(i, stop as usize) {
                    best_path[i] = Some(path);
                    best_egress[i] = e;
                }
            }
        }
    }
}

/// The breakdown of one transit iteration (`extractPathResults` offs
/// 162-393): access to the first boarding stop, waits derived from the
/// departure (`Path.computeWaitTimes`), rides, transfers and egress.
fn transit_breakdown(
    layer: &TransitLayer,
    path: &TransitPath,
    departure: i32,
    access_time: &[i32],
    egress: i32,
    total: i32,
) -> Result<PathBreakdown, String> {
    let access = path
        .board_stops
        .first()
        .map(|&s| access_time[s as usize])
        .filter(|&t| t >= 0)
        .ok_or("path without access to its first boarding stop (R5 fails here too)")?;
    let waits = path
        .wait_times(departure, access)
        .ok_or("path with a negative wait (R5 fails here too)")?;
    let routes = path
        .patterns
        .iter()
        .map(|&pi| layer.patterns[pi as usize].route_id.as_str())
        .collect::<Vec<_>>()
        .join("|");
    Ok(PathBreakdown {
        access: minutes_f32(access),
        wait: minutes_f32(waits.iter().sum()),
        ride: minutes_f32(path.ride_times.iter().sum()),
        transfer: minutes_f32(path.transfer_times.iter().sum()),
        egress: minutes_f32(egress),
        total: minutes_f32(total),
        routes,
        n_rides: path.ride_times.len() as u32,
    })
}

fn validate(layer: &TransitLayer, n_destinations: usize, p: &TransitParams) -> Result<(), String> {
    if n_destinations > MAX_PATH_DESTINATIONS {
        return Err(format!(
            "at most {MAX_PATH_DESTINATIONS} destinations are supported with paths"
        ));
    }
    if p.time_window < 1 {
        return Err("time_window must be at least 1 minute".into());
    }
    if p.draws_per_minute < 1 {
        return Err("draws_per_minute must be at least 1".into());
    }
    if p.max_trip_duration < 1 || p.max_walk_time < 1 {
        return Err("max_trip_duration and max_walk_time must be positive".into());
    }
    if p.max_rides < 1 {
        return Err("max_rides must be at least 1".into());
    }
    if p.walk_speed_kmh.is_nan() || p.walk_speed_kmh <= 0.0 {
        return Err("walk_speed must be positive".into());
    }
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
    f: impl Fn(f64, f64, &mut Vec<u32>) -> Result<T, String> + Sync,
) -> Result<Vec<T>, String> {
    let n_threads = if n_threads > 0 {
        n_threads
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get())
    }
    .clamp(1, origins.len().max(1));
    let next = AtomicUsize::new(0);
    let done: Vec<Vec<(usize, Result<T, String>)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..n_threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    let mut scratch = Vec::new();
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
        let Some(dests) = ctx.origin_paths(lat, lon, scratch)? else {
            return Ok(rows);
        };
        for (j, dp) in dests.iter().enumerate() {
            if dp.is_empty() {
                continue;
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
                        .unwrap_or_else(PathBreakdown::unreachable);
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
        }
        Ok(rows)
    })
}

/// The service date and departure of an arrival search: r5r subtracts
/// `max_trip_duration` from `arrival_datetime` and searches the window
/// `[departure, arrival)` minute by minute (`arrival_travel_time_matrix.R`).
/// Returns `(yyyymmdd, seconds after midnight)`; the day before when the
/// window starts before midnight. Daylight-saving shifts are not modelled.
pub fn arrival_departure(date: u32, arrival_time: i32, max_trip_duration: i32) -> (u32, i32) {
    let mut date = date;
    let mut t = arrival_time - max_trip_duration * 60;
    while t < 0 {
        date = previous_day(date);
        t += 86_400;
    }
    (date, t)
}

/// The day before `yyyymmdd`.
fn previous_day(date: u32) -> u32 {
    let (y, m, d) = (date / 10000, (date / 100) % 100, date % 100);
    if d > 1 {
        return date - 1;
    }
    let (y, m) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    y * 10000 + m * 100 + days
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
    // Arrival of a record leaving at `dep`: `dep + Math.round(totalTime * 60)`.
    let arrives = |dep: i32, pb: &PathBreakdown| dep + java_round(pb.total * 60.0) as i32;
    per_origin(origins, p.n_threads, |lat, lon, scratch| {
        let mut rows = Vec::new();
        let Some(dests) = ctx.origin_paths(lat, lon, scratch)? else {
            return Ok(rows);
        };
        'dest: for (j, dp) in dests.iter().enumerate() {
            if dp.is_empty() {
                continue;
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
                        continue 'dest;
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
                            continue 'dest;
                        }
                    }
                }
            }
        }
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
        let w = PathBreakdown::walk(minutes_f32(126)).report(120).unwrap();
        assert_eq!(
            (w.total_time, w.routes.as_str(), w.n_rides),
            (2.1, "[WALK]", 0)
        );
        // 7230 s = 120.5 min: over the limit, so NA.
        assert_eq!(PathBreakdown::walk(minutes_f32(7230)).report(120), None);
        assert_eq!(PathBreakdown::unreachable().report(120), None);
    }

    #[test]
    fn path_order_sorts_by_total_then_routes() {
        let mk = |total: i32, routes: &str, wait: i32| PathBreakdown {
            wait: minutes_f32(wait),
            total: minutes_f32(total),
            routes: routes.into(),
            ..PathBreakdown::walk(0.0)
        };
        let mut v = [
            mk(600, "b", 0),
            mk(600, "a", 5),
            mk(300, "z", 0),
            mk(600, "a", 1),
        ];
        v.sort_by(path_order);
        let got: Vec<(f64, &str, f64)> = v
            .iter()
            .map(|p| (p.total, p.routes.as_str(), p.wait))
            .collect();
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
        assert_eq!(
            arrival_departure(20190513, 16 * 3600, 120),
            (20190513, 14 * 3600)
        );
        assert_eq!(arrival_departure(20190513, 1800, 120), (20190512, 81000));
        assert_eq!(arrival_departure(20190301, 0, 60), (20190228, 82800));
        assert_eq!(arrival_departure(20200301, 0, 60), (20200229, 82800));
        assert_eq!(arrival_departure(20200101, 0, 60), (20191231, 82800));
    }
}
