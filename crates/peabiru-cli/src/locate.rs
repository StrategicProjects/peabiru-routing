//! `peabiru locate` and `peabiru catchment`: the location models of the
//! core on the travel times from the demand points to the sites.

use crate::access::decay;
use crate::app::{CatchmentArgs, CatchmentTable, LocateArgs, LocateTable};
use crate::common::*;
use crate::matrix::{engine, swap_od, Call};
use peabiru_core::{
    floating_catchment, locate_facilities, CostTable, DecayFunction, FloatingCatchmentParams,
    LocationModel, LocationParams,
};

/// What both commands route with.
struct Routing<'a> {
    net: &'a crate::app::NetworkArgs,
    mode: &'a crate::app::ModeArgs,
    transit: &'a crate::app::TransitArgs,
    street: &'a crate::app::StreetArgs,
    draws: &'a crate::app::DrawArgs,
    fares: &'a crate::app::FareArgs,
    scenario: &'a crate::app::ScenarioArgs,
    percentile: &'a str,
    street_engine: crate::app::Engine,
}

/// The table of minutes from each demand point to each site, limited to
/// trips of up to `longest` minutes when given (a longer trip cannot
/// matter to the caller).
fn cost_table(
    ctx: &Ctx,
    r: &Routing,
    demand: &Points,
    sites: &Points,
    longest: Option<i32>,
) -> Res<CostTable> {
    let walk = ["WALK".to_string()];
    let m = modes(mode_list(r.mode, &walk), &r.mode.mode_egress)?;
    let when = departure(r.transit.departure.as_deref(), &m)?;
    let mut mtd = max_trip_duration(r.street)?;
    if let Some(longest) = longest {
        mtd = mtd.min(longest.max(1));
    }
    let street = street(r.street, &m, mtd)?;
    let call = transit_call(r.transit, Some(r.draws), when)?;
    let pcts = percentiles(&[r.percentile.to_string()])?;
    check_draws(
        call.draws,
        call.time_window,
        r.fares.fare_structure.is_some(),
    )?;
    let built = build(ctx, r.net)?;
    if m.has_transit() {
        check_service(ctx, &built.net, when.date)?;
    }
    let (fares, max_fare) = fares(r.fares, &built.net)?;
    let scen = scenario(ctx, r.scenario, &built.net)?;
    let c = Call {
        net: &built.net,
        modes: &m,
        street: &street,
        transit: &call,
        pcts,
        threads: threads(r.net),
        scenario: &scen,
        engine: engine(r.street_engine),
        fares,
        max_fare,
    };
    let swapped = swap_od(demand, sites, &m, built.has_tif);
    let (ro, rd) = if swapped {
        (sites, demand)
    } else {
        (demand, sites)
    };
    let rows = c.rows(&ro.coords, &rd.coords)?;
    let (mut pi, mut pj, mut pt) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..rows.from.len() {
        if let Some(t) = rows.times[k][0] {
            let (i, j) = if swapped {
                (rows.to[k], rows.from[k])
            } else {
                (rows.from[k], rows.to[k])
            };
            pi.push(i as u32);
            pj.push(j as u32);
            pt.push(f64::from(t));
        }
    }
    Ok(CostTable::from_pairs(
        demand.len(),
        sites.len(),
        &pi,
        &pj,
        &pt,
    )?)
}

/// The weights of a set of points: its one extra column, or 1 each.
fn weights(p: &Points) -> Vec<f64> {
    p.columns
        .first()
        .cloned()
        .unwrap_or_else(|| vec![1.0; p.len()])
}

fn columns(c: &Option<String>) -> Vec<String> {
    c.iter().cloned().collect()
}

fn coord(x: f64) -> String {
    num(x)
}

pub fn locate(a: LocateArgs) -> Res<()> {
    let ctx = Ctx::new("locate", a.net.quiet);
    let model = LocationModel::from_name(a.model.trim()).ok_or_else(|| {
        usage(format!(
            "--model must be max_cover, set_cover, p_median or p_center, got '{}'",
            a.model
        ))
    })?;
    let covering = matches!(model, LocationModel::MaxCover | LocationModel::SetCover);
    let cutoff = match (&a.cutoff, covering) {
        (Some(c), true) => {
            let c = number("cutoff", c)?;
            if !(c.is_finite() && c >= 0.0) {
                return Err(usage("--cutoff must be finite and not negative"));
            }
            Some(c)
        }
        (None, true) => {
            return Err(usage(format!(
                "--cutoff is required with --model {}",
                model.name()
            )))
        }
        (Some(_), false) => {
            return Err(usage(format!(
                "--cutoff must not be given with --model {}",
                model.name()
            )))
        }
        (None, false) => None,
    };
    let n_facilities = count("n-facilities", &a.n_facilities, 0)?;
    let min_gain = number("min-gain", &a.min_gain)?;
    let unreachable = a
        .unreachable_time
        .as_deref()
        .map(|v| number("unreachable-time", v))
        .transpose()?;
    let demand = read_points(&ctx, &a.demand, "demand", true, &columns(&a.demand_column))?;
    let candidates = read_points(&ctx, &a.candidates, "candidates", true, &[])?;
    let existing = a
        .existing
        .as_deref()
        .map(|p| read_points(&ctx, p, "existing", true, &[]))
        .transpose()?;
    let n_existing = existing.as_ref().map_or(0, Points::len);
    // the sites of the table: the existing facilities, then the candidates
    let mut sites = Points {
        ids: Vec::new(),
        coords: Vec::new(),
        columns: Vec::new(),
    };
    for p in existing.iter().chain([&candidates]) {
        sites.ids.extend(p.ids.iter().cloned());
        sites.coords.extend(p.coords.iter().copied());
    }
    if let Some(e) = &existing {
        if let Some(id) = e.ids.iter().find(|id| candidates.ids.contains(id)) {
            return Err(runtime(format!(
                "the candidates and the existing facilities share the id '{id}'"
            )));
        }
    }
    let routing = Routing {
        net: &a.net,
        mode: &a.mode,
        transit: &a.transit,
        street: &a.street,
        draws: &a.draws,
        fares: &a.fares,
        scenario: &a.scenario,
        percentile: &a.percentile,
        street_engine: a.street_engine,
    };
    let costs = cost_table(
        &ctx,
        &routing,
        &demand,
        &sites,
        cutoff.map(|c| c.ceil() as i32),
    )?;
    let w = weights(&demand);
    let mut p = LocationParams::default();
    p.model = model;
    p.n_facilities = n_facilities as usize;
    p.cutoff = cutoff.unwrap_or(f64::NAN);
    p.existing = (0..n_existing).collect();
    p.swap = !a.no_swap;
    p.min_gain = min_gain;
    p.unreachable_time = unreachable;
    p.n_threads = threads(&a.net);
    p.cancel = cancel_token().clone();
    let s = locate_facilities(&costs, &w, &p)?;

    let mut out = Out::open(a.out.output.as_deref())?;
    let rows = match a.table {
        LocateTable::Sites => {
            out.row(&["id", "lon", "lat", "order", "marginal", "cumulative"])?;
            for (k, &j) in s.sites.iter().enumerate() {
                out.row(&[
                    sites.ids[j].clone(),
                    coord(sites.coords[j].1),
                    coord(sites.coords[j].0),
                    (k + 1).to_string(),
                    num(s.marginal[k]),
                    num(s.cumulative[k]),
                ])?;
            }
            s.sites.len()
        }
        LocateTable::Candidates => {
            out.row(&["id", "lon", "lat", "potential", "chosen"])?;
            for j in n_existing..sites.len() {
                out.row(&[
                    sites.ids[j].clone(),
                    coord(sites.coords[j].1),
                    coord(sites.coords[j].0),
                    num(s.potential[j]),
                    s.sites.contains(&j).to_string(),
                ])?;
            }
            sites.len() - n_existing
        }
        LocateTable::Demand => {
            out.row(&[
                "id",
                "lon",
                "lat",
                "demand",
                "facility_id",
                "travel_time",
                "covered",
            ])?;
            for i in 0..demand.len() {
                let t = s.assigned_minutes[i];
                let covered = match cutoff {
                    Some(c) => t <= c,
                    None => !t.is_nan(),
                };
                out.row(&[
                    demand.ids[i].clone(),
                    coord(demand.coords[i].1),
                    coord(demand.coords[i].0),
                    num(w[i]),
                    s.assigned_site[i].map_or_else(String::new, |j| sites.ids[j].clone()),
                    num(t),
                    covered.to_string(),
                ])?;
            }
            demand.len()
        }
    };
    out.finish()?;
    let unserved = if s.unservable_demand > 0.0 {
        format!("; demand no site can serve: {}", num(s.unservable_demand))
    } else {
        String::new()
    };
    ctx.progress(format!(
        "{}: {} new site{}, objective {} (existing only {}, greedy {}, {} swap{}), total demand {}{unserved}; {rows} rows",
        model.name(),
        s.sites.len(),
        if s.sites.len() == 1 { "" } else { "s" },
        num(s.objective),
        num(s.objective_before),
        num(s.objective_greedy),
        s.n_swaps,
        if s.n_swaps == 1 { "" } else { "s" },
        num(s.total_demand),
    ));
    Ok(())
}

pub fn catchment(a: CatchmentArgs) -> Res<()> {
    let ctx = Ctx::new("catchment", a.net.quiet);
    let f = decay(&a.decay_function, a.decay_value.as_deref())?;
    let fixed = matches!(f, DecayFunction::FixedExponential { .. });
    let cutoff = match (&a.cutoff, fixed) {
        (Some(_), true) => return Err(usage("--cutoff must not be given with fixed_exponential")),
        (None, false) => return Err(usage("--cutoff is required with this --decay-function")),
        (None, true) => None,
        (Some(c), false) => {
            let c = number("cutoff", c)?;
            if !(c.is_finite() && c > 0.0) {
                return Err(usage("--cutoff must be finite and positive"));
            }
            Some(c)
        }
    };
    let demand = read_points(&ctx, &a.demand, "demand", true, &columns(&a.demand_column))?;
    let supply = read_points(&ctx, &a.supply, "supply", true, &columns(&a.supply_column))?;
    let routing = Routing {
        net: &a.net,
        mode: &a.mode,
        transit: &a.transit,
        street: &a.street,
        draws: &a.draws,
        fares: &a.fares,
        scenario: &a.scenario,
        percentile: &a.percentile,
        street_engine: a.street_engine,
    };
    // only the step function ignores trips longer than the cutoff
    let longest = cutoff
        .filter(|_| matches!(f, DecayFunction::Step))
        .map(|c| c.ceil() as i32);
    let costs = cost_table(&ctx, &routing, &demand, &supply, longest)?;
    let (w, cap) = (weights(&demand), weights(&supply));
    let mut p = FloatingCatchmentParams::default();
    p.cutoff = cutoff.unwrap_or(f64::NAN);
    p.decay = f;
    p.cancel = cancel_token().clone();
    let r = floating_catchment(&costs, &w, &cap, &p)?;
    let mut out = Out::open(a.out.output.as_deref())?;
    let rows = match a.table {
        CatchmentTable::Demand => {
            out.row(&["id", "lon", "lat", "demand", "accessibility"])?;
            for i in 0..demand.len() {
                out.row(&[
                    demand.ids[i].clone(),
                    coord(demand.coords[i].1),
                    coord(demand.coords[i].0),
                    num(w[i]),
                    num(r.index[i]),
                ])?;
            }
            demand.len()
        }
        CatchmentTable::Supply => {
            out.row(&["id", "lon", "lat", "supply", "catchment_demand", "ratio"])?;
            for j in 0..supply.len() {
                out.row(&[
                    supply.ids[j].clone(),
                    coord(supply.coords[j].1),
                    coord(supply.coords[j].0),
                    num(cap[j]),
                    num(r.catchment_demand[j]),
                    num(r.ratio[j]),
                ])?;
            }
            supply.len()
        }
    };
    out.finish()?;
    ctx.progress(format!(
        "{} demand points, {} facilities, {rows} rows",
        demand.len(),
        supply.len()
    ));
    Ok(())
}
