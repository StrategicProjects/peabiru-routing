//! Fare structures and `paretoFrontier()`. A structure crosses the
//! boundary as a flat object of arrays (see `js/lib/index.js`), which the
//! JavaScript side turns into the package's object.

use super::{err, f64_array, i32_array, points, route_err, transit, transit_params, Network};
use js_sys::{Array, Object, Reflect, Uint8Array};
use peabiru_core::{
    pareto_frontier, read_fare_structure, setup_fare_structure, write_fare_structure, FarePerRoute,
    FarePerTransfer, FarePerType, FareStructure,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

fn get(o: &JsValue, key: &str) -> Result<JsValue, JsError> {
    let v = Reflect::get(o, &JsValue::from_str(key))
        .map_err(|_| err(format!("fare structure: cannot read {key}")))?;
    if v.is_undefined() {
        return Err(err(format!("fare structure: {key} is missing")));
    }
    Ok(v)
}

fn num(o: &JsValue, key: &str) -> Result<f64, JsError> {
    let v = get(o, key)?;
    if v.is_null() {
        return Ok(f64::NAN);
    }
    v.as_f64()
        .ok_or_else(|| err(format!("fare structure: {key} must be a number")))
}

fn text(o: &JsValue, key: &str) -> Result<String, JsError> {
    get(o, key)?
        .as_string()
        .ok_or_else(|| err(format!("fare structure: {key} must be a string")))
}

fn array(o: &JsValue, key: &str) -> Result<Array, JsError> {
    let v = get(o, key)?;
    if !Array::is_array(&v) {
        return Err(err(format!("fare structure: {key} must be an array")));
    }
    Ok(Array::from(&v))
}

fn texts(o: &JsValue, key: &str) -> Result<Vec<Option<String>>, JsError> {
    array(o, key)?
        .iter()
        .map(|v| {
            if v.is_null() || v.is_undefined() {
                Ok(None)
            } else {
                v.as_string()
                    .map(Some)
                    .ok_or_else(|| err(format!("fare structure: {key} must hold strings")))
            }
        })
        .collect()
}

fn numbers(o: &JsValue, key: &str) -> Result<Vec<f64>, JsError> {
    array(o, key)?
        .iter()
        .map(|v| {
            if v.is_null() {
                Ok(f64::NAN)
            } else {
                v.as_f64()
                    .ok_or_else(|| err(format!("fare structure: {key} must hold numbers")))
            }
        })
        .collect()
}

fn bools(o: &JsValue, key: &str) -> Result<Vec<bool>, JsError> {
    array(o, key)?
        .iter()
        .map(|v| {
            v.as_bool()
                .ok_or_else(|| err(format!("fare structure: {key} must hold true or false")))
        })
        .collect()
}

/// The core's structure of the flat object.
pub(crate) fn from_js(o: &JsValue) -> Result<FareStructure, JsError> {
    let mut s = FareStructure::default();
    s.max_discounted_transfers = num(o, "max_discounted_transfers")?;
    s.transfer_time_allowance = num(o, "transfer_time_allowance")?;
    s.fare_cap = num(o, "fare_cap")?;
    let (t, u, a, r, f) = (
        texts(o, "type")?,
        bools(o, "unlimited_transfers")?,
        bools(o, "allow_same_route_transfer")?,
        bools(o, "use_route_fare")?,
        numbers(o, "fare")?,
    );
    if [u.len(), a.len(), r.len(), f.len()]
        .iter()
        .any(|&l| l != t.len())
    {
        return Err(err("the columns of fares_per_type differ in length"));
    }
    for k in 0..t.len() {
        let mut x = FarePerType::default();
        x.fare_type = t[k].clone();
        x.unlimited_transfers = u[k];
        x.allow_same_route_transfer = a[k];
        x.use_route_fare = r[k];
        x.fare = f[k];
        s.fares_per_type.push(x);
    }
    let (a, b, f) = (
        texts(o, "first_leg")?,
        texts(o, "second_leg")?,
        numbers(o, "transfer_fare")?,
    );
    if b.len() != a.len() || f.len() != a.len() {
        return Err(err("the columns of fares_per_transfer differ in length"));
    }
    for k in 0..a.len() {
        let mut x = FarePerTransfer::default();
        x.first_leg = a[k].clone();
        x.second_leg = b[k].clone();
        x.fare = f[k];
        s.fares_per_transfer.push(x);
    }
    let cols = [
        texts(o, "agency_id")?,
        texts(o, "agency_name")?,
        texts(o, "route_id")?,
        texts(o, "route_short_name")?,
        texts(o, "route_long_name")?,
        texts(o, "mode")?,
        texts(o, "fare_type")?,
    ];
    let rf = numbers(o, "route_fare")?;
    if cols.iter().any(|c| c.len() != rf.len()) {
        return Err(err("the columns of fares_per_route differ in length"));
    }
    for k in 0..rf.len() {
        let mut x = FarePerRoute::default();
        x.agency_id = cols[0][k].clone();
        x.agency_name = cols[1][k].clone();
        x.route_id = cols[2][k].clone();
        x.route_short_name = cols[3][k].clone();
        x.route_long_name = cols[4][k].clone();
        x.mode = cols[5][k].clone();
        x.route_fare = rf[k];
        x.fare_type = cols[6][k].clone();
        s.fares_per_route.push(x);
    }
    s.debug_output_file = text(o, "output_file")?;
    s.debug_trip_info = text(o, "trip_info")?;
    Ok(s)
}

fn set(o: &Object, key: &str, v: JsValue) {
    let _ = Reflect::set(o, &JsValue::from_str(key), &v);
}

fn js_texts(v: impl Iterator<Item = Option<String>>) -> JsValue {
    v.map(|s| s.map_or(JsValue::NULL, |s| JsValue::from_str(&s)))
        .collect::<Array>()
        .into()
}

fn js_numbers(v: impl Iterator<Item = f64>) -> JsValue {
    v.map(JsValue::from_f64).collect::<Array>().into()
}

fn js_bools(v: impl Iterator<Item = bool>) -> JsValue {
    v.map(JsValue::from_bool).collect::<Array>().into()
}

/// The flat object of the core's structure.
fn to_js(s: &FareStructure) -> JsValue {
    let o = Object::new();
    let count = |x: f64| {
        if x.is_nan() {
            JsValue::NULL
        } else {
            JsValue::from_f64(x)
        }
    };
    set(
        &o,
        "max_discounted_transfers",
        count(s.max_discounted_transfers),
    );
    set(
        &o,
        "transfer_time_allowance",
        count(s.transfer_time_allowance),
    );
    set(&o, "fare_cap", JsValue::from_f64(s.fare_cap));
    let t = &s.fares_per_type;
    set(&o, "type", js_texts(t.iter().map(|x| x.fare_type.clone())));
    set(
        &o,
        "unlimited_transfers",
        js_bools(t.iter().map(|x| x.unlimited_transfers)),
    );
    set(
        &o,
        "allow_same_route_transfer",
        js_bools(t.iter().map(|x| x.allow_same_route_transfer)),
    );
    set(
        &o,
        "use_route_fare",
        js_bools(t.iter().map(|x| x.use_route_fare)),
    );
    set(&o, "fare", js_numbers(t.iter().map(|x| x.fare)));
    let tr = &s.fares_per_transfer;
    set(
        &o,
        "first_leg",
        js_texts(tr.iter().map(|x| x.first_leg.clone())),
    );
    set(
        &o,
        "second_leg",
        js_texts(tr.iter().map(|x| x.second_leg.clone())),
    );
    set(&o, "transfer_fare", js_numbers(tr.iter().map(|x| x.fare)));
    let r = &s.fares_per_route;
    set(
        &o,
        "agency_id",
        js_texts(r.iter().map(|x| x.agency_id.clone())),
    );
    set(
        &o,
        "agency_name",
        js_texts(r.iter().map(|x| x.agency_name.clone())),
    );
    set(
        &o,
        "route_id",
        js_texts(r.iter().map(|x| x.route_id.clone())),
    );
    set(
        &o,
        "route_short_name",
        js_texts(r.iter().map(|x| x.route_short_name.clone())),
    );
    set(
        &o,
        "route_long_name",
        js_texts(r.iter().map(|x| x.route_long_name.clone())),
    );
    set(&o, "mode", js_texts(r.iter().map(|x| x.mode.clone())));
    set(&o, "route_fare", js_numbers(r.iter().map(|x| x.route_fare)));
    set(
        &o,
        "fare_type",
        js_texts(r.iter().map(|x| x.fare_type.clone())),
    );
    set(&o, "output_file", JsValue::from_str(&s.debug_output_file));
    set(&o, "trip_info", JsValue::from_str(&s.debug_trip_info));
    o.into()
}

/// A routing call's fare structure: `null` or `undefined` for none.
pub(crate) fn fare_arg(fare: &JsValue) -> Result<Option<Arc<FareStructure>>, JsError> {
    if fare.is_null() || fare.is_undefined() {
        return Ok(None);
    }
    Ok(Some(Arc::new(from_js(fare)?)))
}

/// `setupFareStructure()`, as the flat object.
#[wasm_bindgen]
pub fn fare_setup(network: &Network, base_fare: f64, by: &str) -> Result<JsValue, JsError> {
    let s = setup_fare_structure(&network.net, base_fare, by).map_err(route_err)?;
    Ok(to_js(&s))
}

/// `readFareStructure()` of the bytes of a `.zip` file.
#[wasm_bindgen]
pub fn fare_read(bytes: &[u8], encoding: &str) -> Result<JsValue, JsError> {
    let s = read_fare_structure(bytes, encoding).map_err(route_err)?;
    Ok(to_js(&s))
}

/// `writeFareStructure()`: the bytes of the `.zip` file.
#[wasm_bindgen]
pub fn fare_write(fare: JsValue) -> Result<Uint8Array, JsError> {
    let s = from_js(&fare)?;
    let bytes = write_fare_structure(&s).map_err(route_err)?;
    Ok(Uint8Array::from(bytes.as_slice()))
}

/// The checks of a structure given for a network.
#[wasm_bindgen]
pub fn fare_check(network: &Network, fare: JsValue) -> Result<(), JsError> {
    let s = from_js(&fare)?;
    s.validate().map_err(route_err)?;
    s.validate_for(&network.net).map_err(route_err)
}

/// `paretoFrontier()` as `[from, to, percentile, travel_time]`
/// (Int32Array, 0-based indices) and `monetary_cost` (Float64Array);
/// `transit_modes` empty routes by `mode` alone.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn pareto(
    network: &Network,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    mode: &str,
    transit_modes: &str,
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
    seed: Option<i32>,
    fare: JsValue,
    fare_cutoffs: Vec<f64>,
) -> Result<Array, JsError> {
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    let with_transit = !transit_modes.is_empty();
    let mut p = transit_params(
        if with_transit {
            transit_modes
        } else {
            "TRANSIT"
        },
        mode,
        if with_transit { egress_mode } else { mode },
        if with_transit { date } else { 20190513 },
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
        1,
        seed,
    )?;
    p.fare_structure = Some(Arc::new(from_js(&fare)?));
    if with_transit {
        transit(network)?;
    } else {
        // by `mode` alone
        p.transit_modes.clear();
    }
    let f = pareto_frontier(&network.net, &o, &d, &p, &fare_cutoffs).map_err(route_err)?;
    let r = &f.rows;
    let out = Array::new();
    out.push(&i32_array(
        &r.iter().map(|x| x.origin as i32).collect::<Vec<_>>(),
    ));
    out.push(&i32_array(
        &r.iter().map(|x| x.destination as i32).collect::<Vec<_>>(),
    ));
    out.push(&i32_array(
        &r.iter().map(|x| x.percentile).collect::<Vec<_>>(),
    ));
    out.push(&i32_array(
        &r.iter().map(|x| x.travel_time).collect::<Vec<_>>(),
    ));
    out.push(&f64_array(
        &r.iter().map(|x| x.monetary_cost).collect::<Vec<_>>(),
    ));
    Ok(out)
}
