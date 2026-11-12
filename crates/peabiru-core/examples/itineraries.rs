//! Detailed itineraries: the trip
//! between each origin and the destination of the same index, leg by leg.
//!
//! ```sh
//! cargo run --release -p peabiru-core --example itineraries [network_dir]
//! ```

mod common;

use peabiru_core::{
    build_from_dir, detailed_itineraries, BuildOptions, ItineraryParams, Mode, TransitMode,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = common::data_dir();
    let points = common::read_points(&dir, &[]);
    let mut opts = BuildOptions::default();
    opts.n_threads = 2;
    let net = build_from_dir(&dir, &opts)?;

    // Two pairs: the first point to the 10th, the 2nd to the 13th.
    let origins = [points.coords[0], points.coords[1]];
    let destinations = [points.coords[9], points.coords[12]];

    // Street-only itineraries: no transit modes, one street mode.
    for mode in [Mode::Walk, Mode::Bicycle, Mode::Car] {
        let mut p = ItineraryParams::default();
        p.mode = mode;
        p.n_threads = 2;
        let per_pair = detailed_itineraries(&net, &origins, &destinations, &p)?;
        for (k, its) in per_pair.iter().enumerate() {
            for it in its {
                let leg = &it.legs[0];
                println!(
                    "pair {k} {:<7} {:>4.1} min {:>5} m, {} points in the line",
                    leg.mode,
                    it.total_duration,
                    it.total_distance,
                    leg.geometry.len()
                );
            }
        }
    }

    // With public transport and a date. Networks with frequency-based
    // trips are refused here: on the São Paulo
    // sample (a frequency-based feed) this prints the error; on a
    // timetable-based network it prints the legs.
    let mut p = ItineraryParams::default();
    p.transit_modes = TransitMode::ALL.to_vec();
    p.date = 20190513;
    p.departure_time = 14 * 3600;
    p.n_threads = 2;
    match detailed_itineraries(&net, &origins, &destinations, &p) {
        Ok(per_pair) => {
            for (k, its) in per_pair.iter().enumerate() {
                for it in its {
                    println!(
                        "pair {k} option {} at {}: {:.1} min",
                        it.option, it.departure_time, it.total_duration
                    );
                    for leg in &it.legs {
                        println!(
                            "  {:<6} {:>5.1} min (wait {:.1}) {:>5} m {}",
                            leg.mode, leg.segment_duration, leg.wait, leg.distance, leg.route
                        );
                    }
                }
            }
        }
        Err(e) => println!("with public transport: {e}"),
    }

    // The alternatives (McRAPTOR): one itinerary per sequence of routes,
    // up to 10 minutes slower than the best.
    p.shortest_path = false;
    p.suboptimal_minutes = 10;
    if let Ok(per_pair) = detailed_itineraries(&net, &origins, &destinations, &p) {
        for (k, its) in per_pair.iter().enumerate() {
            println!("pair {k}: {} alternatives", its.len());
            for it in its.iter().take(3) {
                let routes: Vec<&str> = it
                    .legs
                    .iter()
                    .filter(|l| !l.route.is_empty())
                    .map(|l| l.route.as_str())
                    .collect();
                println!(
                    "  option {} at {}: {:.1} min, routes {routes:?}",
                    it.option, it.departure_time, it.total_duration
                );
            }
        }
    }
    Ok(())
}
