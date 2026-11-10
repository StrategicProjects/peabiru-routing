//! Public-transport travel-time matrices, as r5r's `travel_time_matrix()`
//! with `mode = c("WALK", "TRANSIT")`: walk access, RAPTOR over the
//! departure window, walk egress, percentiles over the iterations (R5
//! `TravelTimeComputer`, `PerTargetPropagater`, `TravelTimeReducer`).

use super::raptor::{self, Filtered, RaptorRequest, StopTimes, UNREACHED};
use super::walk;
use super::TransitLayer;
use crate::street::link::{find_split, Split};
use crate::street::router::{check_speed, search_from, worker_threads, Mode, ModeGraph, Search};
use crate::street::StreetNetwork;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Routing parameters with r5r's defaults.
#[derive(Debug, Clone)]
pub struct TransitParams {
    /// Service date `yyyymmdd`; it must be a real calendar date. The
    /// default, 0, is a placeholder that [`TransitParams::validate`]
    /// rejects: there is no default date, as r5r has no default departure.
    pub date: u32,
    /// Departure, seconds after midnight.
    pub departure_time: i32,
    pub time_window: i32,
    pub percentiles: Vec<i32>,
    pub max_walk_time: i32,
    pub max_trip_duration: i32,
    pub walk_speed_kmh: f64,
    pub max_rides: usize,
    pub draws_per_minute: i32,
    pub seed: Option<i32>,
    /// Worker threads; 0 means all cores, and more than the cores
    /// available run as many as there are cores.
    pub n_threads: usize,
}

impl TransitParams {
    /// Check the parameters before routing. The bounds are those of R5 and
    /// r5r 2.5.0 (percentiles: `AnalysisWorkerTask.validatePercentiles`;
    /// counts: r5r's `checkmate::assert_count(positive = TRUE)` and R5's
    /// `int` fields; walk speed: at least 1 mm/s, as
    /// `TravelTimeComputer` requires), plus what keeps the arithmetic in
    /// R5's `int` seconds: minutes times 60 must fit, and so must the
    /// departure plus the window plus `max_trip_duration`, the latest clock
    /// time a search can hold. A departure is at most 7 days after
    /// midnight of the service day.
    pub fn validate(&self) -> Result<(), String> {
        validate_date(self.date)?;
        validate_percentiles(&self.percentiles)?;
        let check_in = |name: &str, v: i64, lo: i64, hi: i64, unit: &str| {
            if (lo..=hi).contains(&v) {
                Ok(())
            } else {
                Err(format!("{name} must be from {lo} to {hi}{unit}, got {v}"))
            }
        };
        let check = |name: &str, v: i64, lo: i64, hi: i64| check_in(name, v, lo, hi, "");
        let int_max = i64::from(i32::MAX);
        let max_minutes = int_max / 60;
        check_in(
            "departure_time",
            self.departure_time.into(),
            0,
            7 * 86_400,
            " seconds",
        )?;
        check_in(
            "time_window",
            self.time_window.into(),
            1,
            int_max,
            " minutes",
        )?;
        check("draws_per_minute", self.draws_per_minute.into(), 1, int_max)?;
        // r5r hands R5 `as.integer(time_window * draws_per_minute)`
        let draws = i64::from(self.time_window) * i64::from(self.draws_per_minute);
        if draws > int_max {
            return Err(format!(
                "time_window x draws_per_minute ({} x {} = {draws} Monte Carlo draws) must be \
                 at most {int_max}",
                self.time_window, self.draws_per_minute
            ));
        }
        // max_trip_duration first: r5r passes max_walk_time = Inf as
        // max_trip_duration, so a value too large is reported under the
        // name the user set
        let mins = " minutes";
        check_in(
            "max_trip_duration",
            self.max_trip_duration.into(),
            1,
            max_minutes,
            mins,
        )?;
        check_in(
            "max_walk_time",
            self.max_walk_time.into(),
            1,
            max_minutes,
            mins,
        )?;
        let last_clock = i64::from(self.departure_time)
            + 60 * (i64::from(self.time_window) + i64::from(self.max_trip_duration));
        if last_clock > int_max {
            return Err(format!(
                "time_window + max_trip_duration ({} + {} min) after a departure at {} s must \
                 end within {int_max} s (about 68 years); reduce time_window or \
                 max_trip_duration",
                self.time_window, self.max_trip_duration, self.departure_time
            ));
        }
        check(
            "max_rides",
            i64::try_from(self.max_rides).unwrap_or(i64::MAX),
            1,
            int_max,
        )?;
        check_speed("walk_speed_kmh", self.walk_speed_kmh)?;
        Ok(())
    }
}

impl Default for TransitParams {
    fn default() -> Self {
        TransitParams {
            date: 0,
            departure_time: 0,
            time_window: 10,
            percentiles: vec![50],
            max_walk_time: 120,
            max_trip_duration: 120,
            walk_speed_kmh: 3.6,
            max_rides: 3,
            draws_per_minute: 5,
            seed: None,
            n_threads: 0,
        }
    }
}

/// `AnalysisWorkerTask.MAX_PERCENTILES`.
pub const MAX_PERCENTILES: usize = 5;

/// Check travel-time percentiles as R5 does
/// (`AnalysisWorkerTask.validatePercentiles`): 1 to 5 of them, each in
/// 1..=99. R5 wants them ascending and accepts equal neighbours, so in any
/// order and with repeats they pass here (the results sort them). Street
/// modes have one travel time, reported for every percentile; they take
/// the same check, so that an empty list is an error there too.
pub fn validate_percentiles(percentiles: &[i32]) -> Result<(), String> {
    if percentiles.is_empty() || percentiles.len() > MAX_PERCENTILES {
        return Err(format!(
            "between 1 and {MAX_PERCENTILES} percentiles are allowed, got {}",
            percentiles.len()
        ));
    }
    if let Some(p) = percentiles.iter().find(|p| !(1..=99).contains(*p)) {
        return Err(format!("percentiles must be in 1..=99, got {p}"));
    }
    Ok(())
}

/// Days in month `m` (1-12) of year `y`, proleptic Gregorian calendar.
fn days_in_month(y: i64, m: i64) -> i64 {
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    match m {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Check that `yyyymmdd` is a real calendar date (Gregorian, years 1 to
/// 9999): `20190513` passes, `20190230`, `20191301` and `0` do not. The
/// service calendar is looked up by this date, so a date that does not
/// exist would silently match no service.
pub fn validate_date(date: u32) -> Result<(), String> {
    let (y, m, d) = (
        i64::from(date / 10000),
        i64::from((date / 100) % 100),
        i64::from(date % 100),
    );
    if (1..=9999).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m) {
        Ok(())
    } else {
        Err(format!(
            "date must be a calendar date written yyyymmdd, got {date}"
        ))
    }
}

/// Days since 1970-01-01 of a valid `yyyymmdd` (Howard Hinnant's
/// `days_from_civil`).
pub(crate) fn days_from_civil(date: u32) -> i64 {
    let (y, m, d) = (
        i64::from(date / 10000),
        i64::from((date / 100) % 100),
        i64::from(date % 100),
    );
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The `yyyymmdd` of a day count since 1970-01-01 (`civil_from_days`), or
/// `None` outside years 1 to 9999.
pub(crate) fn civil_from_days(days: i64) -> Option<u32> {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    if !(1..=9999).contains(&y) {
        return None;
    }
    u32::try_from(y * 10000 + m * 100 + d).ok()
}

/// Day of week of `yyyymmdd`, 0 = Monday.
pub fn weekday(date: u32) -> usize {
    // 1970-01-01 is a Thursday
    ((days_from_civil(date) + 3).rem_euclid(7)) as usize
}

/// Network data precomputed once and shared by every query.
pub struct TransitData {
    pub transfers: Vec<Vec<(u32, i32)>>,
    pub trees: Vec<Option<Vec<(u32, i32)>>>,
}

impl TransitData {
    pub fn new(net: &StreetNetwork, layer: &TransitLayer) -> Self {
        let trees = walk::stop_trees(net, layer);
        let transfers = walk::find_transfers(layer, &trees);
        TransitData { transfers, trees }
    }
}

/// One origin's results: `[destination][percentile]`.
type Row = Vec<Vec<Option<u32>>>;

/// Percentile index into `n` sorted values (R5: `ceil(p / 100 * n) - 1`).
pub(crate) fn percentile_index(p: i32, n: usize) -> usize {
    ((f64::from(p) / 100.0 * n as f64).ceil() as i64 - 1).max(0) as usize
}

/// Everything a transit query needs that does not depend on the origin:
/// the request in R5's units, the filtered patterns, the destination
/// linkage and egress tables. Shared by the travel-time matrix and
/// accessibility (R5 `TravelTimeComputer` up to the reducer).
pub(crate) struct Query<'a> {
    net: &'a StreetNetwork,
    layer: &'a TransitLayer,
    data: &'a TransitData,
    p: &'a TransitParams,
    filtered: Filtered,
    /// The walking graph of the access and direct search.
    walk: ModeGraph,
    dest_splits: Vec<Option<Split>>,
    /// Per destination, `(stop, egress walk seconds)` for the stops within
    /// the egress walk limit, in increasing stop order.
    egress: Vec<Vec<(u32, i32)>>,
    /// `max_trip_duration` in seconds.
    max_t: i32,
    /// `(int)(walkSpeed * 1000)`, mm/s.
    walk_mms: i32,
    /// Access and direct walk search limit, seconds.
    street_limit: i32,
}

/// Per-thread buffers reused across origins.
#[derive(Default)]
pub(crate) struct Scratch {
    split: Vec<u32>,
    times: Vec<i32>,
    search: Option<Search>,
    direct: Vec<i32>,
    access: Vec<(u32, i32)>,
    stop_times: StopTimes,
}

impl<'a> Query<'a> {
    pub(crate) fn new(
        net: &'a StreetNetwork,
        layer: &'a TransitLayer,
        data: &'a TransitData,
        destinations: &[(f64, f64)],
        p: &'a TransitParams,
    ) -> Result<Self, String> {
        p.validate()?;
        // RAPTOR keeps iterations x stops durations per origin; their byte
        // count must be a valid allocation size
        let per_minute = if layer.has_frequencies {
            p.draws_per_minute as u64
        } else {
            1
        };
        let cells = p.time_window as u64 * per_minute * layer.stop_ids.len() as u64;
        if cells > isize::MAX as u64 / 4 {
            return Err(format!(
                "time_window x draws_per_minute x stops ({cells}) is too large to allocate"
            ));
        }
        let active = layer.active_services(p.date, weekday(p.date));
        let filtered = Filtered::new(layer, &active)?;
        let walk_speed = (p.walk_speed_kmh * 5.0 / 18.0) as f32;
        let mut scratch = Vec::new();
        let dest_splits: Vec<Option<Split>> = destinations
            .iter()
            .map(|&(lat, lon)| find_split(net, lat, lon, Mode::Walk, &mut scratch))
            .collect();
        let walk_mms = (walk_speed * 1000.0f32) as i32;
        let egress_limit = p.max_walk_time * 60;
        // PerTargetPropagater: egress seconds `mm / walkSpeedMillimetersPerSecond`,
        // stops at or beyond the egress limit skipped.
        let egress = walk::egress_tables(layer, &data.trees, net, destinations, &dest_splits)
            .into_iter()
            .map(|table| {
                table
                    .into_iter()
                    .map(|(stop, mm)| (stop, mm / walk_mms))
                    .filter(|&(_, e)| e < egress_limit)
                    .collect()
            })
            .collect();
        Ok(Query {
            net,
            layer,
            data,
            p,
            filtered,
            walk: ModeGraph::with_walk_speed(net, Mode::Walk, p.walk_speed_kmh),
            dest_splits,
            egress,
            max_t: p.max_trip_duration * 60,
            walk_mms,
            street_limit: p.max_trip_duration.min(p.max_walk_time) * 60,
        })
    }

    /// Route from one origin and hand each destination's travel times in
    /// seconds to `per_destination(j, times)`, destinations in order: one
    /// value per RAPTOR iteration, or a single (unvarying) value when no
    /// stop was reached on foot, `UNREACHED` where unreachable. The slice
    /// may be reordered by the callee. Returns `false`, without calling it,
    /// when the origin cannot be linked to the street network (R5 then
    /// records nothing for the origin).
    pub(crate) fn route_origin(
        &self,
        (lat, lon): (f64, f64),
        scratch: &mut Scratch,
        mut per_destination: impl FnMut(usize, &mut [i32]),
    ) -> bool {
        let (net, layer, p) = (self.net, self.layer, self.p);
        let Scratch {
            split,
            times: tt,
            search,
            direct,
            access,
            stop_times,
        } = scratch;
        let Some(origin) = find_split(net, lat, lon, Mode::Walk, split) else {
            return false;
        };
        // Walk search: direct times and access to stops.
        let search = search.get_or_insert_with(|| Search::new(net));
        search_from(
            net,
            &self.walk,
            &origin,
            &self.dest_splits,
            self.street_limit,
            search,
            direct,
        );
        let vertex_time = search.vertex_time();
        access.clear();
        access.extend(
            layer
                .stop_vertex
                .iter()
                .enumerate()
                .filter(|(_, &v)| v >= 0 && vertex_time[v as usize] < self.street_limit)
                .map(|(s, &v)| (s as u32, vertex_time[v as usize])),
        );
        let transit = !access.is_empty();
        if transit {
            let req = RaptorRequest {
                from_time: p.departure_time,
                time_window: p.time_window,
                max_rides: p.max_rides,
                max_duration: self.max_t,
                walk_mms: self.walk_mms,
                max_walk_min: p.max_walk_time,
                monte_carlo_draws: p.time_window * p.draws_per_minute,
                seed: p.seed,
                origin: (lat, lon),
            };
            raptor::route_stop_major(
                layer,
                &self.data.transfers,
                &self.filtered,
                access,
                &req,
                stop_times,
            );
        }
        let max_t = self.max_t;
        for (j, &d) in direct.iter().enumerate() {
            // PerTargetPropagater.propagate: start from the non-transit time,
            // improve it by vehicle arrival + egress walk. Only the least
            // candidate per iteration matters, so the order of the stops
            // does not.
            tt.clear();
            if !transit {
                tt.push(d);
            } else {
                tt.resize(stop_times.n_iterations(), d);
                // No candidate at or above `bound` can improve any
                // iteration: the smaller of max_trip_duration and the worst
                // time so far.
                let mut bound = max_t.min(d);
                for &(stop, e) in &self.egress[j] {
                    let stop = stop as usize;
                    // every iteration's candidate is at least the stop's
                    // least duration plus the egress walk
                    if stop_times.min(stop).saturating_add(e) >= bound {
                        continue;
                    }
                    let mut worst = 0;
                    for (t, &ts) in tt.iter_mut().zip(stop_times.get(stop)) {
                        // ts + e < max_t also means ts < max_t
                        let c = ts.saturating_add(e);
                        let c = if c < max_t { c } else { UNREACHED };
                        *t = (*t).min(c);
                        worst = worst.max(*t);
                    }
                    bound = max_t.min(worst);
                }
            }
            per_destination(j, tt);
        }
        true
    }
}

/// Run `f(origin_index, scratch)` for every origin on `n_threads` threads
/// (0 = all cores; capped at the cores and the origins), results in origin
/// order.
pub(crate) fn par_origins<T: Send>(
    n_threads: usize,
    n_origins: usize,
    f: impl Fn(usize, &mut Scratch) -> T + Sync,
) -> Vec<T> {
    let n_threads = worker_threads(n_threads, n_origins);
    let next = AtomicUsize::new(0);
    let chunks: Vec<Vec<(usize, T)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..n_threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    let mut scratch = Scratch::default();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= n_origins {
                            break;
                        }
                        done.push((i, f(i, &mut scratch)));
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
    let mut slots: Vec<Option<T>> = (0..n_origins).map(|_| None).collect();
    for (i, r) in chunks.into_iter().flatten() {
        slots[i] = Some(r);
    }
    slots
        .into_iter()
        .map(|r| r.expect("every origin is routed"))
        .collect()
}

/// Travel times in minutes, `[origin][destination][percentile]`, `None`
/// where r5r reports NA; a pair is reported when its lowest percentile is.
/// The percentiles are returned in ascending order, whatever the order of
/// `p.percentiles`; a repeated percentile, which R5 accepts, gets one
/// column per occurrence (see [`validate_percentiles`]).
pub fn travel_time_matrix(
    net: &StreetNetwork,
    layer: &TransitLayer,
    data: &TransitData,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
) -> Result<Vec<Row>, String> {
    let q = Query::new(net, layer, data, destinations, p)?;
    let m = p.max_trip_duration;
    let mut percentiles = p.percentiles.clone();
    percentiles.sort_unstable();
    let nd = destinations.len();
    Ok(par_origins(p.n_threads, origins.len(), |i, scratch| {
        let mut row = vec![vec![None; percentiles.len()]; nd];
        q.route_origin(origins[i], scratch, |j, tt| {
            // TravelTimeReducer.extractTravelTimePercentilesAndRecord
            tt.sort_unstable();
            let n = tt.len();
            let vals: Vec<i32> = percentiles
                .iter()
                .map(|&pc| {
                    let v = tt[percentile_index(pc, n).min(n - 1)];
                    if v == UNREACHED {
                        UNREACHED
                    } else {
                        v / 60
                    }
                })
                .collect();
            if vals[0] <= m {
                for (c, &v) in row[j].iter_mut().zip(&vals) {
                    if v <= m {
                        *c = Some(v as u32);
                    }
                }
            }
        });
        row
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekdays() {
        assert_eq!(weekday(20190513), 0); // Monday
        assert_eq!(weekday(19700101), 3); // Thursday
        assert_eq!(weekday(20240229), 3); // Thursday
        assert_eq!(weekday(20261109), 0); // Monday
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        let ok = TransitParams {
            date: 20190513,
            ..TransitParams::default()
        };
        assert!(ok.validate().is_ok());
        for bad in [
            TransitParams {
                percentiles: vec![],
                ..ok.clone()
            },
            TransitParams {
                percentiles: vec![100],
                ..ok.clone()
            },
            TransitParams {
                time_window: 0,
                ..ok.clone()
            },
            TransitParams {
                draws_per_minute: 0,
                ..ok.clone()
            },
            TransitParams {
                time_window: 1 << 16,
                draws_per_minute: 1 << 16,
                ..ok.clone()
            },
            TransitParams {
                max_trip_duration: i32::MAX / 60 + 1,
                max_walk_time: i32::MAX / 60 + 1,
                ..ok.clone()
            },
            TransitParams {
                time_window: i32::MAX / 100,
                max_trip_duration: i32::MAX / 100,
                ..ok.clone()
            },
            TransitParams {
                max_rides: i32::MAX as usize + 1,
                ..ok.clone()
            },
            TransitParams {
                walk_speed_kmh: 0.0,
                ..ok.clone()
            },
            TransitParams {
                max_trip_duration: 0,
                ..ok.clone()
            },
            TransitParams {
                max_rides: 0,
                ..ok.clone()
            },
            TransitParams {
                walk_speed_kmh: f64::NAN,
                ..ok.clone()
            },
            TransitParams {
                date: 0,
                ..ok.clone()
            },
            TransitParams {
                date: 20190230,
                ..ok.clone()
            },
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
    }

    /// Values r5r 2.5.0 accepts that earlier caps rejected, and the
    /// messages of what remains bounded.
    #[test]
    fn bounds_are_r5s_and_the_int_clock() {
        let ok = TransitParams {
            date: 20190513,
            departure_time: 14 * 3600,
            ..TransitParams::default()
        };
        for fine in [
            TransitParams {
                time_window: 2000,
                ..ok.clone()
            },
            TransitParams {
                time_window: 1000,
                draws_per_minute: 1000,
                ..ok.clone()
            },
            TransitParams {
                max_rides: 1000,
                ..ok.clone()
            },
            TransitParams {
                max_trip_duration: 5_000_000,
                max_walk_time: 5_000_000,
                ..ok.clone()
            },
            TransitParams {
                walk_speed_kmh: 5000.0,
                ..ok.clone()
            },
        ] {
            assert_eq!(fine.validate(), Ok(()), "{fine:?}");
        }
        let err = |p: TransitParams| p.validate().unwrap_err();
        // max_walk_time = Inf in r5r is max_trip_duration: the message
        // names max_trip_duration
        let huge = err(TransitParams {
            max_trip_duration: i32::MAX,
            max_walk_time: i32::MAX,
            ..ok.clone()
        });
        assert!(
            huge.starts_with("max_trip_duration must be from 1 to 35791394"),
            "{huge}"
        );
        let clock = err(TransitParams {
            time_window: 30_000_000,
            max_trip_duration: 10_000_000,
            ..ok.clone()
        });
        assert!(
            clock.starts_with("time_window + max_trip_duration"),
            "{clock}"
        );
        let speed = err(TransitParams {
            walk_speed_kmh: 1e-300,
            ..ok.clone()
        });
        assert_eq!(
            speed,
            "walk_speed_kmh must be at least 0.0036 km/h (1 mm/s), got 1e-300"
        );
    }

    #[test]
    fn percentiles_follow_r5() {
        assert!(validate_percentiles(&[50]).is_ok());
        assert!(validate_percentiles(&[75, 25, 50]).is_ok());
        // R5 checks `percentiles[p] >= percentiles[p - 1]`: repeats pass
        assert!(validate_percentiles(&[50, 50]).is_ok());
        assert_eq!(
            validate_percentiles(&[]).unwrap_err(),
            "between 1 and 5 percentiles are allowed, got 0"
        );
        assert!(validate_percentiles(&[1, 2, 3, 4, 5, 6]).is_err());
        assert!(validate_percentiles(&[0]).is_err());
        assert!(validate_percentiles(&[100]).is_err());
    }

    #[test]
    fn dates_are_calendar_dates() {
        for ok in [20190513, 20200229, 20000229, 19700101, 10101, 99991231] {
            assert!(validate_date(ok).is_ok(), "{ok}");
        }
        for bad in [
            0, 20190230, 20190229, 19000229, 20191301, 20190500, 20190001, 101,
        ] {
            assert!(validate_date(bad).is_err(), "{bad}");
        }
        assert_eq!(
            validate_date(20190230).unwrap_err(),
            "date must be a calendar date written yyyymmdd, got 20190230"
        );
    }

    #[test]
    fn day_counts_round_trip() {
        assert_eq!(days_from_civil(19700101), 0);
        assert_eq!(civil_from_days(-1), Some(19691231));
        for date in [10101, 20000229, 20190513, 20191231, 99991231] {
            assert_eq!(civil_from_days(days_from_civil(date)), Some(date));
        }
        assert_eq!(civil_from_days(days_from_civil(10101) - 1), None);
        assert_eq!(civil_from_days(days_from_civil(99991231) + 1), None);
    }

    #[test]
    fn percentile_indices_match_r5() {
        assert_eq!(percentile_index(50, 10), 4);
        assert_eq!(percentile_index(50, 50), 24);
        assert_eq!(percentile_index(5, 10), 0);
        assert_eq!(percentile_index(95, 10), 9);
    }
}
