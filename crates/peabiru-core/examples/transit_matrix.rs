//! Walk + public transport travel-time matrix, as r5r's
//! `travel_time_matrix(mode = c("WALK", "TRANSIT"))`.
//!
//! ```sh
//! cargo run --release -p peabiru-core --example transit_matrix [network_dir]
//! ```

mod common;

use peabiru_core::{
    build_from_dir, transit_travel_time_matrix, BuildOptions, TransitData, TransitParams,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = common::data_dir();
    let points = common::read_points(&dir, &[]);

    // One .pbf plus every GTFS .zip of the directory, as r5r's build_network().
    let mut opts = BuildOptions::default();
    opts.n_threads = 2;
    let (net, layer) = build_from_dir(&dir, &opts)?;
    // Transfers and egress distances, computed once and shared by every query.
    let data = TransitData::new(&net, &layer);
    println!("{} stops, {} patterns", layer.n_stops(), layer.n_patterns());

    // Monday 2019-05-13, departures from 14:00 (seconds after midnight)
    // over 10 minutes; a seed fixes the draws of the frequency-based feed;
    // r5r's defaults for everything else.
    let mut params = TransitParams::default();
    params.date = 20190513;
    params.departure_time = 14 * 3600;
    params.time_window = 10;
    params.percentiles = vec![25, 50, 75];
    params.seed = Some(42);
    params.n_threads = 2;
    // Whole minutes at each percentile; None = no time.
    let m =
        transit_travel_time_matrix(&net, &layer, &data, &points.coords, &points.coords, &params)?;
    for j in 0..5 {
        println!(
            "{} -> {}: {:?} min at percentiles {:?}",
            points.ids[0],
            points.ids[j],
            m.get(0, j),
            m.percentiles()
        );
    }
    Ok(())
}
