//! Access to jobs by walking and public transport, with a logistic decay.
//!
//! ```sh
//! cargo run --release -p peabiru-core --example accessibility [network_dir]
//! ```

mod common;

use peabiru_core::{
    accessibility_trip_limits, build_from_dir, transit_accessibility, AccessibilityParams,
    BuildOptions, DecayFunction, Destinations, TransitParams,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = common::data_dir();
    // The points file has one column per kind of opportunity.
    let points = common::read_points(&dir, &["jobs"]);
    let mut opts = BuildOptions::default();
    opts.n_threads = 2;
    let net = build_from_dir(&dir, &opts)?;

    let decay = DecayFunction::Logistic {
        standard_deviation_minutes: 5.0,
    };
    let cutoffs = vec![15, 30];
    // The limits that follow from max_trip_duration = 120, max_walk_time = Inf.
    let (max_trip, max_walk) = accessibility_trip_limits(&decay, Some(&cutoffs), 120, None, false)?;
    let mut params = TransitParams::default();
    params.date = 20190513;
    params.departure_time = 14 * 3600;
    params.max_trip_duration = max_trip;
    params.max_walk_time = max_walk;
    params.seed = Some(42);
    params.n_threads = 2;
    let destinations = Destinations::new(&points.coords, &points.columns);
    let mut a = AccessibilityParams::default();
    a.cutoffs = cutoffs;
    a.decay = decay;
    let acc = transit_accessibility(&net, &points.coords, &destinations, &params, &a)?;
    // One value per origin, opportunity column, percentile and cutoff.
    for (i, id) in points.ids.iter().enumerate().take(5) {
        println!(
            "{id}: {:.1} (cutoff 15 min), {:.1} (cutoff 30 min) weighted jobs",
            acc.get(i, 0, 0, 0),
            acc.get(i, 0, 0, 1)
        );
    }
    Ok(())
}
