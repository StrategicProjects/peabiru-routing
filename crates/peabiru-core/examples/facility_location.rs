//! Where to open two new schools so that the most residents are within a
//! 12-minute walk of one, and schools per resident by the two-step
//! floating catchment area.
//!
//! ```sh
//! cargo run --release -p peabiru-core --example facility_location [network_dir]
//! ```

mod common;

use peabiru_core::{
    build_from_dir, floating_catchment, locate_facilities, street_travel_time_matrix, BuildOptions,
    CostTable, FloatingCatchmentParams, LocationModel, LocationParams, Mode, RouteParams,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = common::data_dir();
    let points = common::read_points(&dir, &["population", "schools"]);
    let (population, n_schools) = (&points.columns[0], &points.columns[1]);
    let mut opts = BuildOptions::default();
    opts.n_threads = 2;
    let net = build_from_dir(&dir, &opts)?;

    // The sites of the table: the points with a school (the existing
    // facilities) first, then the others (the candidates).
    let n = points.ids.len();
    let existing: Vec<usize> = (0..n).filter(|&i| n_schools[i] > 0.0).collect();
    let others: Vec<usize> = (0..n).filter(|&i| n_schools[i] == 0.0).collect();
    let sites: Vec<usize> = existing.iter().chain(&others).copied().collect();
    let site_coords: Vec<(f64, f64)> = sites.iter().map(|&i| points.coords[i]).collect();

    // Minutes on foot from every point to every site, up to 12.
    let cutoff = 12;
    let mut route = RouteParams::default();
    route.max_trip_duration = cutoff;
    route.n_threads = 2;
    let matrix = street_travel_time_matrix(&net, &points.coords, &site_coords, Mode::Walk, &route)?;
    let costs = CostTable::from_minutes(n, sites.len(), matrix.as_slice())?;

    let mut params = LocationParams::default();
    params.model = LocationModel::MaxCover;
    params.n_facilities = 2;
    params.cutoff = f64::from(cutoff);
    params.existing = (0..existing.len()).collect();
    params.n_threads = 2;
    let found = locate_facilities(&costs, population, &params)?;
    println!(
        "covered today: {} of {} residents; greedy: {}; after {} swap(s): {}",
        found.covered_before,
        found.total_demand,
        found.objective_greedy,
        found.n_swaps,
        found.covered
    );
    for (k, &j) in found.sites.iter().enumerate() {
        println!(
            "site {}: {} (+{} residents, {} so far)",
            k + 1,
            points.ids[sites[j]],
            found.marginal[k],
            found.cumulative[k]
        );
    }
    // What a school at each candidate would cover alone: the best one.
    let best = (existing.len()..sites.len())
        .max_by(|&a, &b| found.potential[a].total_cmp(&found.potential[b]))
        .expect("a candidate");
    println!(
        "largest potential: {} ({} residents)",
        points.ids[sites[best]], found.potential[best]
    );

    // Schools per resident within a 20-minute walk.
    let school_coords: Vec<(f64, f64)> = existing.iter().map(|&i| points.coords[i]).collect();
    let supply: Vec<f64> = existing.iter().map(|&i| n_schools[i]).collect();
    route.max_trip_duration = 20;
    let matrix =
        street_travel_time_matrix(&net, &points.coords, &school_coords, Mode::Walk, &route)?;
    let costs = CostTable::from_minutes(n, existing.len(), matrix.as_slice())?;
    let mut fca = FloatingCatchmentParams::default();
    fca.cutoff = 20.0;
    let index = floating_catchment(&costs, population, &supply, &fca)?;
    let shared: f64 = population
        .iter()
        .zip(&index.index)
        .map(|(p, a)| p * a)
        .sum();
    println!(
        "{}: {:.2} schools per 10,000 residents; shared out in all: {shared:.1} schools",
        points.ids[0],
        10_000.0 * index.index[0]
    );
    Ok(())
}
