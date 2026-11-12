//! Fares (`setup_fare_structure()`, `max_fare` and
//! `pareto_frontier()`): travel times within a budget, and the trade-off
//! between time and money.
//!
//! ```sh
//! cargo run --release -p peabiru-core --example fares [network_dir [fare_structure.zip]]
//! ```

mod common;

use peabiru_core::{
    build_from_dir, pareto_frontier, read_fare_structure, setup_fare_structure,
    transit_travel_time_matrix, write_fare_structure, BuildOptions, TransitParams,
};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = common::data_dir();
    let points = common::read_points(&dir, &[]);
    let mut opts = BuildOptions::default();
    opts.n_threads = 2;
    let net = build_from_dir(&dir, &opts)?;

    // A fare structure file written by write_fare_structure() (second
    // argument), or one fare type per mode where every ride and every
    // discounted transfer costs 5.
    let fares = match std::env::args_os().nth(2) {
        Some(zip) => read_fare_structure(&std::fs::read(zip)?, "UTF-8")?,
        None => setup_fare_structure(&net, 5.0, "MODE")?,
    };
    for t in &fares.fares_per_type {
        println!(
            "type {:<7} fare {:.2}",
            t.fare_type.as_deref().unwrap_or("NA"),
            t.fare
        );
    }
    println!(
        "{} transfer rules, {} routes, at most {} discounted transfers within {} min",
        fares.fares_per_transfer.len(),
        fares.fares_per_route.len(),
        fares.max_discounted_transfers,
        fares.transfer_time_allowance
    );
    // The structure as the bytes of a .zip file, and back.
    let zip = write_fare_structure(&fares)?;
    assert_eq!(read_fare_structure(&zip, "UTF-8")?, fares);

    let mut p = TransitParams::default();
    p.date = 20190513;
    p.departure_time = 14 * 3600;
    p.seed = Some(42);
    p.n_threads = 2;
    p.fare_structure = Some(Arc::new(fares));
    let origin = &points.coords[..1];
    let destinations = &points.coords[..];

    // The fastest trip that costs no more than max_fare.
    for max_fare in [0.0, 5.0, f64::INFINITY] {
        p.max_fare = max_fare;
        let m = transit_travel_time_matrix(&net, origin, destinations, &p)?;
        let times: Vec<u32> = (0..destinations.len())
            .filter_map(|j| m.get(0, j)[0])
            .collect();
        println!(
            "max_fare {max_fare:>4}: {} destinations, median {} min",
            times.len(),
            median(times)
        );
    }

    // pareto_frontier(): per destination, the travel time at each fare
    // cutoff where it changes.
    p.max_fare = f64::INFINITY;
    let frontier = pareto_frontier(&net, origin, destinations, &p, &[0.0, 5.0, 10.0])?;
    let mut last = usize::MAX;
    for r in &frontier.rows {
        let changed = frontier
            .rows
            .iter()
            .filter(|s| s.destination == r.destination)
            .count()
            > 1;
        if changed && r.destination != last {
            print!("\nto {}:", points.ids[r.destination]);
            last = r.destination;
        }
        if changed {
            print!(" {} min for {:.2}", r.travel_time, r.monetary_cost);
        }
    }
    println!();
    Ok(())
}

fn median(mut v: Vec<u32>) -> u32 {
    v.sort_unstable();
    v.get(v.len() / 2).copied().unwrap_or(0)
}
