//! `peabiru matrix`, `peabiru expanded` and `peabiru arrival`.

use crate::app::{ArrivalArgs, Engine, ExpandedArgs, MatrixArgs};
use crate::common::*;
use peabiru_core::{
    arrival_departure, arrival_travel_time_matrix, expanded_travel_time_matrix,
    street_travel_time_matrix, transit_travel_time_matrix, CsvOutput, ExpandedMatrix, Mode,
    Network, Scenario, StreetEngine, TransitParams,
};
use std::sync::Arc;

/// r5r's `reverse_if_direct_mode()`: with walking alone, more origins than
/// destinations and no elevation raster, r5r routes from the destinations
/// and swaps the columns back, and the two directions can differ.
pub fn swap_od(o: &Points, d: &Points, m: &Modes, has_tif: bool) -> bool {
    !m.has_transit() && m.street == Mode::Walk && o.len() > d.len() && !has_tif
}

/// The rows of a travel-time matrix: origin and destination indices (of
/// the points routed) and one time per percentile (`None`: NA).
pub struct Rows {
    pub from: Vec<usize>,
    pub to: Vec<usize>,
    pub times: Vec<Vec<Option<u32>>>,
}

/// What a matrix call needs once its arguments are checked.
pub struct Call<'a> {
    pub net: &'a Network,
    pub modes: &'a Modes,
    pub street: &'a Street,
    pub transit: &'a TransitCall,
    pub pcts: Vec<i32>,
    pub threads: usize,
    pub scenario: &'a Scenario,
    pub engine: StreetEngine,
    pub fares: Option<Arc<peabiru_core::FareStructure>>,
    pub max_fare: f64,
}

impl Call<'_> {
    fn transit_params(&self) -> TransitParams {
        let mut p = transit_params(
            self.modes,
            self.street,
            self.transit,
            self.pcts.clone(),
            self.threads,
            self.scenario,
        );
        p.fare_structure = self.fares.clone();
        p.max_fare = self.max_fare;
        p
    }

    /// The matrix from `o` to `d`, as r5r's `travel_time_matrix()` reports
    /// it: the pairs reached, with times beyond the trip limit as NA.
    pub fn rows(&self, o: &[(f64, f64)], d: &[(f64, f64)]) -> Res<Rows> {
        let mtd = self.street.lim.trip as u32;
        let np = self.pcts.len();
        let mut r = Rows {
            from: Vec::new(),
            to: Vec::new(),
            times: Vec::new(),
        };
        if self.modes.has_transit() {
            let m = transit_travel_time_matrix(self.net, o, d, &self.transit_params())?;
            for (i, row) in m.rows().enumerate() {
                for (j, cell) in row.chunks_exact(np).enumerate() {
                    if cell[0].is_none() {
                        continue;
                    }
                    r.from.push(i);
                    r.to.push(j);
                    r.times
                        .push(cell.iter().map(|v| v.filter(|&t| t <= mtd)).collect());
                }
            }
        } else {
            let mut p = route_params(
                self.street,
                self.street.lim.trip,
                self.threads,
                self.scenario,
            );
            p.engine = self.engine;
            let m = street_travel_time_matrix(self.net, o, d, self.modes.street, &p)?;
            let nd = d.len();
            for (k, v) in m.as_slice().iter().enumerate() {
                if let Some(v) = v {
                    r.from.push(k / nd);
                    r.to.push(k % nd);
                    let t = Some(*v).filter(|&t| t <= mtd);
                    r.times.push(vec![t; np]);
                }
            }
        }
        Ok(r)
    }

    /// The matrix written as r5r's `output_dir` writes it.
    pub fn write_dir(&self, dir: &str, o: &Points, d: &Points) -> Res<()> {
        let w = CsvOutput::new(dir, &o.ids, &d.ids, self.threads);
        let io = |e: std::io::Error| runtime(format!("cannot write {dir}: {e}"));
        if self.modes.has_transit() {
            let p = self.transit_params();
            for r in w.origin_chunks(d.len() * self.pcts.len()) {
                let m = transit_travel_time_matrix(self.net, &o.coords[r.clone()], &d.coords, &p)?;
                w.write_transit_matrix(r.start, &m).map_err(io)?;
            }
        } else {
            let mut p = route_params(
                self.street,
                self.street.lim.trip,
                self.threads,
                self.scenario,
            );
            p.engine = self.engine;
            for r in w.origin_chunks(d.len()) {
                let m = street_travel_time_matrix(
                    self.net,
                    &o.coords[r.clone()],
                    &d.coords,
                    self.modes.street,
                    &p,
                )?;
                w.write_street_matrix(r.start, &m, &self.pcts).map_err(io)?;
            }
        }
        Ok(())
    }
}

pub fn engine(e: Engine) -> StreetEngine {
    match e {
        Engine::Auto => StreetEngine::Auto,
        Engine::Search => StreetEngine::Search,
        Engine::Hierarchy => StreetEngine::Hierarchy,
    }
}

pub fn matrix(a: MatrixArgs) -> Res<()> {
    let ctx = Ctx::new("matrix", a.net.quiet);
    let (o, d) = read_od(
        &ctx,
        &a.points.origins,
        a.points.destinations.as_deref(),
        true,
        &[],
    )?;
    let walk = ["WALK".to_string()];
    let m = modes(mode_list(&a.mode, &walk), &a.mode.mode_egress)?;
    let when = departure(a.transit.departure.as_deref(), &m)?;
    let mtd = max_trip_duration(&a.street)?;
    let street = street(&a.street, &m, mtd)?;
    let call = transit_call(&a.transit, Some(&a.draws), when)?;
    let pcts = percentiles(&a.percentiles)?;
    check_draws(
        call.draws,
        call.time_window,
        a.fares.fare_structure.is_some(),
    )?;
    let dir = a.dir.output_dir.as_deref().map(output_dir).transpose()?;
    let built = build(&ctx, &a.net)?;
    if m.has_transit() {
        check_service(&ctx, &built.net, when.date)?;
    }
    let (fares, max_fare) = fares(&a.fares, &built.net)?;
    let scen = scenario(&ctx, &a.scenario, &built.net)?;
    let c = Call {
        net: &built.net,
        modes: &m,
        street: &street,
        transit: &call,
        pcts,
        threads: threads(&a.net),
        scenario: &scen,
        engine: engine(a.street_engine),
        fares,
        max_fare,
    };
    if let Some(dir) = dir {
        c.write_dir(&dir, &o, &d)?;
        ctx.progress(format!("{} origins written to {dir}", o.len()));
        return Ok(());
    }
    let swapped = swap_od(&o, &d, &m, built.has_tif);
    let (ro, rd) = if swapped { (&d, &o) } else { (&o, &d) };
    let rows = c.rows(&ro.coords, &rd.coords)?;
    let mut out = Out::open(a.out.output.as_deref())?;
    let mut header = vec!["from_id".to_string(), "to_id".to_string()];
    header.extend(c.pcts.iter().map(|p| format!("travel_time_p{p:02}")));
    out.row(&header)?;
    for k in 0..rows.from.len() {
        let (f, t) = if swapped {
            (&o.ids[rows.to[k]], &d.ids[rows.from[k]])
        } else {
            (&o.ids[rows.from[k]], &d.ids[rows.to[k]])
        };
        let mut rec = vec![f.clone(), t.clone()];
        rec.extend(rows.times[k].iter().map(|v| opt(*v)));
        out.row(&rec)?;
    }
    out.finish()?;
    ctx.progress(format!(
        "{} x {} pairs, {} rows",
        o.len(),
        d.len(),
        rows.from.len()
    ));
    Ok(())
}

/// The columns of an expanded matrix.
fn write_expanded(
    out: &mut Out,
    m: &ExpandedMatrix,
    o: &Points,
    d: &Points,
    first: usize,
    breakdown: bool,
    mtd: i32,
) -> Res<usize> {
    let mut n = 0;
    for (k, rows) in m.origins().enumerate() {
        let from = &o.ids[first + k];
        for r in rows {
            let trip = r.trip.as_ref().filter(|t| t.total_time <= f64::from(mtd));
            let mut rec = vec![
                from.clone(),
                d.ids[r.to as usize].clone(),
                clock(r.departure),
                r.draw.to_string(),
            ];
            if breakdown {
                for v in [
                    trip.map(|t| t.access_time),
                    trip.map(|t| t.wait_time),
                    trip.map(|t| t.ride_time),
                    trip.map(|t| t.transfer_time),
                    trip.map(|t| t.egress_time),
                ] {
                    rec.push(v.map_or_else(String::new, num));
                }
                rec.push(trip.map_or_else(String::new, |t| t.routes.clone()));
                rec.push(trip.map_or_else(String::new, |t| t.n_rides.to_string()));
            } else {
                rec.push(trip.map_or_else(String::new, |t| t.routes.clone()));
            }
            rec.push(trip.map_or_else(String::new, |t| num(t.total_time)));
            out.row(&rec)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Origins routed per call of an expanded matrix, to bound the rows held.
const BATCH: usize = 64;

#[allow(clippy::too_many_arguments)]
fn expanded_run(
    ctx: &Ctx,
    net: &Network,
    o: &Points,
    d: &Points,
    p: &TransitParams,
    arrival: bool,
    breakdown: bool,
    out: Option<&std::path::Path>,
    dir: Option<String>,
    threads: usize,
) -> Res<()> {
    let route = |o: &[(f64, f64)]| {
        if arrival {
            arrival_travel_time_matrix(net, o, &d.coords, p)
        } else {
            expanded_travel_time_matrix(net, o, &d.coords, p)
        }
    };
    if let Some(dir) = dir {
        let w = CsvOutput::new(&dir, &o.ids, &d.ids, threads);
        let freq = net.transit().is_some_and(|t| t.has_frequencies());
        let draws = if freq {
            p.draws_per_minute.max(1) as usize
        } else {
            1
        };
        let per_origin = d.len() * p.time_window.max(1) as usize * draws;
        for r in w.origin_chunks(per_origin) {
            let m = route(&o.coords[r.clone()])?;
            w.write_expanded(r.start, &m, breakdown)
                .map_err(|e| runtime(format!("cannot write {dir}: {e}")))?;
        }
        ctx.progress(format!("{} origins written to {dir}", o.len()));
        return Ok(());
    }
    let mut w = Out::open(out)?;
    let header: &[&str] = if breakdown {
        &[
            "from_id",
            "to_id",
            "departure_time",
            "draw_number",
            "access_time",
            "wait_time",
            "ride_time",
            "transfer_time",
            "egress_time",
            "routes",
            "n_rides",
            "total_time",
        ]
    } else {
        &[
            "from_id",
            "to_id",
            "departure_time",
            "draw_number",
            "routes",
            "total_time",
        ]
    };
    w.row(header)?;
    let mut n = 0;
    for first in (0..o.len()).step_by(BATCH) {
        let last = (first + BATCH).min(o.len());
        let m = route(&o.coords[first..last])?;
        n += write_expanded(&mut w, &m, o, d, first, breakdown, p.max_trip_duration)?;
    }
    w.finish()?;
    ctx.progress(format!("{} x {} pairs, {n} rows", o.len(), d.len()));
    Ok(())
}

/// The expanded and arrival matrices route by public transport only.
fn needs_transit(m: &Modes) -> Res<()> {
    if !m.has_transit() {
        return Err(usage(
            "an expanded or arrival travel time matrix needs public transport in --mode (use \
             `peabiru matrix` for street modes)",
        ));
    }
    Ok(())
}

pub fn expanded(a: ExpandedArgs) -> Res<()> {
    let ctx = Ctx::new("expanded", a.net.quiet);
    let (o, d) = read_od(
        &ctx,
        &a.points.origins,
        a.points.destinations.as_deref(),
        true,
        &[],
    )?;
    let walk = ["WALK".to_string()];
    let m = modes(mode_list(&a.mode, &walk), &a.mode.mode_egress)?;
    needs_transit(&m)?;
    let when = departure(a.transit.departure.as_deref(), &m)?;
    let mtd = max_trip_duration(&a.street)?;
    let street = street(&a.street, &m, mtd)?;
    let call = transit_call(&a.transit, Some(&a.draws), when)?;
    let dir = a.dir.output_dir.as_deref().map(output_dir).transpose()?;
    let built = build(&ctx, &a.net)?;
    check_service(&ctx, &built.net, when.date)?;
    let scen = scenario(&ctx, &a.scenario, &built.net)?;
    let th = threads(&a.net);
    let p = transit_params(&m, &street, &call, vec![50], th, &scen);
    expanded_run(
        &ctx,
        &built.net,
        &o,
        &d,
        &p,
        false,
        a.breakdown,
        a.out.output.as_deref(),
        dir,
        th,
    )
}

pub fn arrival(a: ArrivalArgs) -> Res<()> {
    let ctx = Ctx::new("arrival", a.net.quiet);
    let (o, d) = read_od(
        &ctx,
        &a.points.origins,
        a.points.destinations.as_deref(),
        true,
        &[],
    )?;
    let walk = ["WALK".to_string()];
    let m = modes(mode_list(&a.mode, &walk), &a.mode.mode_egress)?;
    needs_transit(&m)?;
    let mtd = max_trip_duration(&a.street)?;
    let street = street(&a.street, &m, mtd)?;
    let at = datetime("arrival", &a.arrival)?;
    // r5r: the departure is arrival - max_trip_duration, over a window of
    // max_trip_duration minutes
    let (date, time) = arrival_departure(at.date, at.time, street.lim.trip)?;
    let when = When { date, time };
    let mut targs = crate::app::TransitArgs {
        departure: None,
        time_window: street.lim.trip.to_string(),
        max_rides: a.max_rides.clone(),
    };
    targs.departure = None;
    let call = transit_call(&targs, Some(&a.draws), when)?;
    let dir = a.dir.output_dir.as_deref().map(output_dir).transpose()?;
    let built = build(&ctx, &a.net)?;
    check_service(&ctx, &built.net, when.date)?;
    let scen = scenario(&ctx, &a.scenario, &built.net)?;
    let th = threads(&a.net);
    let p = transit_params(&m, &street, &call, vec![50], th, &scen);
    expanded_run(
        &ctx,
        &built.net,
        &o,
        &d,
        &p,
        true,
        a.breakdown,
        a.out.output.as_deref(),
        dir,
        th,
    )
}
