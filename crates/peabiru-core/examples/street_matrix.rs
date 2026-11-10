//! Street-only travel-time matrix: walking, cycling and driving.
//!
//! ```sh
//! cargo run --release -p peabiru-core --example street_matrix [network_dir]
//! ```

mod common;

use peabiru_core::{build_network, street_travel_time_matrix, BuildOptions, Mode, RouteParams};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = common::data_dir();
    let points = common::read_points(&dir, &[]);

    // The street network needs only the .pbf file.
    let pbf = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|x| x == "pbf"))
        .ok_or("no .pbf file in the data directory")?;
    let mut opts = BuildOptions::default();
    opts.n_threads = 2;
    let net = build_network(&pbf, &opts)?;
    println!(
        "{} vertices, {} directed edges",
        net.n_vertices(),
        net.n_edges()
    );

    // r5r's defaults: 3.6 km/h walking, 12 km/h cycling, max_lts 2, 120 min.
    let mut params = RouteParams::default();
    params.n_threads = 2;
    let n = points.coords.len();
    for mode in [Mode::Walk, Mode::Bicycle, Mode::Car] {
        // Origins x destinations, whole minutes; None = no time.
        let m = street_travel_time_matrix(&net, &points.coords, &points.coords, mode, &params)?;
        let reached = m.as_slice().iter().filter(|t| t.is_some()).count();
        println!(
            "{mode:?}: {reached} of {} pairs reached; {} -> {}: {:?} min",
            n * n,
            points.ids[0],
            points.ids[n - 1],
            m.get(0, n - 1)
        );
    }
    Ok(())
}
