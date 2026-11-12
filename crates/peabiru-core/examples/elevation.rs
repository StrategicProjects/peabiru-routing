//! Slope costs from an elevation raster, as r5r's
//! `build_network(elevation = "TOBLER" | "MINETTI")`: walking and cycling
//! times change with the slope of each street, driving times do not.
//!
//! ```sh
//! cargo run --release -p peabiru-core --example elevation [network_dir [raster.tif]]
//! ```
//!
//! Without a raster it uses `spo_hills.tif` from the crate's test data:
//! synthetic hills between 743 and 834 m over the São Paulo sample, not
//! measured data.

mod common;

use peabiru_core::{
    build_from_bytes_with_elevation, gtfs_files_in_dir, street_travel_time_matrix, BuildOptions,
    Elevation, Mode, RouteParams,
};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = common::data_dir();
    let points = common::read_points(&dir, &[]);
    let raster = std::env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/spo_hills.tif")
        });

    // build_from_dir() reads a .tif in the directory when
    // BuildOptions::elevation is set; here the bytes are given instead, to
    // build the same network with each model.
    let pbf = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|x| x == "pbf"))
        .expect("a .pbf file in the directory");
    let pbf = std::fs::read(pbf)?;
    let feeds: Vec<(String, Vec<u8>)> = gtfs_files_in_dir(&dir)?
        .into_iter()
        .map(|p| Ok((p.display().to_string(), std::fs::read(&p)?)))
        .collect::<std::io::Result<_>>()?;
    let feeds: Vec<(&str, &[u8])> = feeds
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let tif = std::fs::read(&raster)?;

    let mut params = RouteParams::default();
    params.n_threads = 2;
    let (o, d) = (&points.coords[..1], &points.coords[..]);
    for model in [Elevation::None, Elevation::Tobler, Elevation::Minetti] {
        let mut opts = BuildOptions::default();
        opts.n_threads = 2;
        opts.elevation = model;
        let net = build_from_bytes_with_elevation(&pbf, &feeds, &tif, &opts)?;
        // What r5r stores silently (voids, no-data values, samples on a
        // pixel boundary of a projected raster); the packages warn.
        for w in net.build_warnings() {
            eprintln!("warning: {w}");
        }
        let mut line = format!("{:<8}", model.r5r_name());
        for mode in [Mode::Walk, Mode::Bicycle, Mode::Car] {
            let m = street_travel_time_matrix(&net, o, d, mode, &params)?;
            let total: u32 = m.row(0).iter().flatten().sum();
            line += &format!("  {mode:?} {total:>4} min");
        }
        println!("{line}   (sum of the times from the first point)");
    }
    Ok(())
}
