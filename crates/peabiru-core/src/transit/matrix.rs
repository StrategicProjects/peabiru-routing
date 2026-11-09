//! Public-transport travel-time matrices, as r5r's `travel_time_matrix()`
//! with `mode = c("WALK", "TRANSIT")`: walk access, RAPTOR over the
//! departure window, walk egress, percentiles over the iterations (R5
//! `TravelTimeComputer`, `PerTargetPropagater`, `TravelTimeReducer`).

use super::raptor::{self, Filtered, RaptorRequest, UNREACHED};
use super::walk;
use super::TransitLayer;
use crate::street::link::{find_split, Split};
use crate::street::router::{street_vertex_times, Mode};
use crate::street::StreetNetwork;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Routing parameters with r5r's defaults.
#[derive(Debug, Clone)]
pub struct TransitParams {
    /// Service date `yyyymmdd`.
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
    pub n_threads: usize,
}

impl TransitParams {
    /// Check the parameters before routing. The percentile rules are R5's
    /// (`AnalysisWorkerTask.validatePercentiles`: at most 5, in 1..=99);
    /// the other bounds keep every value an int in seconds and bound what a
    /// parameter can make RAPTOR allocate (iterations x stops per origin).
    pub fn validate(&self) -> Result<(), String> {
        if self.percentiles.is_empty() || self.percentiles.len() > 5 {
            return Err(format!(
                "between 1 and 5 percentiles are allowed, got {}",
                self.percentiles.len()
            ));
        }
        if let Some(p) = self.percentiles.iter().find(|p| !(1..=99).contains(*p)) {
            return Err(format!("percentiles must be in 1..=99, got {p}"));
        }
        let check = |name: &str, v: i64, lo: i64, hi: i64| {
            if (lo..=hi).contains(&v) {
                Ok(())
            } else {
                Err(format!("{name} must be between {lo} and {hi}, got {v}"))
            }
        };
        check("departure_time", self.departure_time.into(), 0, 7 * 86_400)?;
        check("time_window", self.time_window.into(), 1, 1440)?;
        check("draws_per_minute", self.draws_per_minute.into(), 1, 1000)?;
        check(
            "time_window * draws_per_minute",
            i64::from(self.time_window) * i64::from(self.draws_per_minute),
            1,
            100_000,
        )?;
        check("max_walk_time", self.max_walk_time.into(), 1, 1_000_000)?;
        check(
            "max_trip_duration",
            self.max_trip_duration.into(),
            1,
            1_000_000,
        )?;
        check("max_rides", self.max_rides as i64, 1, 100)?;
        let v = self.walk_speed_kmh;
        // (int)(speed m/s * 1000) is a divisor, so it must be >= 1
        if !v.is_finite() || ((v * 5.0 / 18.0) as f32 * 1000.0f32) < 1.0 || v > 1000.0 {
            return Err(format!(
                "walk_speed_kmh must be between 0.0036 and 1000 km/h, got {v}"
            ));
        }
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

/// Day of week of `yyyymmdd`, 0 = Monday.
pub fn weekday(date: u32) -> usize {
    let (y, m, d) = (
        (date / 10000) as i64,
        ((date / 100) % 100) as i64,
        (date % 100) as i64,
    );
    // days from civil (Howard Hinnant), 1970-01-01 is a Thursday
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    ((days + 3).rem_euclid(7)) as usize
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
    dest_splits: Vec<Option<Split>>,
    egress: Vec<Vec<(u32, i32)>>,
    /// `max_trip_duration` in seconds.
    max_t: i32,
    /// `(int)(walkSpeed * 1000)`, mm/s.
    walk_mms: i32,
    /// Access and direct walk search limit, seconds.
    street_limit: i32,
    /// Egress walk limit, seconds.
    egress_limit: i32,
}

/// Per-thread buffers reused across origins.
#[derive(Default)]
pub(crate) struct Scratch {
    split: Vec<u32>,
    times: Vec<i32>,
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
        let active = layer.active_services(p.date, weekday(p.date));
        let filtered = Filtered::new(layer, &active)?;
        let walk_speed = (p.walk_speed_kmh * 5.0 / 18.0) as f32;
        let mut scratch = Vec::new();
        let dest_splits: Vec<Option<Split>> = destinations
            .iter()
            .map(|&(lat, lon)| find_split(net, lat, lon, Mode::Walk, &mut scratch))
            .collect();
        let egress = walk::egress_tables(layer, &data.trees, net, destinations, &dest_splits);
        Ok(Query {
            net,
            layer,
            data,
            p,
            filtered,
            dest_splits,
            egress,
            max_t: p.max_trip_duration * 60,
            walk_mms: (walk_speed * 1000.0f32) as i32,
            street_limit: p.max_trip_duration.min(p.max_walk_time) * 60,
            egress_limit: p.max_walk_time * 60,
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
        let Some(origin) = find_split(net, lat, lon, Mode::Walk, &mut scratch.split) else {
            return false;
        };
        // Walk search: direct times and access to stops.
        let (vertex_time, direct) = street_vertex_times(
            net,
            &origin,
            &self.dest_splits,
            Mode::Walk,
            p.walk_speed_kmh,
            self.street_limit,
        );
        let access: Vec<(u32, i32)> = layer
            .stop_vertex
            .iter()
            .enumerate()
            .filter(|(_, &v)| v >= 0 && vertex_time[v as usize] < self.street_limit)
            .map(|(s, &v)| (s as u32, vertex_time[v as usize]))
            .collect();
        let times = if access.is_empty() {
            None
        } else {
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
            Some(raptor::route(
                layer,
                &self.data.transfers,
                &self.filtered,
                &access,
                &req,
            ))
        };
        let (max_t, egress_limit, walk_mms) = (self.max_t, self.egress_limit, self.walk_mms);
        let tt = &mut scratch.times;
        for (j, &d) in direct.iter().enumerate() {
            // PerTargetPropagater.propagate: start from the non-transit time,
            // improve it by vehicle arrival + egress walk.
            tt.clear();
            match &times {
                None => tt.push(d),
                Some(t) => tt.resize(t.len(), d),
            }
            if let Some(times) = &times {
                for &(stop, mm) in &self.egress[j] {
                    let e = mm / walk_mms;
                    if e >= egress_limit {
                        continue;
                    }
                    for (k, t) in tt.iter_mut().enumerate() {
                        let ts = times[k][stop as usize];
                        if ts >= max_t || ts >= *t {
                            continue;
                        }
                        let c = ts + e;
                        if c < max_t && c < *t {
                            *t = c;
                        }
                    }
                }
            }
            per_destination(j, tt);
        }
        true
    }
}

/// Run `f(origin_index, scratch)` for every origin on `n_threads` threads
/// (0 = all cores), results in origin order.
pub(crate) fn par_origins<T: Send>(
    n_threads: usize,
    n_origins: usize,
    f: impl Fn(usize, &mut Scratch) -> T + Sync,
) -> Vec<T> {
    let n_threads = if n_threads > 0 {
        n_threads
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get())
    }
    .clamp(1, n_origins.max(1));
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
        let ok = TransitParams::default();
        assert!(ok.validate().is_ok());
        for bad in [
            TransitParams {
                percentiles: vec![],
                ..TransitParams::default()
            },
            TransitParams {
                percentiles: vec![100],
                ..TransitParams::default()
            },
            TransitParams {
                time_window: 0,
                ..TransitParams::default()
            },
            TransitParams {
                draws_per_minute: 0,
                ..TransitParams::default()
            },
            TransitParams {
                time_window: 1440,
                draws_per_minute: 1000,
                ..TransitParams::default()
            },
            TransitParams {
                max_trip_duration: 0,
                ..TransitParams::default()
            },
            TransitParams {
                max_rides: 0,
                ..TransitParams::default()
            },
            TransitParams {
                walk_speed_kmh: f64::NAN,
                ..TransitParams::default()
            },
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn percentile_indices_match_r5() {
        assert_eq!(percentile_index(50, 10), 4);
        assert_eq!(percentile_index(50, 50), 24);
        assert_eq!(percentile_index(5, 10), 0);
        assert_eq!(percentile_index(95, 10), 9);
    }
}
