//! `locate_facilities()` and `floating_catchment()`: the location models of
//! the core on a table of travel times the Python side has computed (or
//! was given).

use peabiru_core::{
    floating_catchment as floating_catchment_core, locate_facilities as locate_facilities_core,
    CancelToken, CostTable, FloatingCatchmentParams, LocationModel, LocationParams,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::{decay, f64_bytes, f64s, i32_bytes, interruptible, value_err, NA_I32};

/// Decode a buffer of native-endian `i32` indices.
fn indices(b: &[u8]) -> PyResult<Vec<u32>> {
    if b.len() % 4 != 0 {
        return Err(value_err("an index buffer must hold whole i32 values"));
    }
    b.chunks_exact(4)
        .map(|c| {
            u32::try_from(i32::from_ne_bytes(c.try_into().expect("4 bytes")))
                .map_err(|_| value_err("the indices of a travel-time table cannot be negative"))
        })
        .collect()
}

fn table(
    n_demand: usize,
    n_sites: usize,
    pair_demand: &[u8],
    pair_site: &[u8],
    pair_minutes: &[u8],
) -> PyResult<CostTable> {
    CostTable::from_pairs(
        n_demand,
        n_sites,
        &indices(pair_demand)?,
        &indices(pair_site)?,
        &f64s(pair_minutes)?,
    )
    .map_err(value_err)
}

/// The solution of a location model as a dict of columns. The pairs are
/// 0-based `i32` indices of demand points and sites with their minutes
/// (`f64`); `demand` holds one weight per demand point.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub(crate) fn locate_facilities<'py>(
    py: Python<'py>,
    n_demand: usize,
    n_sites: usize,
    pair_demand: &[u8],
    pair_site: &[u8],
    pair_minutes: &[u8],
    demand: &[u8],
    existing: Vec<usize>,
    model: &str,
    n_facilities: usize,
    cutoff: f64,
    swap: bool,
    min_gain: f64,
    unreachable_time: Option<f64>,
    n_threads: usize,
) -> PyResult<Bound<'py, PyDict>> {
    let costs = table(n_demand, n_sites, pair_demand, pair_site, pair_minutes)?;
    let demand = f64s(demand)?;
    let cancel = CancelToken::new();
    let mut p = LocationParams::default();
    p.model = LocationModel::from_name(model)
        .ok_or_else(|| value_err(format!("unknown location model '{model}'")))?;
    p.n_facilities = n_facilities;
    p.cutoff = cutoff;
    p.existing = existing;
    p.swap = swap;
    p.min_gain = min_gain;
    p.unreachable_time = unreachable_time;
    p.n_threads = n_threads;
    p.cancel = cancel.clone();
    let s = interruptible(py, &cancel, || {
        locate_facilities_core(&costs, &demand, &p)
    })?;
    let as_i32 = |v: &[usize]| -> Vec<i32> { v.iter().map(|&x| x as i32).collect() };
    let out = PyDict::new(py);
    out.set_item("sites", i32_bytes(py, &as_i32(&s.sites))?)?;
    out.set_item("marginal", f64_bytes(py, &s.marginal)?)?;
    out.set_item("cumulative", f64_bytes(py, &s.cumulative)?)?;
    out.set_item("potential", f64_bytes(py, &s.potential)?)?;
    let assigned: Vec<i32> = s
        .assigned_site
        .iter()
        .map(|a| a.map_or(NA_I32, |j| j as i32))
        .collect();
    out.set_item("assigned_site", i32_bytes(py, &assigned)?)?;
    out.set_item("assigned_minutes", f64_bytes(py, &s.assigned_minutes)?)?;
    out.set_item("unservable", i32_bytes(py, &as_i32(&s.unservable))?)?;
    out.set_item("objective_before", s.objective_before)?;
    out.set_item("objective_greedy", s.objective_greedy)?;
    out.set_item("objective", s.objective)?;
    out.set_item("covered_before", s.covered_before)?;
    out.set_item("covered", s.covered)?;
    out.set_item("total_demand", s.total_demand)?;
    out.set_item("unservable_demand", s.unservable_demand)?;
    out.set_item("n_swaps", s.n_swaps)?;
    Ok(out)
}

/// The two-step floating catchment area as a dict of columns (see
/// [`locate_facilities`] for the table).
#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub(crate) fn floating_catchment<'py>(
    py: Python<'py>,
    n_demand: usize,
    n_sites: usize,
    pair_demand: &[u8],
    pair_site: &[u8],
    pair_minutes: &[u8],
    demand: &[u8],
    supply: &[u8],
    cutoff: f64,
    decay_function: &str,
    decay_value: f64,
) -> PyResult<Bound<'py, PyDict>> {
    let costs = table(n_demand, n_sites, pair_demand, pair_site, pair_minutes)?;
    let (demand, supply) = (f64s(demand)?, f64s(supply)?);
    let cancel = CancelToken::new();
    let mut p = FloatingCatchmentParams::default();
    p.cutoff = cutoff;
    p.decay = decay(decay_function, decay_value)?;
    p.cancel = cancel.clone();
    let f = interruptible(py, &cancel, || {
        floating_catchment_core(&costs, &demand, &supply, &p)
    })?;
    let out = PyDict::new(py);
    out.set_item("index", f64_bytes(py, &f.index)?)?;
    out.set_item("ratio", f64_bytes(py, &f.ratio)?)?;
    out.set_item("catchment_demand", f64_bytes(py, &f.catchment_demand)?)?;
    out.set_item("unreached_supply", f.unreached_supply)?;
    Ok(out)
}
