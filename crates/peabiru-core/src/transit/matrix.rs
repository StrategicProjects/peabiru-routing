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
fn percentile_index(p: i32, n: usize) -> usize {
    ((f64::from(p) / 100.0 * n as f64).ceil() as i64 - 1).max(0) as usize
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
    let active = layer.active_services(p.date, weekday(p.date));
    let filtered = Filtered::new(layer, &active)?;
    let m = p.max_trip_duration;
    let max_t = m * 60;
    let walk_speed = (p.walk_speed_kmh * 5.0 / 18.0) as f32;
    let walk_mms = (walk_speed * 1000.0f32) as i32;
    let street_limit = m.min(p.max_walk_time) * 60;
    let egress_limit = p.max_walk_time * 60;
    let mut percentiles = p.percentiles.clone();
    percentiles.sort_unstable();

    let mut scratch = Vec::new();
    let dest_splits: Vec<Option<Split>> = destinations
        .iter()
        .map(|&(lat, lon)| find_split(net, lat, lon, Mode::Walk, &mut scratch))
        .collect();
    let egress = walk::egress_tables(layer, &data.trees, net, destinations, &dest_splits);

    let n_threads = if p.n_threads > 0 {
        p.n_threads
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get())
    }
    .clamp(1, origins.len().max(1));
    let next = AtomicUsize::new(0);
    let rows: Vec<Vec<(usize, Row)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..n_threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    let mut scratch = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= origins.len() {
                            break;
                        }
                        let (lat, lon) = origins[i];
                        let nd = destinations.len();
                        let mut row = vec![vec![None; percentiles.len()]; nd];
                        let Some(origin) = find_split(net, lat, lon, Mode::Walk, &mut scratch)
                        else {
                            done.push((i, row));
                            continue;
                        };
                        // Walk search: direct times and access to stops.
                        let (vertex_time, direct) = street_vertex_times(
                            net,
                            &origin,
                            &dest_splits,
                            Mode::Walk,
                            p.walk_speed_kmh,
                            street_limit,
                        );
                        let access: Vec<(u32, i32)> = layer
                            .stop_vertex
                            .iter()
                            .enumerate()
                            .filter(|(_, &v)| v >= 0 && vertex_time[v as usize] < street_limit)
                            .map(|(s, &v)| (s as u32, vertex_time[v as usize]))
                            .collect();
                        let times = if access.is_empty() {
                            None
                        } else {
                            let req = RaptorRequest {
                                from_time: p.departure_time,
                                time_window: p.time_window,
                                max_rides: p.max_rides,
                                max_duration: max_t,
                                walk_mms,
                                max_walk_min: p.max_walk_time,
                                monte_carlo_draws: p.time_window * p.draws_per_minute,
                                seed: p.seed,
                                origin: (lat, lon),
                            };
                            Some(raptor::route(
                                layer,
                                &data.transfers,
                                &filtered,
                                &access,
                                &req,
                            ))
                        };
                        for (j, cell) in row.iter_mut().enumerate() {
                            let d = direct[j];
                            let mut tt: Vec<i32> = match &times {
                                None => vec![d],
                                Some(t) => vec![d; t.len()],
                            };
                            if let Some(times) = &times {
                                for &(stop, mm) in &egress[j] {
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
                                for (c, &v) in cell.iter_mut().zip(&vals) {
                                    if v <= m {
                                        *c = Some(v as u32);
                                    }
                                }
                            }
                        }
                        done.push((i, row));
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
    let mut out = vec![Vec::new(); origins.len()];
    for (i, row) in rows.into_iter().flatten() {
        out[i] = row;
    }
    Ok(out)
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
    fn percentile_indices_match_r5() {
        assert_eq!(percentile_index(50, 10), 4);
        assert_eq!(percentile_index(50, 50), 24);
        assert_eq!(percentile_index(5, 10), 0);
        assert_eq!(percentile_index(95, 10), 9);
    }
}
