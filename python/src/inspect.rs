//! Bindings of r5r's network functions (`find_snap()`,
//! `street_network_bbox()`, `street_network_to_sf()`,
//! `transit_network_to_sf()`, `check_transit_availability()`,
//! `get_gtfs_errors()`): as in the rest of the extension, the Python code
//! checks the arguments and builds the tables; these hand back columns.

use super::{f64_bytes, i32_bytes, points, runtime_err, street_mode, value_err, Network};
use peabiru_core::{
    find_snap as find_snap_core, gtfs_errors as gtfs_errors_core, gtfs_files_in_dir,
    street_vertices as street_vertices_core, transit_routes as transit_routes_core,
    transit_service_dates as transit_service_dates_core, transit_stops as transit_stops_core,
};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

fn bool_bytes<'py>(py: Python<'py>, v: &[bool]) -> Bound<'py, PyBytes> {
    let b: Vec<u8> = v.iter().map(|&x| u8::from(x)).collect();
    PyBytes::new(py, &b)
}

/// Snaps of r5r's `find_snap()` as `snap_lat`, `snap_lon`, `distance`
/// (f64, NaN where not found) and `found` (bool bytes).
#[pyfunction]
fn find_snap<'py>(
    py: Python<'py>,
    network: &Network,
    lat: &[u8],
    lon: &[u8],
    radius: f64,
    mode: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let pts = points(lat, lon)?;
    let mode = street_mode(mode)?;
    let snaps = py
        .detach(|| find_snap_core(&network.net, &pts, radius, mode))
        .map_err(value_err)?;
    let col = |f: fn(&peabiru_core::Snap) -> f64| -> Vec<f64> {
        snaps
            .iter()
            .map(|s| s.as_ref().map_or(f64::NAN, f))
            .collect()
    };
    let d = PyDict::new(py);
    d.set_item("snap_lat", f64_bytes(py, &col(|s| s.lat))?)?;
    d.set_item("snap_lon", f64_bytes(py, &col(|s| s.lon))?)?;
    d.set_item("distance", f64_bytes(py, &col(|s| s.distance_m))?)?;
    let found: Vec<bool> = snaps.iter().map(Option::is_some).collect();
    d.set_item("found", bool_bytes(py, &found))?;
    Ok(d)
}

/// `[xmin, ymin, xmax, ymax]` of the street network (R5's envelope).
#[pyfunction]
fn street_bbox(network: &Network) -> [f64; 4] {
    network.net.bbox()
}

/// The vertices of r5r's `street_network_to_sf()`, as columns.
#[pyfunction]
fn street_vertices<'py>(py: Python<'py>, network: &Network) -> PyResult<Bound<'py, PyDict>> {
    let v = street_vertices_core(&network.net);
    let index: Vec<i32> = v.index.iter().map(|&i| i as i32).collect();
    let d = PyDict::new(py);
    d.set_item("index", i32_bytes(py, &index)?)?;
    d.set_item("lat", f64_bytes(py, &v.lat)?)?;
    d.set_item("lon", f64_bytes(py, &v.lon)?)?;
    d.set_item("park_and_ride", bool_bytes(py, &v.park_and_ride))?;
    d.set_item("bike_sharing", bool_bytes(py, &v.bike_sharing))?;
    Ok(d)
}

/// The stops of r5r's `transit_network_to_sf()`, as columns (NaN
/// coordinates for stops not linked to the streets).
#[pyfunction]
fn transit_stops<'py>(py: Python<'py>, network: &Network) -> PyResult<Bound<'py, PyDict>> {
    let s = match &network.transit {
        Some(t) => transit_stops_core(&network.net, &t.layer),
        None => Default::default(),
    };
    let index: Vec<i32> = s.stop_index.iter().map(|&i| i as i32).collect();
    let nan = |v: &[Option<f64>]| -> Vec<f64> { v.iter().map(|x| x.unwrap_or(f64::NAN)).collect() };
    let d = PyDict::new(py);
    d.set_item("stop_index", i32_bytes(py, &index)?)?;
    d.set_item("stop_id", s.stop_id)?;
    d.set_item("stop_name", s.stop_name)?;
    d.set_item("lat", f64_bytes(py, &nan(&s.lat))?)?;
    d.set_item("lon", f64_bytes(py, &nan(&s.lon))?)?;
    d.set_item("linked_to_street", bool_bytes(py, &s.linked_to_street))?;
    Ok(d)
}

/// The routes of r5r's `transit_network_to_sf()`, as columns, with the
/// line of each row as `n_points` (int32) and flat `x`, `y` (f64).
#[pyfunction]
fn transit_routes<'py>(py: Python<'py>, network: &Network) -> PyResult<Bound<'py, PyDict>> {
    let r = match &network.transit {
        Some(t) => transit_routes_core(&network.net, &t.layer).map_err(value_err)?,
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
    let d = PyDict::new(py);
    d.set_item("agency_id", r.agency_id.clone())?;
    d.set_item("agency_name", r.agency_name.clone())?;
    d.set_item("route_id", r.route_id.clone())?;
    d.set_item("long_name", r.long_name.clone())?;
    d.set_item("short_name", r.short_name.clone())?;
    d.set_item("mode", r.mode.clone())?;
    d.set_item("n_points", i32_bytes(py, &n_points)?)?;
    d.set_item("x", f64_bytes(py, &x)?)?;
    d.set_item("y", f64_bytes(py, &y)?)?;
    Ok(d)
}

/// The operating dates of the network's services (`yyyymmdd`, ascending),
/// as r5r's `check_transit_availability()` discovers them.
#[pyfunction]
fn transit_service_dates(network: &Network) -> PyResult<Vec<u32>> {
    match &network.transit {
        Some(t) => transit_service_dates_core(&t.layer).map_err(value_err),
        None => Ok(Vec::new()),
    }
}

/// The paths of the network's GTFS feeds, in loading order.
#[pyfunction]
fn feed_paths(network: &Network) -> Vec<String> {
    network.transit.as_ref().map_or_else(Vec::new, |t| {
        t.layer
            .feed_reports()
            .iter()
            .map(|f| f.path.to_string_lossy().into_owned())
            .collect()
    })
}

/// The GTFS feeds of a directory, in the order R5 loads them.
#[pyfunction]
fn gtfs_files(dir: &str) -> PyResult<Vec<String>> {
    Ok(gtfs_files_in_dir(dir)
        .map_err(|e| runtime_err(format!("{dir}: {e}")))?
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

/// The errors gtfs-lib reports for the feeds at `paths`, read in this
/// order, as the columns of r5r's `gtfs_errors.csv`.
#[pyfunction]
fn gtfs_errors<'py>(py: Python<'py>, paths: Vec<String>) -> PyResult<Bound<'py, PyDict>> {
    let rows = py.detach(|| -> Result<_, String> {
        let mut rows = Vec::new();
        for p in &paths {
            let bytes = std::fs::read(p).map_err(|e| format!("{p}: {e}"))?;
            let name = std::path::Path::new(p)
                .file_name()
                .map_or_else(|| p.clone(), |n| n.to_string_lossy().into_owned());
            rows.extend(gtfs_errors_core(&name, &bytes).map_err(|e| e.to_string())?);
        }
        Ok(rows)
    });
    let rows = rows.map_err(runtime_err)?;
    let d = PyDict::new(py);
    d.set_item("file", rows.iter().map(|e| e.file()).collect::<Vec<_>>())?;
    let line: Vec<i32> = rows.iter().map(|e| e.line()).collect();
    d.set_item("line", i32_bytes(py, &line)?)?;
    d.set_item(
        "type",
        rows.iter().map(|e| e.error_type()).collect::<Vec<_>>(),
    )?;
    d.set_item("field", rows.iter().map(|e| e.field()).collect::<Vec<_>>())?;
    d.set_item("id", rows.iter().map(|e| e.id()).collect::<Vec<_>>())?;
    d.set_item(
        "priority",
        rows.iter()
            .map(|e| e.priority().as_str())
            .collect::<Vec<_>>(),
    )?;
    Ok(d)
}

/// Add the functions of this module to the extension.
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(find_snap, m)?)?;
    m.add_function(wrap_pyfunction!(street_bbox, m)?)?;
    m.add_function(wrap_pyfunction!(street_vertices, m)?)?;
    m.add_function(wrap_pyfunction!(transit_stops, m)?)?;
    m.add_function(wrap_pyfunction!(transit_routes, m)?)?;
    m.add_function(wrap_pyfunction!(transit_service_dates, m)?)?;
    m.add_function(wrap_pyfunction!(feed_paths, m)?)?;
    m.add_function(wrap_pyfunction!(gtfs_files, m)?)?;
    m.add_function(wrap_pyfunction!(gtfs_errors, m)?)?;
    Ok(())
}
