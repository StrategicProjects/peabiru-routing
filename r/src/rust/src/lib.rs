//! R bindings of the peabiru routing core.
//!
//! Everything that mirrors r5r's R wrapper (argument checks, defaults,
//! datetime handling, trip limits, column assembly) lives in the R code of
//! the package; these functions only take already validated values, run the
//! core and hand back plain vectors. Points are passed as separate latitude
//! and longitude vectors; row indices returned to R are 1-based.

use extendr_api::prelude::*;
use extendr_api::{Error, Result};
use peabiru_core::street::{self, BuildOptions, Mode, RouteParams, StreetNetwork};
use peabiru_core::transit::accessibility::{
    street_accessibility, transit_accessibility, AccessibilityParams, Destinations,
};
use peabiru_core::transit::decay::DecayFunction;
use peabiru_core::transit::expanded::{
    arrival_travel_time_matrix, expanded_travel_time_matrix, ExpandedRow,
};
use peabiru_core::transit::matrix::{self, TransitData, TransitParams};
use peabiru_core::transit::{self as tr, TransitLayer};

/// A network built once and shared by every routing call.
struct Network {
    net: StreetNetwork,
    /// Present when the data directory had at least one GTFS feed.
    transit: Option<Transit>,
}

struct Transit {
    layer: TransitLayer,
    data: TransitData,
}

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(msg.to_string())
}

fn network(ptr: &Robj) -> Result<&Network> {
    let p: &ExternalPtr<Network> = ptr
        .try_into()
        .map_err(|_| err("not a peabiru network (was it saved and reloaded? build it again)"))?;
    p.try_addr()
        .map_err(|_| err("this network is no longer valid; build it again"))
}

fn transit(n: &Network) -> Result<&Transit> {
    n.transit
        .as_ref()
        .ok_or_else(|| err("the network has no public transport (no GTFS feed)"))
}

fn threads(n: i32) -> usize {
    n.max(0) as usize
}

fn points(lat: &[f64], lon: &[f64]) -> Vec<(f64, f64)> {
    lat.iter().zip(lon).map(|(&a, &b)| (a, b)).collect()
}

/// Version of the Rust core this package was built against.
/// @noRd
#[extendr]
fn rs_core_version() -> &'static str {
    peabiru_core::VERSION
}

/// Build a network from a directory with one `.pbf` and any GTFS zips.
/// @noRd
#[extendr]
fn rs_build_network(data_path: &str, n_threads: i32) -> Result<Robj> {
    let opts = BuildOptions {
        n_threads: threads(n_threads),
    };
    let (net, layer) = tr::build_from_dir(data_path, &opts).map_err(err)?;
    let transit = if layer.feed_ids.is_empty() {
        None
    } else {
        let data = TransitData::new(&net, &layer);
        Some(Transit { layer, data })
    };
    let ptr = ExternalPtr::new(Network { net, transit });
    Ok(ptr.into())
}

/// Whether `ptr` points to a live network.
/// @noRd
#[extendr]
fn rs_network_valid(ptr: Robj) -> bool {
    network(&ptr).is_ok()
}

/// Sizes of the network, for `print()`.
/// @noRd
#[extendr]
fn rs_network_info(ptr: Robj) -> Result<Robj> {
    let n = network(&ptr)?;
    let (feeds, stops, patterns, services, freq) = match &n.transit {
        Some(t) => (
            t.layer.feed_ids.clone(),
            t.layer.stop_ids.len(),
            t.layer.patterns.len(),
            t.layer.services.len(),
            t.layer.has_frequencies,
        ),
        None => (Vec::new(), 0, 0, 0, false),
    };
    Ok(list!(
        vertices = n.net.n_vertices() as f64,
        edges = n.net.n_edges() as f64,
        feeds = feeds,
        stops = stops as f64,
        patterns = patterns as f64,
        services = services as f64,
        frequencies = freq
    )
    .into())
}

/// Number of services in the network and of those active on `date`
/// (`yyyymmdd`), as r5r's `getTransitServiceAvailability`.
/// @noRd
#[extendr]
fn rs_transit_availability(ptr: Robj, date: i32) -> Result<Vec<f64>> {
    let n = network(&ptr)?;
    let Some(t) = &n.transit else {
        return Ok(vec![0.0, 0.0]);
    };
    let date = date as u32;
    let wd = matrix::weekday(date);
    let active = t
        .layer
        .services
        .iter()
        .filter(|s| s.active_on(date, wd))
        .count();
    Ok(vec![t.layer.services.len() as f64, active as f64])
}

fn street_mode(mode: &str) -> Result<Mode> {
    match mode {
        "WALK" => Ok(Mode::Walk),
        "BICYCLE" => Ok(Mode::Bicycle),
        "CAR" => Ok(Mode::Car),
        _ => Err(err(format!("unsupported street mode '{mode}'"))),
    }
}

/// Street-only travel-time matrix: the reported pairs as
/// `list(from, to, minutes)`, origin-major.
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_street_ttm(
    ptr: Robj,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    mode: &str,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_trip_duration: i32,
    n_threads: i32,
) -> Result<Robj> {
    let n = network(&ptr)?;
    let (o, d) = (points(olat, olon), points(dlat, dlon));
    let params = RouteParams {
        walk_speed_kmh: walk_speed,
        bike_speed_kmh: bike_speed,
        max_lts,
        max_trip_duration_min: max_trip_duration,
        n_threads: threads(n_threads),
    };
    let m = street::travel_time_matrix(&n.net, &o, &d, street_mode(mode)?, &params).map_err(err)?;
    let nd = d.len();
    let (mut from, mut to, mut tt) = (Vec::new(), Vec::new(), Vec::new());
    for (k, v) in m.iter().enumerate() {
        if let Some(v) = v {
            from.push((k / nd) as i32 + 1);
            to.push((k % nd) as i32 + 1);
            tt.push(*v as i32);
        }
    }
    Ok(list!(from = from, to = to, time = tt).into())
}

#[allow(clippy::too_many_arguments)]
fn transit_params(
    date: i32,
    departure_time: i32,
    time_window: i32,
    percentiles: &[i32],
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    max_rides: i32,
    draws_per_minute: i32,
    seed: Option<i32>,
    n_threads: i32,
) -> TransitParams {
    TransitParams {
        date: date as u32,
        departure_time,
        time_window,
        percentiles: percentiles.to_vec(),
        max_walk_time,
        max_trip_duration,
        walk_speed_kmh: walk_speed,
        max_rides: max_rides.max(0) as usize,
        draws_per_minute,
        seed,
        n_threads: threads(n_threads),
    }
}

fn seed_of(has_seed: bool, seed: i32) -> Option<i32> {
    has_seed.then_some(seed)
}

/// Walk + transit travel-time matrix: the reported pairs as
/// `list(from, to, p1, ..., pk)` (percentiles ascending, NA where r5r
/// gives NA), origin-major.
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_transit_ttm(
    ptr: Robj,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    date: i32,
    departure_time: i32,
    time_window: i32,
    percentiles: &[i32],
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    max_rides: i32,
    draws_per_minute: i32,
    has_seed: bool,
    seed: i32,
    n_threads: i32,
) -> Result<Robj> {
    let n = network(&ptr)?;
    let t = transit(n)?;
    let (o, d) = (points(olat, olon), points(dlat, dlon));
    let p = transit_params(
        date,
        departure_time,
        time_window,
        percentiles,
        max_walk_time,
        max_trip_duration,
        walk_speed,
        max_rides,
        draws_per_minute,
        seed_of(has_seed, seed),
        n_threads,
    );
    let m = matrix::travel_time_matrix(&n.net, &t.layer, &t.data, &o, &d, &p).map_err(err)?;
    let np = percentiles.len();
    let (mut from, mut to) = (Vec::new(), Vec::new());
    let mut cols: Vec<Vec<Option<i32>>> = vec![Vec::new(); np];
    for (i, row) in m.iter().enumerate() {
        for (j, cell) in row.iter().enumerate() {
            if cell[0].is_none() {
                continue;
            }
            from.push(i as i32 + 1);
            to.push(j as i32 + 1);
            for (c, v) in cols.iter_mut().zip(cell) {
                c.push(v.map(|v| v as i32));
            }
        }
    }
    let mut out: Vec<Robj> = vec![from.into(), to.into()];
    out.extend(cols.into_iter().map(Robj::from));
    Ok(List::from_values(out).into())
}

fn decay(name: &str, value: f64) -> Result<DecayFunction> {
    Ok(match name {
        "step" => DecayFunction::Step,
        "exponential" => DecayFunction::Exponential,
        "fixed_exponential" => DecayFunction::FixedExponential {
            decay_constant: value,
        },
        "linear" => DecayFunction::Linear {
            width_minutes: value as i32,
        },
        "logistic" => DecayFunction::Logistic {
            standard_deviation_minutes: value,
        },
        _ => return Err(err(format!("unknown decay function '{name}'"))),
    })
}

/// Accessibility values `[origin][opportunity][percentile][cutoff]`,
/// flattened. `mode` is `TRANSIT` (walk access and egress) or a street
/// mode; `opportunities` is a list of double vectors, one per column.
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_accessibility(
    ptr: Robj,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    opportunities: List,
    mode: &str,
    decay_function: &str,
    decay_value: f64,
    cutoffs: &[i32],
    percentiles: &[i32],
    date: i32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_rides: i32,
    draws_per_minute: i32,
    has_seed: bool,
    seed: i32,
    n_threads: i32,
) -> Result<Vec<f64>> {
    let n = network(&ptr)?;
    let (o, d) = (points(olat, olon), points(dlat, dlon));
    let opp: Vec<Vec<f64>> = opportunities
        .values()
        .map(|v| {
            v.as_real_slice()
                .map(<[f64]>::to_vec)
                .ok_or_else(|| err("opportunities must be double vectors"))
        })
        .collect::<Result<_>>()?;
    let dest = Destinations {
        points: &d,
        opportunities: &opp,
    };
    let a = AccessibilityParams {
        cutoffs: cutoffs.to_vec(),
        decay: decay(decay_function, decay_value)?,
    };
    let acc = if mode == "TRANSIT" {
        let t = transit(n)?;
        let p = transit_params(
            date,
            departure_time,
            time_window,
            percentiles,
            max_walk_time,
            max_trip_duration,
            walk_speed,
            max_rides,
            draws_per_minute,
            seed_of(has_seed, seed),
            n_threads,
        );
        transit_accessibility(&n.net, &t.layer, &t.data, &o, &dest, &p, &a)
    } else {
        let params = RouteParams {
            walk_speed_kmh: walk_speed,
            bike_speed_kmh: bike_speed,
            max_lts,
            max_trip_duration_min: max_trip_duration,
            n_threads: threads(n_threads),
        };
        street_accessibility(
            &n.net,
            &o,
            &dest,
            street_mode(mode)?,
            &params,
            percentiles,
            &a,
        )
    }
    .map_err(err)?;
    Ok(acc.values)
}

/// Expanded (or, with `arrival`, arrival) travel-time matrix as columns:
/// `from`, `to` (1-based), `departure` (seconds after midnight), `draw`,
/// the breakdown times (NA where r5r gives NA), `routes`, `n_rides` and
/// `total_time`. For an arrival search the parameters must already be the
/// departure side (date and time of `arrival - max_trip_duration`,
/// `time_window = max_trip_duration`), as r5r's R wrapper sets them.
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_expanded(
    ptr: Robj,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    arrival: bool,
    date: i32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    max_rides: i32,
    draws_per_minute: i32,
    has_seed: bool,
    seed: i32,
    n_threads: i32,
) -> Result<Robj> {
    let n = network(&ptr)?;
    let t = transit(n)?;
    let (o, d) = (points(olat, olon), points(dlat, dlon));
    let p = transit_params(
        date,
        departure_time,
        time_window,
        &[50],
        max_walk_time,
        max_trip_duration,
        walk_speed,
        max_rides,
        draws_per_minute,
        seed_of(has_seed, seed),
        n_threads,
    );
    let rows: Vec<Vec<ExpandedRow>> = if arrival {
        arrival_travel_time_matrix(&n.net, &t.layer, &t.data, &o, &d, &p)
    } else {
        expanded_travel_time_matrix(&n.net, &t.layer, &t.data, &o, &d, &p)
    }
    .map_err(err)?;
    let len: usize = rows.iter().map(Vec::len).sum();
    let mut from = Vec::with_capacity(len);
    let mut to = Vec::with_capacity(len);
    let mut dep = Vec::with_capacity(len);
    let mut draw = Vec::with_capacity(len);
    let mut times: [Vec<Option<f64>>; 6] = Default::default();
    let mut routes: Vec<Option<String>> = Vec::with_capacity(len);
    let mut n_rides: Vec<Option<i32>> = Vec::with_capacity(len);
    for (i, origin_rows) in rows.into_iter().enumerate() {
        for r in origin_rows {
            from.push(i as i32 + 1);
            to.push(r.to as i32 + 1);
            dep.push(r.departure);
            draw.push(r.draw as i32);
            let b = r.trip.as_ref();
            let vals = b.map(|b| {
                [
                    b.access_time,
                    b.wait_time,
                    b.ride_time,
                    b.transfer_time,
                    b.egress_time,
                    b.total_time,
                ]
            });
            for (k, col) in times.iter_mut().enumerate() {
                col.push(vals.map(|v| v[k]));
            }
            n_rides.push(b.map(|b| b.n_rides as i32));
            routes.push(r.trip.map(|b| b.routes));
        }
    }
    let [access, wait, ride, transfer, egress, total] = times;
    Ok(list!(
        from = from,
        to = to,
        departure = dep,
        draw = draw,
        access_time = access,
        wait_time = wait,
        ride_time = ride,
        transfer_time = transfer,
        egress_time = egress,
        routes = routes,
        n_rides = n_rides,
        total_time = total
    )
    .into())
}

// Macro to generate exports.
// This ensures exported functions are registered with R.
// See corresponding C code in `entrypoint.c`.
extendr_module! {
    mod peabiru;
    fn rs_core_version;
    fn rs_build_network;
    fn rs_network_valid;
    fn rs_network_info;
    fn rs_transit_availability;
    fn rs_street_ttm;
    fn rs_transit_ttm;
    fn rs_accessibility;
    fn rs_expanded;
}
