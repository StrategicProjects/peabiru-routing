//! WebAssembly bindings of the peabiru routing core, for the npm package.
//!
//! As in the Python package, everything that mirrors r5r's R wrapper
//! (argument checks, defaults, dates, trip limits, column assembly) lives
//! in the JavaScript code of the package (`js/lib/index.js`); these
//! functions take already validated values, run the core and hand back
//! plain columns as typed arrays (one per column, never one object per
//! row: a matrix can have millions of rows). Row indices are 0-based.
//!
//! Each instance runs on one thread: the core runs its workers one after
//! another on WebAssembly. Parallelism is in the JavaScript code: a pool
//! of workers (`js/lib/worker.js`), each with its own instance of this
//! module and its own network built from the same bytes, routes
//! contiguous shares of the origins, and the calling side concatenates
//! the columns. That gives the results of one instance because the core
//! routes every origin on its own (a transit origin's Monte Carlo
//! generator is seeded from the seed and the origin's coordinates, not
//! from its position); the `from` indices handed back count from the
//! first origin of the share. The core's cancellation token is not wired:
//! a computation cannot be interrupted from JavaScript, so an aborted
//! call terminates its workers and the pool starts new ones.

use js_sys::{Array, Float64Array, Int32Array, Uint8Array};
use peabiru_core::{
    arrival_travel_time_matrix, build_from_bytes, expanded_travel_time_matrix,
    isochrone_lines as isochrone_lines_core, isochrone_polygons as isochrone_polygons_core,
    street_accessibility, street_edges as street_edges_core, street_travel_time_matrix,
    street_travel_time_surface, transit_accessibility, transit_travel_time_matrix,
    transit_travel_time_surface, validate_date, AccessibilityParams, BuildOptions, DecayFunction,
    Destinations, ExpandedRow, Mode, RouteError, RouteParams, StreetNetwork, TransitData,
    TransitLayer, TransitMode, TransitParams,
};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

mod inspect;

/// Missing value in the `i32` columns handed to JavaScript.
const NA_I32: i32 = i32::MIN;

/// A network built once and shared by every routing call.
#[wasm_bindgen]
pub struct Network {
    net: StreetNetwork,
    /// Present when at least one GTFS feed was given.
    transit: Option<Transit>,
}

struct Transit {
    layer: TransitLayer,
    data: TransitData,
}

fn err(msg: impl std::fmt::Display) -> JsError {
    JsError::new(&msg.to_string())
}

/// The error of a failed build; for feeds with errors of priority `HIGH`,
/// r5r's message, pointing to `lister` to see them.
fn build_error(e: &peabiru_core::NetworkError, lister: &str) -> String {
    match e.high_priority_gtfs_errors() {
        None => e.to_string(),
        Some(feeds) => {
            let list: Vec<String> = feeds
                .iter()
                .map(|(p, n)| {
                    let name = p.file_name().map_or_else(
                        || p.display().to_string(),
                        |f| f.to_string_lossy().into_owned(),
                    );
                    format!("{name}: {n} error{}", if *n == 1 { "" } else { "s" })
                })
                .collect();
            format!(
                "High priority GTFS errors found; network build failed ({}). Use {lister} to see them.",
                list.join(", ")
            )
        }
    }
}

fn route_err(e: RouteError) -> JsError {
    match e {
        RouteError::Invalid(msg) => JsError::new(&msg),
        e => err(e),
    }
}

fn transit(n: &Network) -> Result<&Transit, JsError> {
    n.transit
        .as_ref()
        .ok_or_else(|| err("the network has no public transport (no GTFS feed)"))
}

/// Points as `(lat, lon)` from two coordinate columns.
fn points(lat: &[f64], lon: &[f64]) -> Result<Vec<(f64, f64)>, JsError> {
    if lat.len() != lon.len() {
        return Err(err("lat and lon must have the same length"));
    }
    Ok(lat.iter().copied().zip(lon.iter().copied()).collect())
}

fn i32_array(v: &[i32]) -> JsValue {
    Int32Array::from(v).into()
}

fn f64_array(v: &[f64]) -> JsValue {
    Float64Array::from(v).into()
}

fn strings(v: &[String]) -> JsValue {
    v.iter()
        .map(|s| JsValue::from_str(s))
        .collect::<Array>()
        .into()
}

#[wasm_bindgen]
impl Network {
    /// Build a network from the bytes of a `.pbf` file and of GTFS feeds:
    /// `feeds` is an array of `[name, Uint8Array]` pairs, loaded in that
    /// order.
    pub fn build(pbf: &[u8], feeds: Array) -> Result<Network, JsError> {
        let mut owned: Vec<(String, Vec<u8>)> = Vec::with_capacity(feeds.length() as usize);
        for f in feeds.iter() {
            let pair = Array::from(&f);
            let name = pair
                .get(0)
                .as_string()
                .ok_or_else(|| err("a feed name must be a string"))?;
            let bytes = pair
                .get(1)
                .dyn_into::<Uint8Array>()
                .map_err(|_| err("a feed's data must be a Uint8Array"))?
                .to_vec();
            owned.push((name, bytes));
        }
        let feeds: Vec<(&str, &[u8])> = owned
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        let (net, layer) = build_from_bytes(pbf, &feeds, &BuildOptions::default())
            .map_err(|e| err(build_error(&e, "getGtfsErrors() on the same feeds")))?;
        let transit = if layer.feed_ids().is_empty() {
            None
        } else {
            let data = TransitData::new(&net, &layer);
            Some(Transit { layer, data })
        };
        Ok(Network { net, transit })
    }

    /// Vertices of the street network.
    pub fn n_vertices(&self) -> usize {
        self.net.n_vertices()
    }

    /// Directed edges of the street network.
    pub fn n_edges(&self) -> usize {
        self.net.n_edges()
    }

    /// The feed ids, in loading order.
    pub fn feed_ids(&self) -> Array {
        let ids = self
            .transit
            .as_ref()
            .map_or(&[][..], |t| t.layer.feed_ids());
        ids.iter().map(|s| JsValue::from_str(s)).collect()
    }

    /// `[stops, patterns, services, frequencies (0/1)]`.
    pub fn transit_sizes(&self) -> Vec<u32> {
        match &self.transit {
            Some(t) => vec![
                t.layer.n_stops() as u32,
                t.layer.n_patterns() as u32,
                t.layer.n_services() as u32,
                u32::from(t.layer.has_frequencies()),
            ],
            None => vec![0, 0, 0, 0],
        }
    }

    /// Warnings for the feeds that r5r refuses to build a network from,
    /// one per feed.
    pub fn r5r_warnings(&self) -> Array {
        let w = self
            .transit
            .as_ref()
            .map_or_else(Vec::new, |t| t.layer.r5r_warnings());
        w.iter().map(|s| JsValue::from_str(s)).collect()
    }

    /// `[services, services active on date]` (`date` as `yyyymmdd`), as
    /// r5r's `getTransitServiceAvailability`.
    pub fn transit_availability(&self, date: u32) -> Result<Vec<u32>, JsError> {
        validate_date(date).map_err(route_err)?;
        let Some(t) = &self.transit else {
            return Ok(vec![0, 0]);
        };
        let active = t.layer.n_services_active_on(date).map_err(route_err)?;
        Ok(vec![t.layer.n_services() as u32, active as u32])
    }
}

/// Version of the Rust core this module was built from.
#[wasm_bindgen]
pub fn core_version() -> String {
    peabiru_core::VERSION.to_string()
}

/// The missing value of the `i32` columns.
#[wasm_bindgen]
pub fn na_i32() -> i32 {
    NA_I32
}

fn street_mode(mode: &str) -> Result<Mode, JsError> {
    match mode {
        "WALK" => Ok(Mode::Walk),
        "BICYCLE" => Ok(Mode::Bicycle),
        "CAR" => Ok(Mode::Car),
        _ => Err(err(format!("unsupported street mode '{mode}'"))),
    }
}

/// The transit modes of r5r's `transit_mode` string (`"BUS;RAIL"`, or
/// every mode and `TRANSIT` for `"TRANSIT"`); `TRANSIT` itself selects no
/// route.
fn parse_transit_modes(modes: &str) -> Result<Vec<TransitMode>, JsError> {
    modes
        .split(';')
        .filter(|m| !m.is_empty() && *m != "TRANSIT")
        .map(|m| {
            TransitMode::from_name(m).ok_or_else(|| err(format!("unknown transit mode '{m}'")))
        })
        .collect()
}

fn route_params(walk_speed: f64, bike_speed: f64, max_lts: i32, max_trip: i32) -> RouteParams {
    let mut p = RouteParams::default();
    p.walk_speed_kmh = walk_speed;
    p.bike_speed_kmh = bike_speed;
    p.max_lts = max_lts;
    p.max_trip_duration_min = max_trip;
    p
}

#[allow(clippy::too_many_arguments)]
fn transit_params(
    modes: &str,
    access_mode: &str,
    egress_mode: &str,
    date: u32,
    departure_time: i32,
    time_window: i32,
    percentiles: Vec<i32>,
    max_walk_time: i32,
    max_bike_time: i32,
    max_car_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_rides: usize,
    draws_per_minute: i32,
    seed: i32,
) -> Result<TransitParams, JsError> {
    let mut p = TransitParams::default();
    p.transit_modes = parse_transit_modes(modes)?;
    p.access_mode = street_mode(access_mode)?;
    p.egress_mode = street_mode(egress_mode)?;
    p.date = date;
    p.departure_time = departure_time;
    p.time_window = time_window;
    p.percentiles = percentiles;
    p.max_walk_time = max_walk_time;
    p.max_bike_time = max_bike_time;
    p.max_car_time = max_car_time;
    p.max_trip_duration = max_trip_duration;
    p.walk_speed_kmh = walk_speed;
    p.bike_speed_kmh = bike_speed;
    p.max_lts = max_lts;
    p.max_rides = max_rides;
    p.draws_per_minute = draws_per_minute;
    p.seed = Some(seed);
    Ok(p)
}

/// Street-only travel-time matrix: the reported pairs as
/// `[from, to, minutes]` Int32Arrays, origin-major.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn street_ttm(
    network: &Network,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    mode: &str,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_trip_duration: i32,
) -> Result<Array, JsError> {
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    let mode = street_mode(mode)?;
    let p = route_params(walk_speed, bike_speed, max_lts, max_trip_duration);
    let m = street_travel_time_matrix(&network.net, &o, &d, mode, &p).map_err(route_err)?;
    let nd = d.len();
    let (mut from, mut to, mut tt) = (Vec::new(), Vec::new(), Vec::new());
    for (k, v) in m.as_slice().iter().enumerate() {
        if let Some(v) = v {
            from.push((k / nd) as i32);
            to.push((k % nd) as i32);
            tt.push(*v as i32);
        }
    }
    Ok([i32_array(&from), i32_array(&to), i32_array(&tt)]
        .into_iter()
        .collect())
}

/// Public transport travel-time matrix: the reported pairs as `from`, `to`
/// and one Int32Array per percentile (ascending), with `NA_I32` where r5r
/// gives NA; origin-major.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn transit_ttm(
    network: &Network,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    transit_modes: &str,
    access_mode: &str,
    egress_mode: &str,
    bike_speed: f64,
    max_lts: i32,
    max_bike_time: i32,
    max_car_time: i32,
    date: u32,
    departure_time: i32,
    time_window: i32,
    percentiles: Vec<i32>,
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    max_rides: usize,
    draws_per_minute: i32,
    seed: i32,
) -> Result<Array, JsError> {
    let t = transit(network)?;
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    let np = percentiles.len();
    let p = transit_params(
        transit_modes,
        access_mode,
        egress_mode,
        date,
        departure_time,
        time_window,
        percentiles,
        max_walk_time,
        max_bike_time,
        max_car_time,
        max_trip_duration,
        walk_speed,
        bike_speed,
        max_lts,
        max_rides,
        draws_per_minute,
        seed,
    )?;
    let m = transit_travel_time_matrix(&network.net, &t.layer, &t.data, &o, &d, &p)
        .map_err(route_err)?;
    let (mut from, mut to) = (Vec::new(), Vec::new());
    let mut cols: Vec<Vec<i32>> = vec![Vec::new(); np];
    for (i, row) in m.rows().enumerate() {
        for (j, cell) in row.chunks_exact(np).enumerate() {
            if cell[0].is_none() {
                continue;
            }
            from.push(i as i32);
            to.push(j as i32);
            for (c, v) in cols.iter_mut().zip(cell) {
                c.push(v.map_or(NA_I32, |v| v as i32));
            }
        }
    }
    let out = Array::new();
    out.push(&i32_array(&from));
    out.push(&i32_array(&to));
    for c in &cols {
        out.push(&i32_array(c));
    }
    Ok(out)
}

fn decay(name: &str, value: f64) -> Result<DecayFunction, JsError> {
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

/// Accessibility values `[origin][opportunity][percentile][cutoff]` in one
/// Float64Array. `mode` is the street mode (of the access to public
/// transport when `transit_modes` is not empty); `opportunities` holds the
/// `n_opportunities` opportunity columns one after the other, each with
/// one value per destination.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn accessibility(
    network: &Network,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    opportunities: &[f64],
    n_opportunities: usize,
    mode: &str,
    transit_modes: &str,
    egress_mode: &str,
    decay_function: &str,
    decay_value: f64,
    cutoffs: Vec<i32>,
    percentiles: Vec<i32>,
    date: u32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
    max_bike_time: i32,
    max_car_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_rides: usize,
    draws_per_minute: i32,
    seed: i32,
) -> Result<Float64Array, JsError> {
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    if opportunities.len() != n_opportunities * d.len() {
        return Err(err(
            "every opportunity column must have one value per destination",
        ));
    }
    let opp: Vec<Vec<f64>> = (0..n_opportunities)
        .map(|k| opportunities[k * d.len()..(k + 1) * d.len()].to_vec())
        .collect();
    let mut a = AccessibilityParams::default();
    a.cutoffs = cutoffs;
    a.decay = decay(decay_function, decay_value)?;
    let dest = Destinations::new(&d, &opp);
    let values = if !transit_modes.is_empty() {
        let t = transit(network)?;
        let p = transit_params(
            transit_modes,
            mode,
            egress_mode,
            date,
            departure_time,
            time_window,
            percentiles,
            max_walk_time,
            max_bike_time,
            max_car_time,
            max_trip_duration,
            walk_speed,
            bike_speed,
            max_lts,
            max_rides,
            draws_per_minute,
            seed,
        )?;
        transit_accessibility(&network.net, &t.layer, &t.data, &o, &dest, &p, &a)
    } else {
        let m = street_mode(mode)?;
        let p = route_params(walk_speed, bike_speed, max_lts, max_trip_duration);
        street_accessibility(&network.net, &o, &dest, m, &p, &percentiles, &a)
    }
    .map_err(route_err)?;
    Ok(Float64Array::from(values.values()))
}

/// Expanded (or, with `arrival`, arrival) travel-time matrix as an array
/// of columns: `from`, `to` (0-based Int32Array), `departure` (Int32Array,
/// seconds after midnight), `draw` (Int32Array), the times `access_time`,
/// `wait_time`, `ride_time`, `transfer_time`, `egress_time`, `total_time`
/// (Float64Array, NaN where r5r gives NA), `n_rides` (Int32Array, `NA_I32`
/// for NA), and `routes` dictionary-encoded as `route_codes` (Int32Array,
/// -1 for NA) into the array of strings `route_levels`. For an arrival
/// search the parameters must already be the departure side (date and
/// time of `arrival - max_trip_duration`, `time_window =
/// max_trip_duration`), as r5r's R wrapper sets them.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn expanded(
    network: &Network,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    transit_modes: &str,
    access_mode: &str,
    egress_mode: &str,
    bike_speed: f64,
    max_lts: i32,
    max_bike_time: i32,
    max_car_time: i32,
    arrival: bool,
    date: u32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    max_rides: usize,
    draws_per_minute: i32,
    seed: i32,
) -> Result<Array, JsError> {
    let t = transit(network)?;
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    let p = transit_params(
        transit_modes,
        access_mode,
        egress_mode,
        date,
        departure_time,
        time_window,
        vec![50],
        max_walk_time,
        max_bike_time,
        max_car_time,
        max_trip_duration,
        walk_speed,
        bike_speed,
        max_lts,
        max_rides,
        draws_per_minute,
        seed,
    )?;
    let rows = if arrival {
        arrival_travel_time_matrix(&network.net, &t.layer, &t.data, &o, &d, &p)
    } else {
        expanded_travel_time_matrix(&network.net, &t.layer, &t.data, &o, &d, &p)
    }
    .map_err(route_err)?;
    let c = ExpandedColumns::new(rows.into_vec());
    let out = Array::new();
    out.push(&i32_array(&c.from));
    out.push(&i32_array(&c.to));
    out.push(&i32_array(&c.departure));
    out.push(&i32_array(&c.draw));
    for col in &c.times {
        out.push(&f64_array(col));
    }
    out.push(&i32_array(&c.n_rides));
    out.push(&i32_array(&c.route_codes));
    out.push(&strings(&c.route_levels));
    Ok(out)
}

/// Polygon isochrones as an array of columns the JavaScript side assembles
/// into GeoJSON: per row (origin, percentile, cutoff; r5r's order)
/// `origin` (0-based Int32Array), `isochrone` (Float64Array), `percentile`
/// and `n_polygons` (Int32Array); per polygon `n_rings`; per ring
/// `n_points` (Int32Array); per point `x` (longitude) and `y` (latitude),
/// Float64Array. `mode` is the street mode (of the access to public
/// transport when `transit_modes` is not empty); `max_trip_duration` is
/// the search limit (r5r: the largest cutoff plus 10).
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn isochrone_polygons(
    network: &Network,
    olat: &[f64],
    olon: &[f64],
    mode: &str,
    transit_modes: &str,
    egress_mode: &str,
    cutoffs: Vec<f64>,
    zoom: i32,
    percentiles: Vec<i32>,
    date: u32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
    max_bike_time: i32,
    max_car_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_rides: usize,
    draws_per_minute: i32,
    seed: i32,
) -> Result<Array, JsError> {
    let o = points(olat, olon)?;
    let surface = if !transit_modes.is_empty() {
        let t = transit(network)?;
        let p = transit_params(
            transit_modes,
            mode,
            egress_mode,
            date,
            departure_time,
            time_window,
            percentiles,
            max_walk_time,
            max_bike_time,
            max_car_time,
            max_trip_duration,
            walk_speed,
            bike_speed,
            max_lts,
            max_rides,
            draws_per_minute,
            seed,
        )?;
        transit_travel_time_surface(&network.net, &t.layer, &t.data, &o, &p, zoom)
    } else {
        let m = street_mode(mode)?;
        let p = route_params(walk_speed, bike_speed, max_lts, max_trip_duration);
        street_travel_time_surface(&network.net, &o, m, &p, &percentiles, zoom)
    }
    .map_err(route_err)?;
    let rows = isochrone_polygons_core(&surface, &cutoffs).map_err(route_err)?;
    let (mut origin, mut iso, mut pct, mut n_polygons) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut n_rings, mut n_points, mut x, mut y) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for r in &rows {
        origin.push(r.origin as i32);
        iso.push(r.isochrone);
        pct.push(r.percentile);
        n_polygons.push(r.polygons.len() as i32);
        for poly in &r.polygons {
            n_rings.push(poly.len() as i32);
            for ring in poly {
                n_points.push(ring.len() as i32);
                for &[px, py] in ring {
                    x.push(px);
                    y.push(py);
                }
            }
        }
    }
    Ok([
        i32_array(&origin),
        f64_array(&iso),
        i32_array(&pct),
        i32_array(&n_polygons),
        i32_array(&n_rings),
        i32_array(&n_points),
        f64_array(&x),
        f64_array(&y),
    ]
    .into_iter()
    .collect())
}

/// The street edges of r5r's `street_network_to_sf()$edges` as an array of
/// columns: `edge_index`, `from_vertex`, `to_vertex` (Int32Array),
/// `osm_id` (Float64Array), `street_class` (Int32Array codes: 0 MOTORWAY,
/// 1 PRIMARY, 2 SECONDARY, 3 TERTIARY, 4 OTHER), `length` (Float64Array),
/// `walk`, `car` (Int32Array 0/1), `car_speed` (Float64Array), `bicycle`
/// (0/1), `bicycle_lts` (Int32Array), the centroid r5r routes to as `lat`,
/// `lon` (Float64Array, NaN without one), and the line of each edge as
/// `n_points` (Int32Array) and flat `x`, `y` (Float64Array).
#[wasm_bindgen]
pub fn street_edges(network: &Network) -> Array {
    let e = street_edges_core(&network.net);
    let rows = 0..e.len();
    let ints = |f: &dyn Fn(usize) -> i32| -> JsValue {
        i32_array(&rows.clone().map(f).collect::<Vec<_>>())
    };
    let floats = |f: &dyn Fn(usize) -> f64| -> JsValue {
        f64_array(&rows.clone().map(f).collect::<Vec<_>>())
    };
    let class_code = |s: &str| match s {
        "MOTORWAY" => 0,
        "PRIMARY" => 1,
        "SECONDARY" => 2,
        "TERTIARY" => 3,
        _ => 4,
    };
    let cen = e.centroid_points();
    let (mut n_points, mut x, mut y) = (Vec::with_capacity(e.len()), Vec::new(), Vec::new());
    for i in rows.clone() {
        let g = e.geometry(i);
        n_points.push(g.len() as i32);
        for &[px, py] in g {
            x.push(px);
            y.push(py);
        }
    }
    let lat: Vec<f64> = cen.iter().map(|c| c.0).collect();
    let lon: Vec<f64> = cen.iter().map(|c| c.1).collect();
    [
        ints(&|i| e.edge_index(i) as i32),
        floats(&|i| e.osm_id(i) as f64),
        ints(&|i| e.from_vertex(i) as i32),
        ints(&|i| e.to_vertex(i) as i32),
        ints(&|i| class_code(e.street_class(i))),
        floats(&|i| e.length_m(i)),
        ints(&|i| i32::from(e.walk(i))),
        ints(&|i| i32::from(e.car(i))),
        floats(&|i| e.car_speed_kph(i)),
        ints(&|i| i32::from(e.bicycle(i))),
        ints(&|i| e.bicycle_lts(i)),
        f64_array(&lat),
        f64_array(&lon),
        i32_array(&n_points),
        f64_array(&x),
        f64_array(&y),
    ]
    .into_iter()
    .collect()
}

/// Line isochrones from a matrix of median minutes (`NA_I32` where not
/// reported), `n_origins` rows of `n_edges` values, origin-major: the rows
/// in r5r's order as `[origin, edge, travel_time]` (0-based Int32Arrays)
/// and `isochrone` (Float64Array).
#[wasm_bindgen]
pub fn isochrone_lines(
    n_origins: usize,
    n_edges: usize,
    minutes: &[i32],
    cutoffs: Vec<f64>,
) -> Result<Array, JsError> {
    let m: Vec<Option<u32>> = minutes.iter().map(|&v| u32::try_from(v).ok()).collect();
    let lines = isochrone_lines_core(n_origins, n_edges, &m, &cutoffs).map_err(route_err)?;
    let origin: Vec<i32> = lines.iter().map(|l| l.origin as i32).collect();
    let edge: Vec<i32> = lines.iter().map(|l| l.edge as i32).collect();
    let tt: Vec<i32> = lines.iter().map(|l| l.travel_time as i32).collect();
    let iso: Vec<f64> = lines.iter().map(|l| l.isochrone).collect();
    Ok([
        i32_array(&origin),
        i32_array(&edge),
        i32_array(&tt),
        f64_array(&iso),
    ]
    .into_iter()
    .collect())
}

/// The rows of an expanded matrix as columns.
struct ExpandedColumns {
    from: Vec<i32>,
    to: Vec<i32>,
    departure: Vec<i32>,
    draw: Vec<i32>,
    /// access, wait, ride, transfer, egress, total.
    times: [Vec<f64>; 6],
    n_rides: Vec<i32>,
    route_codes: Vec<i32>,
    route_levels: Vec<String>,
}

impl ExpandedColumns {
    fn new(rows: Vec<Vec<ExpandedRow>>) -> Self {
        let len: usize = rows.iter().map(Vec::len).sum();
        let mut c = ExpandedColumns {
            from: Vec::with_capacity(len),
            to: Vec::with_capacity(len),
            departure: Vec::with_capacity(len),
            draw: Vec::with_capacity(len),
            times: Default::default(),
            n_rides: Vec::with_capacity(len),
            route_codes: Vec::with_capacity(len),
            route_levels: Vec::new(),
        };
        let mut levels: HashMap<String, i32> = HashMap::new();
        for (i, origin_rows) in rows.into_iter().enumerate() {
            for r in origin_rows {
                c.from.push(i as i32);
                c.to.push(r.to as i32);
                c.departure.push(r.departure);
                c.draw.push(r.draw as i32);
                let vals = r.trip.as_ref().map(|b| {
                    [
                        b.access_time,
                        b.wait_time,
                        b.ride_time,
                        b.transfer_time,
                        b.egress_time,
                        b.total_time,
                    ]
                });
                for (k, col) in c.times.iter_mut().enumerate() {
                    col.push(vals.map_or(f64::NAN, |v| v[k]));
                }
                match r.trip {
                    Some(b) => {
                        c.n_rides.push(b.n_rides as i32);
                        let next = c.route_levels.len() as i32;
                        let code = *levels.entry(b.routes).or_insert_with_key(|k| {
                            c.route_levels.push(k.clone());
                            next
                        });
                        c.route_codes.push(code);
                    }
                    None => {
                        c.n_rides.push(NA_I32);
                        c.route_codes.push(-1);
                    }
                }
            }
        }
        c
    }
}
