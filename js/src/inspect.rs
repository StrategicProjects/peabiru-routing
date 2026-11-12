//! Bindings of r5r's network functions (`find_snap()`,
//! `street_network_bbox()`, `street_network_to_sf()`,
//! `transit_network_to_sf()`, `check_transit_availability()`,
//! `get_gtfs_errors()`): as in the rest of the module, `js/lib/index.js`
//! checks the arguments and builds the results; these hand back columns.

use super::{err, f64_array, i32_array, points, route_err, street_mode, Network};
use js_sys::{Array, Uint8Array};
use peabiru_core::{
    find_snap as find_snap_core, gtfs_errors as gtfs_errors_core,
    street_vertices as street_vertices_core, transit_routes as transit_routes_core,
    transit_service_dates as transit_service_dates_core, transit_stops as transit_stops_core,
};
use wasm_bindgen::prelude::*;

fn bools(v: &[bool]) -> JsValue {
    let b: Vec<u8> = v.iter().map(|&x| u8::from(x)).collect();
    Uint8Array::from(&b[..]).into()
}

/// Strings, `null` where missing.
fn opt_strings<S: AsRef<str>>(v: &[Option<S>]) -> JsValue {
    v.iter()
        .map(|s| {
            s.as_ref()
                .map_or(JsValue::NULL, |s| JsValue::from_str(s.as_ref()))
        })
        .collect::<Array>()
        .into()
}

fn str_array<S: AsRef<str>>(v: &[S]) -> JsValue {
    v.iter()
        .map(|s| JsValue::from_str(s.as_ref()))
        .collect::<Array>()
        .into()
}

/// Snaps of r5r's `find_snap()`: `[snap_lat, snap_lon, distance]`
/// (Float64Array, NaN where not found) and `found` (Uint8Array, 0/1).
#[wasm_bindgen]
pub fn find_snap(
    network: &Network,
    lat: &[f64],
    lon: &[f64],
    radius: f64,
    mode: &str,
) -> Result<Array, JsError> {
    let snaps = find_snap_core(&network.net, &points(lat, lon)?, radius, street_mode(mode)?)
        .map_err(route_err)?;
    let col = |f: fn(&peabiru_core::Snap) -> f64| -> JsValue {
        f64_array(
            &snaps
                .iter()
                .map(|s| s.as_ref().map_or(f64::NAN, f))
                .collect::<Vec<_>>(),
        )
    };
    let found: Vec<bool> = snaps.iter().map(Option::is_some).collect();
    Ok([
        col(|s| s.lat),
        col(|s| s.lon),
        col(|s| s.distance_m),
        bools(&found),
    ]
    .into_iter()
    .collect())
}

/// `[xmin, ymin, xmax, ymax]` of the street network (R5's envelope).
#[wasm_bindgen]
pub fn street_bbox(network: &Network) -> Vec<f64> {
    network.net.street().bbox().to_vec()
}

/// The vertices of r5r's `street_network_to_sf()`: `[index (Int32Array),
/// lat, lon (Float64Array), park_and_ride, bike_sharing (Uint8Array)]`.
#[wasm_bindgen]
pub fn street_vertices(network: &Network) -> Array {
    let v = street_vertices_core(&network.net);
    let index: Vec<i32> = v.index.iter().map(|&i| i as i32).collect();
    [
        i32_array(&index),
        f64_array(&v.lat),
        f64_array(&v.lon),
        bools(&v.park_and_ride),
        bools(&v.bike_sharing),
    ]
    .into_iter()
    .collect()
}

/// The stops of r5r's `transit_network_to_sf()`: `[stop_index
/// (Int32Array), stop_id, stop_name (arrays, null where blank), lat, lon
/// (Float64Array, NaN for stops not linked to the streets),
/// linked_to_street (Uint8Array)]`.
#[wasm_bindgen]
pub fn transit_stops(network: &Network) -> Array {
    let s = match network.net.transit() {
        Some(_) => transit_stops_core(&network.net),
        None => Default::default(),
    };
    let index: Vec<i32> = s.stop_index.iter().map(|&i| i as i32).collect();
    let nan = |v: &[Option<f64>]| -> Vec<f64> { v.iter().map(|x| x.unwrap_or(f64::NAN)).collect() };
    [
        i32_array(&index),
        str_array(&s.stop_id),
        opt_strings(&s.stop_name),
        f64_array(&nan(&s.lat)),
        f64_array(&nan(&s.lon)),
        bools(&s.linked_to_street),
    ]
    .into_iter()
    .collect()
}

/// The routes of r5r's `transit_network_to_sf()`: `[agency_id,
/// agency_name, route_id, long_name, short_name, mode]` (arrays, null where
/// blank) and the line of each row as `n_points` (Int32Array) and flat `x`,
/// `y` (Float64Array).
#[wasm_bindgen]
pub fn transit_routes(network: &Network) -> Result<Array, JsError> {
    let r = match network.net.transit() {
        Some(_) => transit_routes_core(&network.net).map_err(route_err)?,
        None => Default::default(),
    };
    let (mut n_points, mut x, mut y) = (Vec::with_capacity(r.len()), Vec::new(), Vec::new());
    for i in 0..r.len() {
        let g = r.geometry(i);
        n_points.push(g.len() as i32);
        for &[px, py] in g {
            x.push(px);
            y.push(py);
        }
    }
    let route_id: Vec<Option<&str>> = r.route_id.iter().map(|s| Some(s.as_str())).collect();
    Ok([
        opt_strings(&r.agency_id),
        opt_strings(&r.agency_name),
        opt_strings(&route_id),
        opt_strings(&r.long_name),
        opt_strings(&r.short_name),
        str_array(&r.mode),
        i32_array(&n_points),
        f64_array(&x),
        f64_array(&y),
    ]
    .into_iter()
    .collect())
}

/// The operating dates of the network's services (`yyyymmdd`, ascending),
/// as r5r's `check_transit_availability()` discovers them.
#[wasm_bindgen]
pub fn transit_service_dates(network: &Network) -> Result<Vec<u32>, JsError> {
    match network.net.transit() {
        Some(_) => transit_service_dates_core(&network.net).map_err(route_err),
        None => Ok(Vec::new()),
    }
}

/// The errors gtfs-lib reports for one feed (its name and the bytes of
/// its `.zip`), as the columns of r5r's `gtfs_errors.csv`: `[file, line
/// (Int32Array), type, field, id, priority]` (arrays, null where missing).
#[wasm_bindgen]
pub fn gtfs_errors(name: &str, zip: &[u8]) -> Result<Array, JsError> {
    let rows = gtfs_errors_core(name, zip).map_err(err)?;
    let line: Vec<i32> = rows.iter().map(|e| e.line()).collect();
    Ok([
        opt_strings(&rows.iter().map(|e| e.file()).collect::<Vec<_>>()),
        i32_array(&line),
        str_array(&rows.iter().map(|e| e.error_type()).collect::<Vec<_>>()),
        opt_strings(&rows.iter().map(|e| e.field()).collect::<Vec<_>>()),
        opt_strings(&rows.iter().map(|e| e.id()).collect::<Vec<_>>()),
        str_array(
            &rows
                .iter()
                .map(|e| e.priority().as_str())
                .collect::<Vec<_>>(),
        ),
    ]
    .into_iter()
    .collect())
}
