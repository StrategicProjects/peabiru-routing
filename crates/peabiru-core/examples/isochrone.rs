//! Isochrones by mode from one point, as r5r's `isochrone()`: a travel-time
//! surface on a Web Mercator grid, then polygons for each cutoff.
//!
//! ```sh
//! cargo run --release -p peabiru-core --example isochrone [network_dir]
//! ```

mod common;

use peabiru_core::{
    build_from_dir, isochrone_polygons, isochrone_trip_limit, street_travel_time_surface,
    transit_travel_time_surface, BuildOptions, Mode, Polygon, RouteParams, TransitParams,
};

/// Area of polygons (shells minus holes) in km², on a local flat
/// approximation; the coordinates are `[lon, lat]`.
fn area_km2(polygons: &[Polygon]) -> f64 {
    let ring = |r: &Vec<[f64; 2]>| {
        let kx = r[0][1].to_radians().cos() * 111.32;
        let a: f64 = r
            .windows(2)
            .map(|w| (w[0][0] * kx) * (w[1][1] * 110.57) - (w[1][0] * kx) * (w[0][1] * 110.57))
            .sum();
        (a / 2.0).abs()
    };
    polygons
        .iter()
        .map(|p| ring(&p[0]) - p[1..].iter().map(ring).sum::<f64>())
        .sum()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = common::data_dir();
    let points = common::read_points(&dir, &[]);
    let origin = &points.coords[..1];

    let mut opts = BuildOptions::default();
    opts.n_threads = 2;
    let net = build_from_dir(&dir, &opts)?;

    let cutoffs = [5.0, 10.0, 15.0];
    // the Web Mercator zoom of the grid, 9 to 12
    let zoom = 12;
    // As r5r: polygons route up to the largest cutoff plus 10 minutes.
    let limit = isochrone_trip_limit(&cutoffs)? + 10;

    // Streets: one surface per mode, percentiles [50] (all the same here).
    let mut params = RouteParams::default();
    params.max_trip_duration_min = limit;
    params.n_threads = 2;
    for mode in [Mode::Walk, Mode::Bicycle, Mode::Car] {
        let surface = street_travel_time_surface(&net, origin, mode, &params, &[50], zoom)?;
        report(
            &format!("{mode:?}"),
            &isochrone_polygons(&surface, &cutoffs)?,
        );
    }

    // Walk + public transport: Monday 2019-05-13, departures from 14:00 over
    // 30 minutes, the median; a seed fixes the frequency-based draws.
    let mut p = TransitParams::default();
    p.date = 20190513;
    p.departure_time = 14 * 3600;
    p.time_window = 30;
    p.max_trip_duration = limit;
    p.seed = Some(42);
    p.n_threads = 2;
    let surface = transit_travel_time_surface(&net, origin, &p, zoom)?;
    report("Transit", &isochrone_polygons(&surface, &cutoffs)?);
    Ok(())
}

/// One line per mode: the area within each cutoff, smallest first.
fn report(mode: &str, isochrones: &[peabiru_core::IsochronePolygon]) {
    let areas: Vec<String> = isochrones
        .iter()
        .rev() // largest cutoff first, as r5r
        .map(|i| format!("{} min {:.2}", i.isochrone, area_km2(&i.polygons)))
        .collect();
    println!("{mode:<8} {} km²", areas.join(" · "));
}
