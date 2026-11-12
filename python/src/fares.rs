//! Fare structures and `pareto_frontier()`. A structure crosses the
//! boundary as a flat dict of lists (see `peabiru/_fares.py`), which the
//! Python side turns into a fare structure (a dict with DataFrames).

use super::{
    f64_bytes, i32_bytes, interruptible, parse_transit_modes, points, street_mode, transit,
    transit_params, transit_street_mode, value_err, Network,
};
use peabiru_core::{
    pareto_frontier, read_fare_structure, setup_fare_structure, write_fare_structure, FarePerRoute,
    FarePerTransfer, FarePerType, FareStructure,
};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use std::sync::Arc;

fn get<'py, T>(d: &Bound<'py, PyDict>, key: &str) -> PyResult<T>
where
    T: for<'a> FromPyObject<'a, 'py>,
{
    d.get_item(key)?
        .ok_or_else(|| value_err(format!("fare structure: {key} is missing")))?
        .extract::<T>()
        .map_err(|e| {
            let e: PyErr = e.into();
            value_err(format!("fare structure: {key}: {e}"))
        })
}

/// The core's structure of the flat dict.
pub(crate) fn from_py(d: &Bound<'_, PyDict>) -> PyResult<FareStructure> {
    let mut s = FareStructure::default();
    s.max_discounted_transfers = get(d, "max_discounted_transfers")?;
    s.transfer_time_allowance = get(d, "transfer_time_allowance")?;
    s.fare_cap = get(d, "fare_cap")?;
    let types: Vec<Option<String>> = get(d, "type")?;
    let unl: Vec<bool> = get(d, "unlimited_transfers")?;
    let same: Vec<bool> = get(d, "allow_same_route_transfer")?;
    let use_route: Vec<bool> = get(d, "use_route_fare")?;
    let fare: Vec<f64> = get(d, "fare")?;
    for k in 0..types.len() {
        let mut t = FarePerType::default();
        t.fare_type = types[k].clone();
        t.unlimited_transfers = unl[k];
        t.allow_same_route_transfer = same[k];
        t.use_route_fare = use_route[k];
        t.fare = fare[k];
        s.fares_per_type.push(t);
    }
    let a: Vec<Option<String>> = get(d, "first_leg")?;
    let b: Vec<Option<String>> = get(d, "second_leg")?;
    let f: Vec<f64> = get(d, "transfer_fare")?;
    for k in 0..a.len() {
        let mut t = FarePerTransfer::default();
        t.first_leg = a[k].clone();
        t.second_leg = b[k].clone();
        t.fare = f[k];
        s.fares_per_transfer.push(t);
    }
    let col = |k: &str| -> PyResult<Vec<Option<String>>> { get(d, k) };
    let (ai, an, ri, rs, rl, m, ft) = (
        col("agency_id")?,
        col("agency_name")?,
        col("route_id")?,
        col("route_short_name")?,
        col("route_long_name")?,
        col("mode")?,
        col("fare_type")?,
    );
    let rf: Vec<f64> = get(d, "route_fare")?;
    for k in 0..rf.len() {
        let mut r = FarePerRoute::default();
        r.agency_id = ai[k].clone();
        r.agency_name = an[k].clone();
        r.route_id = ri[k].clone();
        r.route_short_name = rs[k].clone();
        r.route_long_name = rl[k].clone();
        r.mode = m[k].clone();
        r.route_fare = rf[k];
        r.fare_type = ft[k].clone();
        s.fares_per_route.push(r);
    }
    s.debug_output_file = get(d, "output_file")?;
    s.debug_trip_info = get(d, "trip_info")?;
    Ok(s)
}

/// The flat dict of the core's structure.
fn to_py<'py>(py: Python<'py>, s: &FareStructure) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("max_discounted_transfers", s.max_discounted_transfers)?;
    d.set_item("transfer_time_allowance", s.transfer_time_allowance)?;
    d.set_item("fare_cap", s.fare_cap)?;
    let t = &s.fares_per_type;
    d.set_item(
        "type",
        t.iter().map(|x| x.fare_type.clone()).collect::<Vec<_>>(),
    )?;
    d.set_item(
        "unlimited_transfers",
        t.iter().map(|x| x.unlimited_transfers).collect::<Vec<_>>(),
    )?;
    d.set_item(
        "allow_same_route_transfer",
        t.iter()
            .map(|x| x.allow_same_route_transfer)
            .collect::<Vec<_>>(),
    )?;
    d.set_item(
        "use_route_fare",
        t.iter().map(|x| x.use_route_fare).collect::<Vec<_>>(),
    )?;
    d.set_item("fare", t.iter().map(|x| x.fare).collect::<Vec<_>>())?;
    let tr = &s.fares_per_transfer;
    d.set_item(
        "first_leg",
        tr.iter().map(|x| x.first_leg.clone()).collect::<Vec<_>>(),
    )?;
    d.set_item(
        "second_leg",
        tr.iter().map(|x| x.second_leg.clone()).collect::<Vec<_>>(),
    )?;
    d.set_item(
        "transfer_fare",
        tr.iter().map(|x| x.fare).collect::<Vec<_>>(),
    )?;
    let r = &s.fares_per_route;
    d.set_item(
        "agency_id",
        r.iter().map(|x| x.agency_id.clone()).collect::<Vec<_>>(),
    )?;
    d.set_item(
        "agency_name",
        r.iter().map(|x| x.agency_name.clone()).collect::<Vec<_>>(),
    )?;
    d.set_item(
        "route_id",
        r.iter().map(|x| x.route_id.clone()).collect::<Vec<_>>(),
    )?;
    d.set_item(
        "route_short_name",
        r.iter()
            .map(|x| x.route_short_name.clone())
            .collect::<Vec<_>>(),
    )?;
    d.set_item(
        "route_long_name",
        r.iter()
            .map(|x| x.route_long_name.clone())
            .collect::<Vec<_>>(),
    )?;
    d.set_item("mode", r.iter().map(|x| x.mode.clone()).collect::<Vec<_>>())?;
    d.set_item(
        "route_fare",
        r.iter().map(|x| x.route_fare).collect::<Vec<_>>(),
    )?;
    d.set_item(
        "fare_type",
        r.iter().map(|x| x.fare_type.clone()).collect::<Vec<_>>(),
    )?;
    d.set_item("output_file", &s.debug_output_file)?;
    d.set_item("trip_info", &s.debug_trip_info)?;
    Ok(d)
}

/// A routing call's fare structure (`None` for none).
pub(crate) fn fare_arg(fare: Option<&Bound<'_, PyDict>>) -> PyResult<Option<Arc<FareStructure>>> {
    fare.map(|d| from_py(d).map(Arc::new)).transpose()
}

/// `setup_fare_structure()`, as the flat dict.
#[pyfunction]
fn fare_setup<'py>(
    py: Python<'py>,
    network: &Bound<'py, Network>,
    base_fare: f64,
    by: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let n = network.get();
    let s = setup_fare_structure(&n.net, base_fare, by).map_err(value_err)?;
    to_py(py, &s)
}

/// `read_fare_structure()` of the bytes of a `.zip` file.
#[pyfunction]
fn fare_read<'py>(py: Python<'py>, bytes: &[u8], encoding: &str) -> PyResult<Bound<'py, PyDict>> {
    let s = read_fare_structure(bytes, encoding).map_err(value_err)?;
    to_py(py, &s)
}

/// `write_fare_structure()`: the bytes of the `.zip` file.
#[pyfunction]
fn fare_write<'py>(py: Python<'py>, fare: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyBytes>> {
    let s = from_py(fare)?;
    Ok(PyBytes::new(
        py,
        &write_fare_structure(&s).map_err(value_err)?,
    ))
}

/// The checks of a structure against a network's routes.
#[pyfunction]
fn fare_check(network: &Bound<'_, Network>, fare: &Bound<'_, PyDict>) -> PyResult<()> {
    let s = from_py(fare)?;
    s.validate().map_err(value_err)?;
    s.validate_for(&network.get().net).map_err(value_err)
}

/// `pareto_frontier()`: `(from, to, percentile, travel_time)` as
/// int32 bytes and `monetary_cost` as float64 bytes; `transit_modes`
/// empty routes by `mode` alone.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn pareto<'py>(
    py: Python<'py>,
    network: &Bound<'py, Network>,
    olat: &[u8],
    olon: &[u8],
    dlat: &[u8],
    dlon: &[u8],
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
    n_threads: usize,
    fare: &Bound<'py, PyDict>,
    fare_cutoffs: Vec<f64>,
) -> PyResult<Bound<'py, PyDict>> {
    let n = network.get();
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    let with_transit = !transit_modes.is_empty();
    let modes = parse_transit_modes(if with_transit {
        transit_modes
    } else {
        "TRANSIT"
    })?;
    let mut p = transit_params(
        modes,
        street_mode(mode)?,
        if with_transit {
            transit_street_mode(egress_mode)?
        } else {
            street_mode(mode)?
        },
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
        n_threads,
    );
    p.fare_structure = Some(Arc::new(from_py(fare)?));
    if with_transit {
        transit(n)?;
    } else {
        // by `mode` alone
        p.transit_modes.clear();
    }
    let f = interruptible(py, &p.cancel, || {
        pareto_frontier(&n.net, &o, &d, &p, &fare_cutoffs)
    })?;
    let r = &f.rows;
    let out = PyDict::new(py);
    let ints = |v: Vec<i32>| i32_bytes(py, &v);
    out.set_item("from", ints(r.iter().map(|x| x.origin as i32).collect())?)?;
    out.set_item(
        "to",
        ints(r.iter().map(|x| x.destination as i32).collect())?,
    )?;
    out.set_item(
        "percentile",
        ints(r.iter().map(|x| x.percentile).collect())?,
    )?;
    out.set_item(
        "travel_time",
        ints(r.iter().map(|x| x.travel_time).collect())?,
    )?;
    out.set_item(
        "monetary_cost",
        f64_bytes(py, &r.iter().map(|x| x.monetary_cost).collect::<Vec<_>>())?,
    )?;
    Ok(out)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(fare_setup, m)?)?;
    m.add_function(wrap_pyfunction!(fare_read, m)?)?;
    m.add_function(wrap_pyfunction!(fare_write, m)?)?;
    m.add_function(wrap_pyfunction!(fare_check, m)?)?;
    m.add_function(wrap_pyfunction!(pareto, m)?)?;
    Ok(())
}
