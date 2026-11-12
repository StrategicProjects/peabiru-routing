//! Looking into a network, as r5r's `find_snap()`, `street_network_bbox()`,
//! `street_network_to_sf()`, `transit_network_to_sf()`,
//! `check_transit_availability()` and `gtfs_errors()` do.
//!
//! ```sh
//! cargo run --release -p peabiru-core --example inspect [network_dir]
//! ```

mod common;

use peabiru_core::{
    build_from_dir, find_snap, gtfs_errors, gtfs_files_in_dir, street_edges, street_vertices,
    transit_routes, transit_service_dates, transit_stops, BuildOptions, Mode,
};
use std::collections::BTreeMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = common::data_dir();
    let points = common::read_points(&dir, &[]);
    let mut opts = BuildOptions::default();
    opts.n_threads = 2;
    // As r5r's build_network(), the build fails when a feed has GTFS errors
    // of priority HIGH; ignore_gtfs_errors builds what can be read anyway.
    let (net, layer) = match build_from_dir(&dir, &opts) {
        Ok(built) => built,
        Err(e) if e.high_priority_gtfs_errors().is_some() => {
            println!("refused: {e}");
            opts.ignore_gtfs_errors = true;
            let built = build_from_dir(&dir, &opts)?;
            for w in built.1.r5r_warnings() {
                println!("built anyway: {w}");
            }
            built
        }
        Err(e) => return Err(e.into()),
    };

    // Where the first three points meet the streets open to walking
    // (r5r's find_snap(), radius 1600 m).
    for (id, snap) in
        points
            .ids
            .iter()
            .zip(find_snap(&net, &points.coords[..3], 1600.0, Mode::Walk)?)
    {
        match snap {
            Some(s) => println!(
                "point {id} snaps to ({:.6}, {:.6}), {:.1} m away",
                s.lat, s.lon, s.distance_m
            ),
            None => println!("point {id} is not near a street"),
        }
    }

    // The box of the street network, [xmin, ymin, xmax, ymax].
    let b = net.bbox();
    println!(
        "bbox: lon {:.4} to {:.4}, lat {:.4} to {:.4}",
        b[0], b[2], b[1], b[3]
    );

    // The street network as r5r's street_network_to_sf() lists it.
    let vertices = street_vertices(&net);
    let edges = street_edges(&net);
    let km: f64 = (0..edges.len()).map(|i| edges.length_m(i)).sum::<f64>() / 1000.0;
    let car = (0..edges.len()).filter(|&i| edges.car(i)).count();
    println!(
        "streets: {} vertices, {} directed edges ({car} open to cars), {km:.1} km",
        vertices.index.len(),
        edges.len()
    );

    // Public transport: stops, one route row per trip pattern, service dates.
    let stops = transit_stops(&net, &layer);
    let linked = stops.linked_to_street.iter().filter(|&&l| l).count();
    let routes = transit_routes(&net, &layer)?;
    let mut by_mode: BTreeMap<&str, usize> = BTreeMap::new();
    for m in &routes.mode {
        *by_mode.entry(*m).or_default() += 1;
    }
    println!(
        "transit: {} stops ({linked} linked to the streets), {} patterns {by_mode:?}",
        stops.stop_index.len(),
        routes.len()
    );
    let dates = transit_service_dates(&layer)?;
    println!(
        "service from {} to {} ({} dates); on 2019-05-13, {} of {} services run",
        dates[0],
        dates[dates.len() - 1],
        dates.len(),
        layer.n_services_active_on(20190513)?,
        layer.n_services()
    );

    // The errors gtfs-lib records loading each feed, by priority and type.
    // Errors of priority HIGH make build_from_dir() fail, as r5r's
    // build_network() does (BuildOptions::ignore_gtfs_errors to build anyway).
    for path in gtfs_files_in_dir(&dir)? {
        let bytes = std::fs::read(&path)?;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let mut counts: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for e in gtfs_errors(&name, &bytes)? {
            *counts
                .entry((e.priority().as_str(), e.error_type()))
                .or_default() += 1;
        }
        println!("{name}: {counts:?}");
    }
    Ok(())
}
