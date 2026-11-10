//! WebAssembly bindings of the peabiru routing core, for the npm package.
//!
//! As in the Python package, everything that mirrors r5r's R wrapper
//! (argument checks, defaults, dates, trip limits, column assembly) lives
//! in the JavaScript code of the package (`js/lib/index.js`); these
//! functions take already validated values, run the core and hand back
//! plain columns as typed arrays (one per column, never one object per
//! row: a matrix can have millions of rows). Row indices are 0-based.
//!
//! Everything runs on the calling thread: the core runs its workers one
//! after another on WebAssembly. A later version can run several
//! instances in Web Workers, each building its own network and routing a
//! share of the origins (results never depend on how origins are shared
//! out). The core's cancellation token is not wired: a computation on the
//! calling thread cannot be interrupted from JavaScript, and a caller
//! that needs to stop one runs it in a Worker and terminates the Worker.

use js_sys::{Array, Float64Array, Int32Array, Uint8Array};
use peabiru_core::{
    arrival_travel_time_matrix, build_from_bytes, expanded_travel_time_matrix,
    street_accessibility, street_travel_time_matrix, transit_accessibility,
    transit_travel_time_matrix, validate_date, AccessibilityParams, BuildOptions, DecayFunction,
    Destinations, ExpandedRow, Mode, RouteError, RouteParams, StreetNetwork, TransitData,
    TransitLayer, TransitParams,
};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

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
        let (net, layer) = build_from_bytes(pbf, &feeds, &BuildOptions::default()).map_err(err)?;
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
) -> TransitParams {
    let mut p = TransitParams::default();
    p.date = date;
    p.departure_time = departure_time;
    p.time_window = time_window;
    p.percentiles = percentiles;
    p.max_walk_time = max_walk_time;
    p.max_trip_duration = max_trip_duration;
    p.walk_speed_kmh = walk_speed;
    p.max_rides = max_rides;
    p.draws_per_minute = draws_per_minute;
    p.seed = Some(seed);
    p
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

/// Walk + transit travel-time matrix: the reported pairs as `from`, `to`
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
        date,
        departure_time,
        time_window,
        percentiles,
        max_walk_time,
        max_trip_duration,
        walk_speed,
        max_rides,
        draws_per_minute,
        seed,
    );
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
/// Float64Array. `mode` is `TRANSIT` (walk access and egress) or a street
/// mode; `opportunities` holds the `n_opportunities` opportunity columns
/// one after the other, each with one value per destination.
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
    decay_function: &str,
    decay_value: f64,
    cutoffs: Vec<i32>,
    percentiles: Vec<i32>,
    date: u32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
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
    let values = if mode == "TRANSIT" {
        let t = transit(network)?;
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
            seed,
        );
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
        date,
        departure_time,
        time_window,
        vec![50],
        max_walk_time,
        max_trip_duration,
        walk_speed,
        max_rides,
        draws_per_minute,
        seed,
    );
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
