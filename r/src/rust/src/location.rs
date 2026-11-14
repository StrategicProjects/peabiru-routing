//! `locate_facilities()` and `floating_catchment()`: the location models of
//! the core on a table of travel times the R side has computed (or was
//! given).

use super::{decay, err, interruptible, Outcome};
use extendr_api::prelude::*;
use extendr_api::Result;
use peabiru_core::{
    floating_catchment, locate_facilities, CancelToken, CostTable, FloatingCatchmentParams,
    LocationModel, LocationParams,
};

/// The table from its reachable pairs, with 0-based indices.
fn table(
    n_demand: i32,
    n_sites: i32,
    pair_demand: &[i32],
    pair_site: &[i32],
    pair_minutes: &[f64],
) -> Result<CostTable> {
    let index = |v: &[i32]| -> Result<Vec<u32>> {
        v.iter()
            .map(|&x| {
                u32::try_from(x)
                    .map_err(|_| err("the indices of a travel-time table cannot be negative"))
            })
            .collect()
    };
    CostTable::from_pairs(
        n_demand.max(0) as usize,
        n_sites.max(0) as usize,
        &index(pair_demand)?,
        &index(pair_site)?,
        pair_minutes,
    )
    .map_err(err)
}

/// The solution of a location model as a list: `sites`, `unservable` and
/// `assigned_site` 0-based (`NA` for a demand point that reaches no
/// facility); `unreachable_time` not positive for the default.
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_locate_facilities(
    n_demand: i32,
    n_sites: i32,
    pair_demand: &[i32],
    pair_site: &[i32],
    pair_minutes: &[f64],
    demand: &[f64],
    n_existing: i32,
    model: &str,
    n_facilities: i32,
    cutoff: f64,
    swap: bool,
    min_gain: f64,
    unreachable_time: f64,
    n_threads: i32,
) -> Result<Robj> {
    let costs = table(n_demand, n_sites, pair_demand, pair_site, pair_minutes)?;
    let cancel = CancelToken::new();
    let mut p = LocationParams::default();
    p.model = LocationModel::from_name(model)
        .ok_or_else(|| err(format!("unknown location model '{model}'")))?;
    p.n_facilities = n_facilities.max(0) as usize;
    p.cutoff = cutoff;
    p.existing = (0..n_existing.max(0) as usize).collect();
    p.swap = swap;
    p.min_gain = min_gain;
    p.unreachable_time = (unreachable_time > 0.0).then_some(unreachable_time);
    p.n_threads = n_threads.max(0) as usize;
    p.cancel = cancel.clone();
    let s = match interruptible(&cancel, || locate_facilities(&costs, demand, &p))? {
        Outcome::Done(v) => v,
        Outcome::Stopped(cond) => return super::stopped(cond),
    };
    let as_i32 = |v: &[usize]| -> Vec<i32> { v.iter().map(|&x| x as i32).collect() };
    let assigned: Vec<Rint> = s
        .assigned_site
        .iter()
        .map(|a| a.map_or(Rint::na(), |j| Rint::from(j as i32)))
        .collect();
    let minutes: Vec<Rfloat> = s
        .assigned_minutes
        .iter()
        .map(|&t| {
            if t.is_nan() {
                Rfloat::na()
            } else {
                Rfloat::from(t)
            }
        })
        .collect();
    Ok(list!(
        sites = as_i32(&s.sites),
        marginal = s.marginal,
        cumulative = s.cumulative,
        potential = s.potential,
        assigned_site = Integers::from_values(assigned),
        assigned_minutes = Doubles::from_values(minutes),
        unservable = as_i32(&s.unservable),
        objective_before = s.objective_before,
        objective_greedy = s.objective_greedy,
        objective = s.objective,
        covered_before = s.covered_before,
        covered = s.covered,
        total_demand = s.total_demand,
        unservable_demand = s.unservable_demand,
        n_swaps = s.n_swaps as i32
    )
    .into())
}

/// The two-step floating catchment area as a list (see
/// `rs_locate_facilities` for the table).
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_floating_catchment(
    n_demand: i32,
    n_sites: i32,
    pair_demand: &[i32],
    pair_site: &[i32],
    pair_minutes: &[f64],
    demand: &[f64],
    supply: &[f64],
    cutoff: f64,
    decay_function: &str,
    decay_value: f64,
) -> Result<Robj> {
    let costs = table(n_demand, n_sites, pair_demand, pair_site, pair_minutes)?;
    let cancel = CancelToken::new();
    let mut p = FloatingCatchmentParams::default();
    p.cutoff = cutoff;
    p.decay = decay(decay_function, decay_value)?;
    p.cancel = cancel.clone();
    let f = match interruptible(&cancel, || floating_catchment(&costs, demand, supply, &p))? {
        Outcome::Done(v) => v,
        Outcome::Stopped(cond) => return super::stopped(cond),
    };
    Ok(list!(
        index = f.index,
        ratio = f.ratio,
        catchment_demand = f.catchment_demand,
        unreached_supply = f.unreached_supply
    )
    .into())
}

extendr_module! {
    mod location;
    fn rs_locate_facilities;
    fn rs_floating_catchment;
}
