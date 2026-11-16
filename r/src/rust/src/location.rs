//! `locate_facilities()` and `floating_catchment()`: the location models of
//! the core on a table of travel times the R side has computed (or was
//! given).

use super::{decay, err, interruptible, Outcome};
use extendr_api::prelude::*;
use extendr_api::Result;
use peabiru_core::{
    allocate_budget, distance_pairs, floating_catchment, locate_facilities, BudgetCriterion,
    BudgetGroup, CancelToken, CostTable, FloatingCatchmentParams, LocationModel, LocationParams,
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
    coverage: f64,
    capacity: &[f64],
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
    p.coverage = coverage;
    p.capacity = (!capacity.is_empty()).then(|| capacity.to_vec());
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
        upper_bound = s.upper_bound,
        served = s.served,
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

/// The pairs of points at most `max_distance` metres apart in a straight
/// line: `from` and `to` 0-based, and the metres.
/// @noRd
#[extendr]
fn rs_distance_pairs(
    from_lon: &[f64],
    from_lat: &[f64],
    to_lon: &[f64],
    to_lat: &[f64],
    max_distance: f64,
) -> Result<Robj> {
    let zip = |lon: &[f64], lat: &[f64]| -> Vec<(f64, f64)> {
        lon.iter().copied().zip(lat.iter().copied()).collect()
    };
    let (from, to) = (zip(from_lon, from_lat), zip(to_lon, to_lat));
    let cancel = CancelToken::new();
    let d = match interruptible(&cancel, || {
        distance_pairs(&from, &to, max_distance, &cancel)
    })? {
        Outcome::Done(v) => v,
        Outcome::Stopped(cond) => return super::stopped(cond),
    };
    let as_i32 = |v: &[u32]| -> Vec<i32> { v.iter().map(|&x| x as i32).collect() };
    Ok(list!(
        from = as_i32(&d.from),
        to = as_i32(&d.to),
        metres = d.metres
    )
    .into())
}

/// The groups of a budget allocation from the gains of all of them end to
/// end and how many each has.
fn budget_groups<'a>(
    marginal: &'a [f64],
    lengths: &[i32],
    covered: &[f64],
    total: &[f64],
) -> std::result::Result<Vec<BudgetGroup<'a>>, String> {
    if lengths.len() != covered.len() || lengths.len() != total.len() {
        return Err("a budget allocation needs one length, covered and total per group".into());
    }
    let mut groups = Vec::with_capacity(lengths.len());
    let mut at = 0usize;
    for (g, &n) in lengths.iter().enumerate() {
        let n = n.max(0) as usize;
        let Some(part) = marginal.get(at..at + n) else {
            return Err("the gains of a budget allocation are shorter than their lengths".into());
        };
        groups.push(BudgetGroup {
            marginal: part,
            covered: covered[g],
            total: total[g],
        });
        at += n;
    }
    Ok(groups)
}

/// A budget of facilities shared among groups: `group` 0-based, `gain`,
/// `covered` and `lowest_share` per facility, `units` per group.
/// @noRd
#[extendr]
fn rs_allocate_budget(
    marginal: &[f64],
    lengths: &[i32],
    covered: &[f64],
    total: &[f64],
    budget: i32,
    criterion: &str,
) -> Result<Robj> {
    let groups = budget_groups(marginal, lengths, covered, total).map_err(err)?;
    let criterion = BudgetCriterion::from_name(criterion)
        .ok_or_else(|| err(format!("unknown criterion '{criterion}'")))?;
    let a = allocate_budget(&groups, budget.max(0) as usize, criterion).map_err(err)?;
    let as_i32 = |v: &[usize]| -> Vec<i32> { v.iter().map(|&x| x as i32).collect() };
    Ok(list!(
        group = as_i32(&a.group),
        gain = a.gain,
        covered = a.covered,
        lowest_share = a.lowest_share,
        units = as_i32(&a.units)
    )
    .into())
}

extendr_module! {
    mod location;
    fn rs_allocate_budget;
    fn rs_distance_pairs;
    fn rs_locate_facilities;
    fn rs_floating_catchment;
}
