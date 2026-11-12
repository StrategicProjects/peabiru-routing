//! `peabiru itineraries`, `peabiru options` and `peabiru pareto`.

use crate::app::{Format, ItinerariesArgs, OptionsArgs, ParetoArgs, TransitArgs};
use crate::common::*;
use peabiru_core::{
    detailed_itineraries_noted, pareto_frontier, trip_options, write_itineraries_csv,
    write_trip_options_csv, Criterion, ItineraryLeg, ItineraryParams, TripOptionsParams,
};
use std::path::Path;

/// r5r's pairing of origins and destinations: origin `i` to destination
/// `i`, one point to all the others, or every origin to every destination.
fn pairs(ctx: &Ctx, o: &Points, d: &Points, all_to_all: bool) -> Res<(Points, Points)> {
    let (no, nd) = (o.len(), d.len());
    if all_to_all || no == 1 || nd == 1 {
        let oi: Vec<usize> = (0..no)
            .flat_map(|i| std::iter::repeat(i).take(nd))
            .collect();
        let di: Vec<usize> = (0..no).flat_map(|_| 0..nd).collect();
        if !all_to_all && (no > 1 || nd > 1) {
            ctx.warn(if no == 1 {
                "the origins were expanded to match the number of destinations"
            } else {
                "the destinations were expanded to match the number of origins"
            });
        }
        return Ok((o.pick(&oi), d.pick(&di)));
    }
    if no != nd {
        return Err(usage(
            "without --all-to-all, the origins and destinations must have the same number of \
             rows, or one of them a single row",
        ));
    }
    Ok((o.clone(), d.clone()))
}

/// The checks of `output_dir` for files named after each pair.
fn pair_dir(dir: &Path, o: &Points, d: &Points) -> Res<String> {
    let dir = output_dir(dir)?;
    let mut seen = std::collections::HashSet::new();
    for (a, b) in o.ids.iter().zip(&d.ids) {
        if !seen.insert((a, b)) {
            return Err(usage(
                "with --output-dir, origin-destination pairs must be unique: each pair is \
                 written to a file named from_<id>_to_<id>.csv",
            ));
        }
    }
    let ids: Vec<&String> = o.ids.iter().chain(&d.ids).collect();
    check_file_ids(&ids)?;
    Ok(dir)
}

fn leg_links(l: &ItineraryLeg) -> [String; 4] {
    [
        l.osm_id_list(),
        l.edge_id_list(),
        l.board_stop_id.clone(),
        l.alight_stop_id.clone(),
    ]
}

const LINKS: [&str; 4] = [
    "osm_id_list",
    "edge_id_list",
    "board_stop_id",
    "alight_stop_id",
];

/// The departure arguments of a pairwise command.
struct Pairwise {
    o: Points,
    d: Points,
    m: Modes,
    street: Street,
    call: TransitCall,
}

fn pairwise(
    ctx: &Ctx,
    points: &crate::app::PointsArgs,
    all_to_all: bool,
    mode: &[String],
    egress: &str,
    transit: &TransitArgs,
    street_args: &crate::app::StreetArgs,
) -> Res<Pairwise> {
    let (o, d) = read_od(
        ctx,
        &points.origins,
        points.destinations.as_deref(),
        all_to_all,
        &[],
    )?;
    let (o, d) = pairs(ctx, &o, &d, all_to_all)?;
    let m = modes(mode, egress)?;
    let when = departure(transit.departure.as_deref(), &m)?;
    let mtd = max_trip_duration(street_args)?;
    let street = street(street_args, &m, mtd)?;
    let call = transit_call(transit, None, when)?;
    Ok(Pairwise {
        o,
        d,
        m,
        street,
        call,
    })
}

pub fn itineraries(a: ItinerariesArgs) -> Res<()> {
    let ctx = Ctx::new("itineraries", a.net.quiet);
    let walk = ["WALK".to_string()];
    let pw = pairwise(
        &ctx,
        &a.points,
        a.all_to_all,
        mode_list(&a.mode, &walk),
        &a.mode.mode_egress,
        &a.transit,
        &a.street,
    )?;
    let Pairwise {
        o,
        d,
        m,
        street,
        call,
    } = pw;
    if a.drop_geometry && a.osm_link_ids {
        return Err(usage(
            "--osm-link-ids needs the geometry: not with --drop-geometry",
        ));
    }
    let mcraptor = m.has_transit() && !(a.shortest_path && a.fares.fare_structure.is_none());
    check_draws(1, call.time_window, mcraptor)?;
    let sub = count("suboptimal-minutes", &a.suboptimal_minutes, 0)?;
    if a.fares.fare_structure.is_some() && sub > 0 {
        return Err(usage(
            "--suboptimal-minutes must be 0 when calculating fares",
        ));
    }
    if a.shortest_path && sub > 0 {
        return Err(usage(
            "--suboptimal-minutes must be 0 with --shortest-path true",
        ));
    }
    let dir = a
        .output_dir
        .as_deref()
        .map(|p| pair_dir(p, &o, &d))
        .transpose()?;
    let built = build(&ctx, &a.net)?;
    if m.has_transit() {
        check_service(&ctx, &built.net, call.when.date)?;
        if built.net.transit().is_some_and(|t| t.has_frequencies()) {
            return Err(usage(
                "detailed itineraries are not computed on networks with frequency-based GTFS \
                 (a frequencies table), as in r5r: convert the feed to timetables first",
            ));
        }
    }
    let (fares, max_fare) = fares(&a.fares, &built.net)?;
    let scen = scenario(&ctx, &a.scenario, &built.net)?;
    let mut p = ItineraryParams::default();
    p.scenario = scen;
    p.mode = m.street;
    p.transit_modes = m.transit.clone();
    if m.has_transit() {
        p.egress_mode = m.egress;
    }
    p.date = call.when.date;
    p.departure_time = call.when.time;
    p.time_window = call.time_window;
    p.max_walk_time = street.lim.walk;
    p.max_bike_time = street.lim.bike;
    p.max_car_time = street.lim.car;
    p.max_trip_duration = street.lim.trip;
    p.walk_speed = street.walk_speed;
    p.bike_speed = street.bike_speed;
    p.max_rides = call.max_rides;
    p.max_lts = street.max_lts;
    p.shortest_path = a.shortest_path;
    p.suboptimal_minutes = sub;
    let with_fare = fares.is_some();
    p.fare_structure = fares;
    p.max_fare = max_fare;
    p.n_threads = threads(&a.net);
    p.osm_link_ids = a.osm_link_ids;
    p.cancel = cancel_token().clone();
    let (res, notes) = detailed_itineraries_noted(&built.net, &o.coords, &d.coords, &p)?;
    for w in notes.warnings(o.len()) {
        ctx.warn(w);
    }
    if let Some(dir) = dir {
        let ids: Vec<(String, String)> = o.ids.iter().cloned().zip(d.ids.iter().cloned()).collect();
        let pts: Vec<_> = o
            .coords
            .iter()
            .copied()
            .zip(d.coords.iter().copied())
            .collect();
        write_itineraries_csv(&dir, &ids, &pts, &res, a.drop_geometry, a.osm_link_ids)
            .map_err(|e| runtime(format!("cannot write {dir}: {e}")))?;
        ctx.progress(format!("{} pairs written to {dir}", o.len()));
        return Ok(());
    }
    let mut cols: Vec<&str> = vec![
        "from_id",
        "from_lat",
        "from_lon",
        "to_id",
        "to_lat",
        "to_lon",
        "option",
        "departure_time",
        "total_duration",
        "total_distance",
    ];
    if with_fare {
        cols.push("total_fare");
    }
    cols.extend(["segment", "mode"]);
    if with_fare {
        cols.push("cumulative_fare");
    }
    cols.extend(["segment_duration", "wait", "distance", "route"]);
    if a.osm_link_ids {
        cols.extend(LINKS);
    }
    let out = Out::open(a.out.output.as_deref())?;
    let geojson = a.format == Format::Geojson;
    let mut csv = None;
    let mut gj = None;
    if geojson {
        gj = Some(GeoJson::start(out)?);
    } else {
        let mut out = out;
        let mut h = cols.clone();
        if !a.drop_geometry {
            h.push("geometry");
        }
        out.row(&h)?;
        csv = Some(out);
    }
    let mut n = 0;
    for (k, its) in res.iter().enumerate() {
        for it in its {
            for (j, l) in it.legs.iter().enumerate() {
                let mut vals: Vec<(String, bool)> = vec![
                    (o.ids[k].clone(), true),
                    (num(o.coords[k].0), false),
                    (num(o.coords[k].1), false),
                    (d.ids[k].clone(), true),
                    (num(d.coords[k].0), false),
                    (num(d.coords[k].1), false),
                    (it.option.to_string(), false),
                    (it.departure_time.clone(), true),
                    (num(it.total_duration), false),
                    (it.total_distance.to_string(), false),
                ];
                if with_fare {
                    vals.push((num(it.total_fare.unwrap_or(0.0)), false));
                }
                vals.push(((j + 1).to_string(), false));
                vals.push((l.mode.clone(), true));
                if with_fare {
                    vals.push((num(l.cumulative_fare.unwrap_or(0.0)), false));
                }
                vals.push((num(l.segment_duration), false));
                vals.push((num(l.wait), false));
                vals.push((l.distance.to_string(), false));
                vals.push((l.route.clone(), true));
                if a.osm_link_ids {
                    vals.extend(leg_links(l).into_iter().map(|s| (s, true)));
                }
                let geom = if a.drop_geometry {
                    Geom::None
                } else {
                    Geom::Line(&l.geometry)
                };
                if let Some(out) = csv.as_mut() {
                    let mut rec: Vec<String> = vals.into_iter().map(|v| v.0).collect();
                    if !a.drop_geometry {
                        rec.push(geom.wkt());
                    }
                    out.row(&rec)?;
                } else if let Some(g) = gj.as_mut() {
                    let props: Vec<(&str, J<'_>)> = cols
                        .iter()
                        .zip(&vals)
                        .map(|(c, (v, text))| {
                            let j = if *text {
                                J::S(v.as_str())
                            } else {
                                v.parse::<f64>().map_or(J::Null, J::N)
                            };
                            (*c, j)
                        })
                        .collect();
                    g.feature(&props, geom)?;
                }
                n += 1;
            }
        }
    }
    if let Some(out) = csv {
        out.finish()?;
    }
    if let Some(g) = gj {
        g.finish()?;
    }
    ctx.progress(format!("{} pairs, {n} legs", o.len()));
    Ok(())
}

/// The criteria of `--criteria` (by default all, the fare only with a
/// fare structure).
fn criteria(v: Option<&[String]>, with_fare: bool) -> Res<Vec<Criterion>> {
    let Some(v) = v else {
        return Ok(Criterion::ALL
            .iter()
            .copied()
            .filter(|c| with_fare || *c != Criterion::Fare)
            .collect());
    };
    let mut out = Vec::new();
    for c in v {
        let up = c.trim().to_ascii_uppercase();
        let k = Criterion::from_name(&up).ok_or_else(|| {
            usage(format!(
                "--criteria must be among TIME, FARE, TRANSFERS, WALK, got '{c}'"
            ))
        })?;
        if out.contains(&k) {
            return Err(usage("--criteria has a criterion twice"));
        }
        out.push(k);
    }
    if out.is_empty() {
        return Err(usage("--criteria must name at least one criterion"));
    }
    if out.contains(&Criterion::Fare) && !with_fare {
        return Err(usage("the FARE criterion needs a --fare-structure"));
    }
    Ok(out)
}

pub fn options(a: OptionsArgs) -> Res<()> {
    let ctx = Ctx::new("options", a.net.quiet);
    let pw = pairwise(
        &ctx,
        &a.points,
        a.all_to_all,
        &a.mode,
        &a.mode_egress,
        &a.transit,
        &a.street,
    )?;
    let Pairwise {
        o,
        d,
        m,
        street,
        call,
    } = pw;
    let crit = criteria(a.criteria.as_deref(), a.fares.fare_structure.is_some())?;
    let max_options = count_or_inf("max-options", &a.max_options)?.map_or(0, |n| n as usize);
    let walk_resolution = count("walk-resolution", &a.walk_resolution, 1)?;
    if a.drop_geometry && a.osm_link_ids {
        return Err(usage(
            "--osm-link-ids needs the geometry: not with --drop-geometry",
        ));
    }
    let dir = a
        .output_dir
        .as_deref()
        .map(|p| pair_dir(p, &o, &d))
        .transpose()?;
    let built = build(&ctx, &a.net)?;
    if m.has_transit() {
        check_service(&ctx, &built.net, call.when.date)?;
    }
    let (fares, max_fare) = fares(&a.fares, &built.net)?;
    let scen = scenario(&ctx, &a.scenario, &built.net)?;
    let mut p = TripOptionsParams::default();
    p.scenario = scen;
    p.mode = m.street;
    p.transit_modes = m.transit.clone();
    if m.has_transit() {
        p.egress_mode = m.egress;
    }
    p.date = call.when.date;
    p.departure_time = call.when.time;
    p.time_window = call.time_window;
    p.criteria = crit;
    p.max_walk_time = street.lim.walk;
    p.max_bike_time = street.lim.bike;
    p.max_car_time = street.lim.car;
    p.max_trip_duration = street.lim.trip;
    p.walk_speed = street.walk_speed;
    p.bike_speed = street.bike_speed;
    p.max_rides = call.max_rides;
    p.max_lts = street.max_lts;
    let with_fare = fares.is_some();
    p.fare_structure = fares;
    p.max_fare = max_fare;
    p.max_options = max_options;
    p.walk_resolution = walk_resolution;
    // the legs are made only to be written
    p.itineraries = dir.is_some() || a.legs.is_some();
    p.osm_link_ids = a.osm_link_ids;
    p.n_threads = threads(&a.net);
    p.cancel = cancel_token().clone();
    let res = trip_options(&built.net, &o.coords, &d.coords, &p)?;
    if let Some(dir) = dir {
        let ids: Vec<(String, String)> = o.ids.iter().cloned().zip(d.ids.iter().cloned()).collect();
        let pts: Vec<_> = o
            .coords
            .iter()
            .copied()
            .zip(d.coords.iter().copied())
            .collect();
        write_trip_options_csv(&dir, &ids, &pts, &res, a.drop_geometry, a.osm_link_ids)
            .map_err(|e| runtime(format!("cannot write {dir}: {e}")))?;
        ctx.progress(format!("{} pairs written to {dir}", o.len()));
        return Ok(());
    }
    // the options
    let mut out = Out::open(a.out.output.as_deref())?;
    let mut h = vec!["from_id", "to_id", "option", "total_time"];
    if with_fare {
        h.push("fare");
    }
    h.extend([
        "transfers",
        "walk_time",
        "departure_time",
        "arrival_time",
        "n_departures",
        "first_departure",
        "last_departure",
        "description",
    ]);
    out.row(&h)?;
    let mut n = 0;
    for (k, opts) in res.iter().enumerate() {
        for t in opts {
            let mut rec = vec![
                o.ids[k].clone(),
                d.ids[k].clone(),
                t.option.to_string(),
                num(t.total_time),
            ];
            if with_fare {
                rec.push(num(t.fare.unwrap_or(0.0)));
            }
            rec.extend([
                t.transfers.to_string(),
                num(t.walk_time),
                clock(t.departure_s),
                clock(t.arrival_s),
                t.n_departures().to_string(),
                clock(t.departures[0]),
                clock(*t.departures.last().expect("a departure")),
                t.description.clone(),
            ]);
            out.row(&rec)?;
            n += 1;
        }
    }
    out.finish()?;
    // the departures of each option
    if let Some(path) = &a.departures {
        let mut out = Out::open(Some(path))?;
        out.row(&[
            "from_id",
            "to_id",
            "option",
            "departure_time",
            "arrival_time",
            "total_time",
        ])?;
        for (k, opts) in res.iter().enumerate() {
            for t in opts {
                for ((&dp, &tt), &tm) in t.departures.iter().zip(&t.totals_s).zip(&t.totals) {
                    out.row(&[
                        o.ids[k].clone(),
                        d.ids[k].clone(),
                        t.option.to_string(),
                        clock(dp),
                        clock(dp + tt),
                        num(tm),
                    ])?;
                }
            }
        }
        out.finish()?;
    }
    // the legs of each option
    if let Some(path) = &a.legs {
        let mut cols = vec!["from_id", "to_id", "option", "segment", "mode"];
        if with_fare {
            cols.push("cumulative_fare");
        }
        cols.extend(["segment_duration", "wait", "distance", "route"]);
        if a.osm_link_ids {
            cols.extend(LINKS);
        }
        let out = Out::open(Some(path))?;
        let (mut csv, mut gj) = (None, None);
        if a.format == Format::Geojson {
            gj = Some(GeoJson::start(out)?);
        } else {
            let mut out = out;
            let mut hh = cols.clone();
            if !a.drop_geometry {
                hh.push("geometry");
            }
            out.row(&hh)?;
            csv = Some(out);
        }
        for (k, opts) in res.iter().enumerate() {
            for t in opts {
                let Some(it) = &t.itinerary else { continue };
                for (j, l) in it.legs.iter().enumerate() {
                    let mut vals: Vec<(String, bool)> = vec![
                        (o.ids[k].clone(), true),
                        (d.ids[k].clone(), true),
                        (t.option.to_string(), false),
                        ((j + 1).to_string(), false),
                        (l.mode.clone(), true),
                    ];
                    if with_fare {
                        vals.push((num(l.cumulative_fare.unwrap_or(0.0)), false));
                    }
                    vals.push((num(l.segment_duration), false));
                    vals.push((num(l.wait), false));
                    vals.push((l.distance.to_string(), false));
                    vals.push((l.route.clone(), true));
                    if a.osm_link_ids {
                        vals.extend(leg_links(l).into_iter().map(|s| (s, true)));
                    }
                    let geom = if a.drop_geometry {
                        Geom::None
                    } else {
                        Geom::Line(&l.geometry)
                    };
                    if let Some(out) = csv.as_mut() {
                        let mut rec: Vec<String> = vals.into_iter().map(|v| v.0).collect();
                        if !a.drop_geometry {
                            rec.push(geom.wkt());
                        }
                        out.row(&rec)?;
                    } else if let Some(g) = gj.as_mut() {
                        let props: Vec<(&str, J<'_>)> = cols
                            .iter()
                            .zip(&vals)
                            .map(|(c, (v, text))| {
                                let j = if *text {
                                    J::S(v.as_str())
                                } else {
                                    v.parse::<f64>().map_or(J::Null, J::N)
                                };
                                (*c, j)
                            })
                            .collect();
                        g.feature(&props, geom)?;
                    }
                }
            }
        }
        if let Some(out) = csv {
            out.finish()?;
        }
        if let Some(g) = gj {
            g.finish()?;
        }
    }
    ctx.progress(format!("{} pairs, {n} options", o.len()));
    Ok(())
}

pub fn pareto(a: ParetoArgs) -> Res<()> {
    let ctx = Ctx::new("pareto", a.net.quiet);
    let (o, d) = read_od(
        &ctx,
        &a.points.origins,
        a.points.destinations.as_deref(),
        true,
        &[],
    )?;
    let m = modes(&a.mode, &a.mode_egress)?;
    let when = departure(a.transit.departure.as_deref(), &m)?;
    let mtd = max_trip_duration(&a.street)?;
    let street = street(&a.street, &m, mtd)?;
    let mut call = transit_call(&a.transit, None, when)?;
    call.seed = a.seed.as_deref().map(|s| int("seed", s)).transpose()?;
    // r5r samples time_window departures (one draw a minute)
    call.draws = 1;
    let pcts = percentiles(&a.percentiles)?;
    check_draws(1, call.time_window, m.has_transit())?;
    let mut cutoffs = Vec::new();
    for v in &a.fare_cutoffs {
        cutoffs.push(number("fare-cutoffs", v)?);
    }
    let built = build(&ctx, &a.net)?;
    if m.has_transit() {
        check_service(&ctx, &built.net, when.date)?;
    }
    let fs = fare_structure(&a.fare_structure, Some(&built.net))?;
    let scen = peabiru_core::Scenario::default();
    let mut p = transit_params(&m, &street, &call, pcts, threads(&a.net), &scen);
    if !m.has_transit() {
        // by the street mode alone
        p.egress_mode = m.street;
    }
    p.fare_structure = Some(std::sync::Arc::new(fs));
    let f = pareto_frontier(&built.net, &o.coords, &d.coords, &p, &cutoffs)?;
    let mut out = Out::open(a.out.output.as_deref())?;
    out.row(&[
        "from_id",
        "to_id",
        "percentile",
        "travel_time",
        "monetary_cost",
    ])?;
    for r in &f.rows {
        let tt = if r.travel_time > street.lim.trip {
            String::new()
        } else {
            r.travel_time.to_string()
        };
        out.row(&[
            o.ids[r.origin].clone(),
            d.ids[r.destination].clone(),
            r.percentile.to_string(),
            tt,
            num(r.monetary_cost),
        ])?;
    }
    out.finish()?;
    ctx.progress(format!("{} rows", f.rows.len()));
    Ok(())
}
