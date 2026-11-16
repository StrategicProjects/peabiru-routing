//! `locateFacilities()` and `floatingCatchment()`: the location models of
//! the core on a table of travel times the JavaScript side has computed
//! (or was given). They need no network and run on the calling thread.

use js_sys::{Array, Float64Array, Int32Array};
use peabiru_core::{
    distance_pairs as distance_pairs_core, floating_catchment as floating_catchment_core,
    locate_facilities as locate_facilities_core, CostTable, FloatingCatchmentParams, LocationModel,
    LocationParams,
};
use wasm_bindgen::prelude::*;

use crate::{decay, err, route_err, NA_I32};

/// The table from its reachable pairs, with 0-based indices.
fn table(
    n_demand: usize,
    n_sites: usize,
    pair_demand: &[i32],
    pair_site: &[i32],
    pair_minutes: &[f64],
) -> Result<CostTable, JsError> {
    let index = |v: &[i32]| -> Result<Vec<u32>, JsError> {
        v.iter()
            .map(|&x| {
                u32::try_from(x)
                    .map_err(|_| err("the indices of a travel-time table cannot be negative"))
            })
            .collect()
    };
    CostTable::from_pairs(
        n_demand,
        n_sites,
        &index(pair_demand)?,
        &index(pair_site)?,
        pair_minutes,
    )
    .map_err(route_err)
}

/// The solution of a location model: `[sites, marginal, cumulative,
/// potential, assigned_site, assigned_minutes, unservable, numbers,
/// served]`, the
/// indices 0-based (`assigned_site` missing for a demand point that
/// reaches no facility) and `numbers` holding `objective_before`,
/// `objective_greedy`, `objective`, `covered_before`, `covered`,
/// `total_demand`, `unservable_demand`, `n_swaps` and `upper_bound`. The first
/// `n_existing` sites are the existing facilities; `capacity` is empty or
/// has one value per site; `unreachable_time` not
/// positive for the default.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn locate_facilities(
    n_demand: usize,
    n_sites: usize,
    pair_demand: &[i32],
    pair_site: &[i32],
    pair_minutes: &[f64],
    demand: &[f64],
    n_existing: usize,
    model: &str,
    n_facilities: usize,
    cutoff: f64,
    swap: bool,
    min_gain: f64,
    coverage: f64,
    capacity: &[f64],
    unreachable_time: f64,
) -> Result<Array, JsError> {
    let costs = table(n_demand, n_sites, pair_demand, pair_site, pair_minutes)?;
    let mut p = LocationParams::default();
    p.model = LocationModel::from_name(model)
        .ok_or_else(|| err(format!("unknown location model '{model}'")))?;
    p.n_facilities = n_facilities;
    p.cutoff = cutoff;
    p.existing = (0..n_existing).collect();
    p.swap = swap;
    p.min_gain = min_gain;
    p.coverage = coverage;
    p.capacity = (!capacity.is_empty()).then(|| capacity.to_vec());
    p.unreachable_time = (unreachable_time > 0.0).then_some(unreachable_time);
    p.n_threads = 1;
    let s = locate_facilities_core(&costs, demand, &p).map_err(route_err)?;
    let as_i32 = |v: &[usize]| -> Vec<i32> { v.iter().map(|&x| x as i32).collect() };
    let assigned: Vec<i32> = s
        .assigned_site
        .iter()
        .map(|a| a.map_or(NA_I32, |j| j as i32))
        .collect();
    let numbers = [
        s.objective_before,
        s.objective_greedy,
        s.objective,
        s.covered_before,
        s.covered,
        s.total_demand,
        s.unservable_demand,
        s.n_swaps as f64,
        s.upper_bound,
    ];
    let out = Array::new();
    out.push(&Int32Array::from(&as_i32(&s.sites)[..]));
    out.push(&Float64Array::from(&s.marginal[..]));
    out.push(&Float64Array::from(&s.cumulative[..]));
    out.push(&Float64Array::from(&s.potential[..]));
    out.push(&Int32Array::from(&assigned[..]));
    out.push(&Float64Array::from(&s.assigned_minutes[..]));
    out.push(&Int32Array::from(&as_i32(&s.unservable)[..]));
    out.push(&Float64Array::from(&numbers[..]));
    out.push(&Float64Array::from(&s.served[..]));
    Ok(out)
}

/// The two-step floating catchment area: `[index, ratio,
/// catchment_demand]` (see [`locate_facilities`] for the table).
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn floating_catchment(
    n_demand: usize,
    n_sites: usize,
    pair_demand: &[i32],
    pair_site: &[i32],
    pair_minutes: &[f64],
    demand: &[f64],
    supply: &[f64],
    cutoff: f64,
    decay_function: &str,
    decay_value: f64,
) -> Result<Array, JsError> {
    let costs = table(n_demand, n_sites, pair_demand, pair_site, pair_minutes)?;
    let mut p = FloatingCatchmentParams::default();
    p.cutoff = cutoff;
    p.decay = decay(decay_function, decay_value)?;
    let f = floating_catchment_core(&costs, demand, supply, &p).map_err(route_err)?;
    let out = Array::new();
    out.push(&Float64Array::from(&f.index[..]));
    out.push(&Float64Array::from(&f.ratio[..]));
    out.push(&Float64Array::from(&f.catchment_demand[..]));
    Ok(out)
}

/// The pairs of points at most `max_distance` metres apart in a straight
/// line: `[from, to, metres]`, the indices 0-based.
#[wasm_bindgen]
pub fn distance_pairs(
    from_lon: &[f64],
    from_lat: &[f64],
    to_lon: &[f64],
    to_lat: &[f64],
    max_distance: f64,
) -> Result<Array, JsError> {
    let zip = |lon: &[f64], lat: &[f64]| -> Vec<(f64, f64)> {
        lon.iter().copied().zip(lat.iter().copied()).collect()
    };
    let d = distance_pairs_core(
        &zip(from_lon, from_lat),
        &zip(to_lon, to_lat),
        max_distance,
        &Default::default(),
    )
    .map_err(route_err)?;
    let as_i32 = |v: &[u32]| -> Vec<i32> { v.iter().map(|&x| x as i32).collect() };
    let out = Array::new();
    out.push(&Int32Array::from(&as_i32(&d.from)[..]));
    out.push(&Int32Array::from(&as_i32(&d.to)[..]));
    out.push(&Float64Array::from(&d.metres[..]));
    Ok(out)
}
