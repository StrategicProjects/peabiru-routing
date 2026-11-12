//! The repository's sample data for the tests, which the crate published
//! on crates.io does not carry. Shared by the unit tests (`mod sample_data`
//! in `lib.rs`, under `cfg(test)`) and the integration tests
//! (`#[path = "../src/sample_data.rs"] mod sample_data;`).

use std::io::Write;

/// The São Paulo sample of the repository, `r/inst/extdata/spo` (an
/// OpenStreetMap `.pbf`, a frequency-based GTFS feed and `spo_points.csv`),
/// or `None` when it is not there, as in the packaged crate: the calling
/// test then returns at once, after printing that it was skipped and why.
pub fn spo() -> Option<&'static str> {
    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../r/inst/extdata/spo");
    let files = ["spo_osm.pbf", "spo_gtfs.zip", "spo_points.csv"];
    if files
        .iter()
        .all(|f| std::path::Path::new(DIR).join(f).is_file())
    {
        return Some(DIR);
    }
    // libtest names each test's thread after the test
    let thread = std::thread::current();
    let test = thread.name().unwrap_or("test");
    // straight to stderr, in one write: libtest holds back what `eprintln!`
    // prints in a test that passes
    let line = format!(
        "skipped {test}: the repository's sample data (r/inst/extdata/spo) is not here, \
         as in the packaged crate\n"
    );
    let _ = std::io::stderr().write_all(line.as_bytes());
    None
}
