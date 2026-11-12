//! `peabiru accessibility` and `peabiru isochrone`.

use crate::app::{AccessibilityArgs, Format, IsochroneArgs};
use crate::common::*;
use peabiru_core::unstable::{isochrone_polygons_in, CxxLibrary};
use peabiru_core::{
    isochrone_lines, isochrone_polygons, street_accessibility, street_edges,
    street_travel_time_surface, transit_accessibility, transit_travel_time_surface,
    AccessibilityParams, CsvOutput, DecayFunction, Destinations,
};

/// r5r's `assign_decay_function()`.
fn decay(name: &str, value: Option<&str>) -> Res<DecayFunction> {
    let name = name.trim();
    let v = value.map(|v| number("decay-value", v)).transpose()?;
    if let Some(x) = v {
        if !x.is_finite() {
            return Err(usage("--decay-value must be finite"));
        }
    }
    let need = || {
        v.ok_or_else(|| {
            usage(format!(
                "--decay-value is required with --decay-function {name}"
            ))
        })
    };
    Ok(match name {
        "step" | "exponential" => {
            if v.is_some() {
                return Err(usage(format!(
                    "--decay-value must not be given with --decay-function {name}"
                )));
            }
            if name == "step" {
                DecayFunction::Step
            } else {
                DecayFunction::Exponential
            }
        }
        "fixed_exponential" => {
            let x = need()?;
            if x <= 0.0 || x >= 1.0 {
                return Err(usage(
                    "--decay-value must be between 0 and 1 (exclusive) with fixed_exponential",
                ));
            }
            DecayFunction::FixedExponential { decay_constant: x }
        }
        "linear" | "logistic" => {
            let x = need()?;
            if x < 1.0 {
                return Err(usage(format!(
                    "--decay-value must be at least 1 with {name}"
                )));
            }
            if name == "linear" {
                if x.fract() != 0.0 || x > 59.0 {
                    return Err(usage(
                        "--decay-value must be a whole number from 1 to 59 with linear",
                    ));
                }
                DecayFunction::Linear {
                    width_minutes: x as i32,
                }
            } else {
                DecayFunction::Logistic {
                    standard_deviation_minutes: x,
                }
            }
        }
        _ => {
            return Err(usage(format!(
                "--decay-function must be step, exponential, fixed_exponential, linear or \
                 logistic, got '{name}'"
            )))
        }
    })
}

/// r5r's `set_cutoffs()`: the cutoffs R5 receives (ascending, or the
/// placeholder 0 for fixed_exponential).
fn cutoffs(values: Option<&[String]>, fixed: bool) -> Res<Option<Vec<i32>>> {
    let Some(values) = values else {
        if !fixed {
            return Err(usage("--cutoffs is required with this --decay-function"));
        }
        return Ok(None);
    };
    if fixed {
        return Err(usage("--cutoffs must not be given with fixed_exponential"));
    }
    let mut c = Vec::new();
    for v in values {
        let x = count("cutoffs", v, 1)?;
        if x > 120 {
            return Err(usage(format!("--cutoffs must be from 1 to 120, got '{v}'")));
        }
        c.push(x);
    }
    let n = c.len();
    c.sort_unstable();
    c.dedup();
    if n == 0 || n > 12 || c.len() != n {
        return Err(usage("--cutoffs must be 1 to 12 distinct values"));
    }
    Ok(Some(c))
}

pub fn accessibility(a: AccessibilityArgs) -> Res<()> {
    let ctx = Ctx::new("accessibility", a.net.quiet);
    let (o, d) = read_od(
        &ctx,
        &a.points.origins,
        a.points.destinations.as_deref(),
        true,
        &a.opportunities,
    )?;
    let walk = ["WALK".to_string()];
    let m = modes(mode_list(&a.mode, &walk), &a.mode.mode_egress)?;
    let when = departure(a.transit.departure.as_deref(), &m)?;
    let f = decay(&a.decay_function, a.decay_value.as_deref())?;
    let fixed = matches!(f, DecayFunction::FixedExponential { .. });
    let cuts = cutoffs(a.cutoffs.as_deref(), fixed)?;
    let mut mtd = max_trip_duration(&a.street)?;
    if let Some(c) = &cuts {
        let top = *c.last().expect("a cutoff");
        if mtd < top {
            return Err(usage(
                "--max-trip-duration cannot be shorter than the largest of --cutoffs",
            ));
        }
        // only the step function ignores trips longer than the cutoffs
        if matches!(f, DecayFunction::Step) {
            mtd = top;
        }
    }
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
    let th = threads(&a.net);
    let mut ap = AccessibilityParams::default();
    // r5r passes the placeholder 0 when cutoffs is NULL
    ap.cutoffs = cuts.clone().unwrap_or_else(|| vec![0]);
    ap.decay = f;
    let dest = Destinations::new(&d.coords, &d.columns);
    let mut tp = transit_params(&m, &street, &call, pcts.clone(), th, &scen);
    tp.fare_structure = fares;
    tp.max_fare = max_fare;
    let rp = route_params(&street, street.lim.trip, th, &scen);
    let route = |o: &[(f64, f64)]| {
        if m.has_transit() {
            transit_accessibility(&built.net, o, &dest, &tp, &ap)
        } else {
            street_accessibility(&built.net, o, &dest, m.street, &rp, &pcts, &ap)
        }
    };
    if let Some(dir) = dir {
        let w = CsvOutput::new(&dir, &o.ids, &[], th);
        let per_origin = a.opportunities.len() * pcts.len() * ap.cutoffs.len();
        for r in w.origin_chunks(per_origin) {
            let acc = route(&o.coords[r.clone()])?;
            w.write_accessibility(r.start, &acc, &a.opportunities)
                .map_err(|e| runtime(format!("cannot write {dir}: {e}")))?;
        }
        ctx.progress(format!("{} origins written to {dir}", o.len()));
        return Ok(());
    }
    let acc = route(&o.coords)?;
    let mut out = Out::open(a.out.output.as_deref())?;
    if fixed {
        out.row(&["id", "opportunity", "percentile", "accessibility"])?;
    } else {
        out.row(&["id", "opportunity", "percentile", "cutoff", "accessibility"])?;
    }
    for (i, id) in o.ids.iter().enumerate() {
        for (k, name) in a.opportunities.iter().enumerate() {
            for (p, pc) in acc.percentiles().iter().enumerate() {
                for (c, cut) in acc.cutoffs().iter().enumerate() {
                    let v = num(acc.get(i, k, p, c));
                    if fixed {
                        out.row(&[id.as_str(), name, &pc.to_string(), &v])?;
                    } else {
                        out.row(&[id.as_str(), name, &pc.to_string(), &cut.to_string(), &v])?;
                    }
                }
            }
        }
    }
    out.finish()?;
    ctx.progress(format!("{} origins, {} rows", o.len(), acc.values().len()));
    Ok(())
}

pub fn isochrone(a: IsochroneArgs) -> Res<()> {
    let ctx = Ctx::new("isochrone", a.net.quiet);
    let o = read_points(&ctx, &a.origins, "origins", true, &[])?;
    if o.len() == 0 {
        return Err(runtime(format!(
            "{}: the origins have no rows",
            a.origins.display()
        )));
    }
    let mut values = Vec::new();
    for v in &a.cutoffs {
        let x = number("cutoffs", v)?;
        if !x.is_finite() || x < 0.0 {
            return Err(usage(format!(
                "--cutoffs must be finite numbers >= 0, got '{v}'"
            )));
        }
        values.push(x);
    }
    if values.is_empty() {
        return Err(usage("--cutoffs must hold at least one value"));
    }
    let top = values.iter().copied().fold(f64::MIN, f64::max);
    if top > f64::from(i32::MAX) {
        return Err(usage(
            "the largest of --cutoffs is beyond the integer range",
        ));
    }
    // r5r: the largest cutoff is the trip limit (polygons route 10 minutes
    // past it), the bands start at 0
    let limit = top as i32;
    let mut cuts = values.clone();
    cuts.push(0.0);
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    if cuts.len() < 2 {
        return Err(usage(
            "--cutoffs must hold at least one value greater than 0",
        ));
    }
    let m = modes(&a.mode, &a.mode_egress)?;
    let when = departure(a.transit.departure.as_deref(), &m)?;
    let polygons = !a.lines;
    let zoom = count("zoom", &a.zoom, 0)?;
    if polygons && !(9..=12).contains(&zoom) {
        return Err(usage(format!(
            "--zoom must be from 9 to 12, got '{}'",
            a.zoom
        )));
    }
    if !polygons && a.percentiles.is_some() {
        return Err(usage(
            "percentiles are not supported for line-based isochrones",
        ));
    }
    let pcts = percentiles(a.percentiles.as_deref().unwrap_or(&["50".to_string()]))?;
    let mtd = if polygons { limit + 10 } else { limit };
    if mtd < 1 {
        return Err(usage("the largest of --cutoffs must be at least 1"));
    }
    let street = street(&a.street, &m, mtd)?;
    let call = transit_call(&a.transit, Some(&a.draws), when)?;
    let cxx = match a.cxx_library.as_deref() {
        None => CxxLibrary::NATIVE,
        Some("libc++") => CxxLibrary::Libcxx,
        Some("libstdc++") => CxxLibrary::Libstdcxx,
        Some(other) => {
            return Err(usage(format!(
                "--cxx-library must be libc++ or libstdc++, got '{other}'"
            )))
        }
    };
    let built = build(&ctx, &a.net)?;
    let th = threads(&a.net);
    let scen = peabiru_core::Scenario::default();
    let mut out = Out::open(a.out.output.as_deref())?;
    if polygons {
        let surface = if m.has_transit() {
            let p = transit_params(&m, &street, &call, pcts.clone(), th, &scen);
            transit_travel_time_surface(&built.net, &o.coords, &p, zoom)?
        } else {
            let p = route_params(&street, street.lim.trip, th, &scen);
            street_travel_time_surface(&built.net, &o.coords, m.street, &p, &pcts, zoom)?
        };
        let rows = if a.cxx_library.is_some() {
            isochrone_polygons_in(&surface, &cuts, cxx)?
        } else {
            isochrone_polygons(&surface, &cuts)?
        };
        match a.format {
            Format::Geojson => {
                let mut g = GeoJson::start(out)?;
                for r in &rows {
                    let pc = format!("p{}", r.percentile);
                    g.feature(
                        &[
                            ("id", J::S(&o.ids[r.origin])),
                            ("isochrone", J::N(r.isochrone)),
                            ("percentile", J::S(&pc)),
                        ],
                        Geom::MultiPolygon(&r.polygons),
                    )?;
                }
                g.finish()?;
            }
            Format::Csv => {
                out.row(&["id", "isochrone", "percentile", "geometry"])?;
                for r in &rows {
                    out.row(&[
                        o.ids[r.origin].clone(),
                        num(r.isochrone),
                        format!("p{}", r.percentile),
                        Geom::MultiPolygon(&r.polygons).wkt(),
                    ])?;
                }
                out.finish()?;
            }
        }
        ctx.progress(format!("{} polygons", rows.len()));
        return Ok(());
    }
    // lines: the matrix to the centroids of the street edges, as r5r's
    // travel_time_matrix() gives it
    let edges = street_edges(&built.net);
    let dest = edges.centroid_points();
    let c = crate::matrix::Call {
        net: &built.net,
        modes: &m,
        street: &street,
        transit: &call,
        pcts: vec![50],
        threads: th,
        scenario: &scen,
        engine: peabiru_core::StreetEngine::Auto,
        fares: None,
        max_fare: f64::INFINITY,
    };
    if m.has_transit() {
        check_service(&ctx, &built.net, when.date)?;
    }
    let rows = c.rows(&o.coords, &dest)?;
    let ne = edges.len();
    let mut minutes = vec![None; o.len() * ne];
    for k in 0..rows.from.len() {
        minutes[rows.from[k] * ne + rows.to[k]] = rows.times[k][0];
    }
    let lines = isochrone_lines(o.len(), ne, &minutes, &cuts)?;
    let b = |x: bool| if x { "true" } else { "false" };
    match a.format {
        Format::Geojson => {
            let mut g = GeoJson::start(out)?;
            for l in &lines {
                let e = l.edge;
                g.feature(
                    &[
                        ("id", J::S(&o.ids[l.origin])),
                        ("edge_index", J::S(&edges.edge_index(e).to_string())),
                        ("osm_id", J::I(edges.osm_id(e))),
                        ("isochrone", J::N(l.isochrone)),
                        ("travel_time_p50", J::I(i64::from(l.travel_time))),
                        ("from_vertex", J::I(i64::from(edges.from_vertex(e)))),
                        ("to_vertex", J::I(i64::from(edges.to_vertex(e)))),
                        ("street_class", J::S(edges.street_class(e))),
                        ("length", J::N(edges.length_m(e))),
                        ("walk", J::B(edges.walk(e))),
                        ("car", J::B(edges.car(e))),
                        ("car_speed", J::N(edges.car_speed_kph(e))),
                        ("bicycle", J::B(edges.bicycle(e))),
                        ("bicycle_lts", J::I(i64::from(edges.bicycle_lts(e)))),
                    ],
                    Geom::Line(edges.geometry(e)),
                )?;
            }
            g.finish()?;
        }
        Format::Csv => {
            out.row(&[
                "id",
                "edge_index",
                "osm_id",
                "isochrone",
                "travel_time_p50",
                "from_vertex",
                "to_vertex",
                "street_class",
                "length",
                "walk",
                "car",
                "car_speed",
                "bicycle",
                "bicycle_lts",
                "geometry",
            ])?;
            for l in &lines {
                let e = l.edge;
                out.row(&[
                    o.ids[l.origin].clone(),
                    edges.edge_index(e).to_string(),
                    edges.osm_id(e).to_string(),
                    num(l.isochrone),
                    l.travel_time.to_string(),
                    edges.from_vertex(e).to_string(),
                    edges.to_vertex(e).to_string(),
                    edges.street_class(e).to_string(),
                    num(edges.length_m(e)),
                    b(edges.walk(e)).into(),
                    b(edges.car(e)).into(),
                    num(edges.car_speed_kph(e)),
                    b(edges.bicycle(e)).into(),
                    edges.bicycle_lts(e).to_string(),
                    Geom::Line(edges.geometry(e)).wkt(),
                ])?;
            }
            out.finish()?;
        }
    }
    ctx.progress(format!("{} street segments", lines.len()));
    Ok(())
}
