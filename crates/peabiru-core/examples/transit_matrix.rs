//! Walk + public transport travel-time matrix, as r5r's
//! `travel_time_matrix(mode = c("WALK", "TRANSIT"))`.
//!
//! ```sh
//! cargo run --release -p peabiru-core --example transit_matrix [network_dir]
//! ```

mod common;

use peabiru_core::street::BuildOptions;
use peabiru_core::transit::build_from_dir;
use peabiru_core::transit::matrix::{transit_travel_time_matrix, TransitData, TransitParams};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = common::data_dir();
    let points = common::read_points(&dir, &[]);

    // One .pbf plus every GTFS .zip of the directory, as r5r's build_network().
    let (net, layer) = build_from_dir(&dir, &BuildOptions { n_threads: 2 })?;
    // Transfers and egress distances, computed once and shared by every query.
    let data = TransitData::new(&net, &layer);
    println!(
        "{} stops, {} patterns",
        layer.stop_ids.len(),
        layer.patterns.len()
    );

    // Monday 2019-05-13, departures from 14:00 (seconds after midnight)
    // over 10 minutes; a seed fixes the draws of the frequency-based feed;
    // r5r's defaults for everything else.
    let params = TransitParams {
        date: 20190513,
        departure_time: 14 * 3600,
        time_window: 10,
        percentiles: vec![25, 50, 75],
        seed: Some(42),
        n_threads: 2,
        ..TransitParams::default()
    };
    // [origin][destination][percentile], whole minutes; None = no time.
    let m =
        transit_travel_time_matrix(&net, &layer, &data, &points.coords, &points.coords, &params)?;
    for j in 0..5 {
        println!("{} -> {}: {:?}", points.ids[0], points.ids[j], m.get(0, j));
    }
    Ok(())
}
