//! Every trip of the departure window
//! (`expanded_travel_time_matrix()`), and the latest trip that arrives in
//! time (`arrival_travel_time_matrix()`).
//!
//! ```sh
//! cargo run --release -p peabiru-core --example expanded [network_dir]
//! ```

mod common;

use peabiru_core::{
    arrival_departure, arrival_travel_time_matrix, build_from_dir, expanded_travel_time_matrix,
    BuildOptions, TransitParams,
};

fn hhmm(s: i32) -> String {
    format!("{:02}:{:02}", s / 3600, (s / 60) % 60)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = common::data_dir();
    let points = common::read_points(&dir, &[]);
    let mut opts = BuildOptions::default();
    opts.n_threads = 2;
    let net = build_from_dir(&dir, &opts)?;
    let origin = &points.coords[..1];

    // Expanded: one row per departure minute (and per draw with frequencies).
    let mut params = TransitParams::default();
    params.date = 20190513;
    params.departure_time = 14 * 3600;
    params.time_window = 2;
    params.seed = Some(42);
    params.n_threads = 2;
    let rows = expanded_travel_time_matrix(&net, origin, &points.coords, &params)?;
    // One destination, a ride on metro line 1 away.
    let to = points.ids.iter().position(|id| id == "89a8100c02fffff");
    for r in rows.origin(0).iter().filter(|r| Some(r.to as usize) == to) {
        match &r.trip {
            Some(t) => println!(
                "depart {} draw {}: {} min via {} ({} rides)",
                hhmm(r.departure),
                r.draw,
                t.total_time,
                t.routes,
                t.n_rides
            ),
            None => println!("depart {} draw {}: no trip", hhmm(r.departure), r.draw),
        }
    }

    // Arrival by 15:00 within 60 minutes: the search window is the 60
    // minutes before the arrival time.
    let max_trip = 60;
    let (date, departure) = arrival_departure(20190513, 15 * 3600, max_trip)?;
    let mut params = TransitParams::default();
    params.date = date;
    params.departure_time = departure;
    params.time_window = max_trip;
    params.max_trip_duration = max_trip;
    params.max_walk_time = max_trip;
    params.seed = Some(42);
    params.n_threads = 2;
    let rows = arrival_travel_time_matrix(&net, origin, &points.coords, &params)?;
    for r in rows.origin(0).iter().take(5) {
        if let Some(t) = &r.trip {
            println!(
                "to {}: leave {} for {} min",
                points.ids[r.to as usize],
                hhmm(r.departure),
                t.total_time
            );
        }
    }
    Ok(())
}
