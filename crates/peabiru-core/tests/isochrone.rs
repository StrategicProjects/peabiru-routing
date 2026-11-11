//! Isochrones on the São Paulo sample: the surfaces agree with the
//! travel-time matrices, the polygons are nested, the lines are banded.
//! Parity with r5r is checked by bench/compare/isochrone_parity.R.

use peabiru_core::{
    build_from_dir, isochrone_lines, isochrone_polygons, street_edges, street_travel_time_matrix,
    street_travel_time_surface, transit_travel_time_surface, BuildOptions, Mode, RouteParams,
    TransitData, TransitParams, WebMercatorGrid,
};

const SPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../r/inst/extdata/spo");
const ORIGIN: (f64, f64) = (-23.5563, -46.6290);

/// Twice the signed area of a closed ring, in squared degrees.
fn ring_area(r: &[[f64; 2]]) -> f64 {
    r.windows(2)
        .map(|w| w[0][0] * w[1][1] - w[1][0] * w[0][1])
        .sum::<f64>()
        .abs()
}

#[test]
fn street_surface_is_the_matrix_to_the_cell_centres() {
    let (net, _) = build_from_dir(SPO, &BuildOptions::default()).unwrap();
    let mut p = RouteParams::default();
    p.max_trip_duration_min = 25;
    p.n_threads = 2;
    let s = street_travel_time_surface(&net, &[ORIGIN], Mode::Walk, &p, &[75, 25], 11).unwrap();
    let g = *s.grid();
    assert_eq!(g, WebMercatorGrid::for_network(&net, 11).unwrap());
    assert_eq!(s.percentiles(), &[25, 75]);
    // one unvarying time, the same at every percentile
    assert_eq!(s.minutes(0, 0), s.minutes(0, 1));
    let m = street_travel_time_matrix(&net, &[ORIGIN], &g.cell_centers(), Mode::Walk, &p).unwrap();
    let mut reached = 0;
    for (cell, &minutes) in s.minutes(0, 0).iter().enumerate() {
        // the matrix drops times beyond the limit, the surface keeps them
        match m.get(0, cell) {
            Some(t) => {
                assert_eq!(minutes, t as i32);
                reached += 1;
            }
            None => assert!(minutes == i32::MAX || minutes > 25),
        }
    }
    assert!(reached > 50, "{reached} cells reached");
}

#[test]
fn polygons_grow_with_the_cutoff() {
    let (net, layer) = build_from_dir(SPO, &BuildOptions::default()).unwrap();
    let data = TransitData::new(&net, &layer);
    let mut p = TransitParams::default();
    p.date = 20190513;
    p.departure_time = 14 * 3600;
    p.seed = Some(42);
    p.max_trip_duration = 40;
    p.n_threads = 2;
    let far = (-23.0, -46.0);
    let s = transit_travel_time_surface(&net, &layer, &data, &[ORIGIN, far], &p, 10).unwrap();
    let rows = isochrone_polygons(&s, &[0.0, 30.0, 10.0, 20.0]).unwrap();
    assert_eq!(rows.len(), 6);
    let cut: Vec<(usize, f64)> = rows.iter().map(|r| (r.origin, r.isochrone)).collect();
    assert_eq!(
        cut,
        vec![
            (0, 30.0),
            (0, 20.0),
            (0, 10.0),
            (1, 30.0),
            (1, 20.0),
            (1, 10.0)
        ]
    );
    let area = |k: usize| -> f64 {
        rows[k]
            .polygons
            .iter()
            .map(|poly| ring_area(&poly[0]) - poly[1..].iter().map(|h| ring_area(h)).sum::<f64>())
            .sum()
    };
    assert!(area(0) > area(1) && area(1) > area(2) && area(2) > 0.0);
    for r in &rows[..3] {
        for ring in r.polygons.iter().flatten() {
            assert_eq!(ring.first(), ring.last(), "rings are closed");
            assert!(ring.len() >= 4);
        }
    }
    // an origin far from the streets reaches nothing
    assert!(rows[3..].iter().all(|r| r.polygons.is_empty()));
}

#[test]
fn lines_are_the_edges_reached_banded() {
    let (net, _) = build_from_dir(SPO, &BuildOptions::default()).unwrap();
    let edges = street_edges(&net);
    assert!(!edges.is_empty() && edges.len() <= net.n_edges());
    for i in 0..edges.len() {
        let g = edges.geometry(i);
        assert!(g.len() >= 2);
        if let Some((lat, lon)) = edges.centroid(i) {
            assert!((-24.0..-23.0).contains(&lat) && (-47.0..-46.0).contains(&lon));
        }
    }
    let mut p = RouteParams::default();
    p.max_trip_duration_min = 12;
    let dest = edges.centroid_points();
    let m = street_travel_time_matrix(&net, &[ORIGIN], &dest, Mode::Walk, &p).unwrap();
    let lines = isochrone_lines(1, edges.len(), m.as_slice(), &[0.0, 6.0, 12.0]).unwrap();
    assert!(!lines.is_empty());
    let reached = m
        .as_slice()
        .iter()
        .filter(|t| matches!(t, Some(t) if *t > 0))
        .count();
    assert_eq!(lines.len(), reached);
    for w in lines.windows(2) {
        let (a, b) = (w[0], w[1]);
        assert!(
            a.isochrone > b.isochrone
                || (a.isochrone == b.isochrone && a.travel_time >= b.travel_time)
        );
    }
    for l in &lines {
        let band = if l.travel_time <= 6 { 6.0 } else { 12.0 };
        assert_eq!(l.isochrone, band);
    }
}

/// A surface call: street mode (`None` for public transport), zoom,
/// threads, and the origins.
type Call = (Option<Mode>, i32, usize, &'static [(f64, f64)]);

fn surface_of(
    net: &peabiru_core::StreetNetwork,
    layer: &peabiru_core::TransitLayer,
    data: &TransitData,
    (mode, zoom, threads, origins): Call,
) -> peabiru_core::TravelTimeSurface {
    match mode {
        Some(m) => {
            let mut p = RouteParams::default();
            p.max_trip_duration_min = 30;
            p.n_threads = threads;
            if m == Mode::Bicycle {
                p.max_lts = 3;
            }
            street_travel_time_surface(net, origins, m, &p, &[50], zoom).unwrap()
        }
        None => {
            let mut p = TransitParams::default();
            p.date = 20190513;
            p.departure_time = 14 * 3600;
            p.seed = Some(42);
            p.max_trip_duration = 30;
            p.percentiles = vec![50, 10];
            p.n_threads = threads;
            transit_travel_time_surface(net, layer, data, origins, &p, zoom).unwrap()
        }
    }
}

/// Surfaces called one after another on one network, in mixed order and
/// over more grids and modes than the network keeps linkages for, are
/// those of a network that never routed before.
#[test]
fn repeated_surfaces_are_those_of_a_fresh_network() {
    const ONE: &[(f64, f64)] = &[ORIGIN];
    const TWO: &[(f64, f64)] = &[(-23.5577, -46.6321), (-23.0, -46.0)];
    let modes = [None, Some(Mode::Walk), Some(Mode::Car), Some(Mode::Bicycle)];
    // every mode at every zoom, interleaved: 12 linkages by street mode
    // (public transport walks), more than the 9 kept
    let mut calls: Vec<Call> = Vec::new();
    for round in 0..3 {
        for (k, z) in [12, 9, 11, 10].into_iter().enumerate() {
            for (j, &m) in modes.iter().enumerate() {
                let threads = if (round + j + k) % 2 == 0 { 1 } else { 4 };
                let origins = if (round + j) % 3 == 0 { TWO } else { ONE };
                calls.push((m, z, threads, origins));
            }
        }
        calls.rotate_left(5);
    }
    let fresh = || build_from_dir(SPO, &BuildOptions::default()).unwrap();
    let (net, layer) = fresh();
    let data = TransitData::new(&net, &layer);
    let mut want = Vec::new();
    for &(m, z, _, origins) in &calls {
        let key = (m, z, origins.len());
        if want.iter().any(|(k, _)| *k == key) {
            continue;
        }
        let (net, layer) = fresh();
        let data = TransitData::new(&net, &layer);
        want.push((key, surface_of(&net, &layer, &data, (m, z, 2, origins))));
    }
    // and on a clone, which shares what the network kept
    let copy = net.clone();
    for (i, &call) in calls.iter().enumerate() {
        let net = if i % 7 == 6 { &copy } else { &net };
        let got = surface_of(net, &layer, &data, call);
        let (m, z, threads, origins) = call;
        let key = (m, z, origins.len());
        let w = &want.iter().find(|(k, _)| *k == key).unwrap().1;
        assert!(
            &got == w,
            "call {i}: {m:?} at zoom {z}, {threads} threads, {} origins",
            origins.len()
        );
    }
}
