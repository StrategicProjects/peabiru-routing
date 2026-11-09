//! Multimodal routing engine behind the `peabiru` packages for R and Python.
//!
//! The engine builds a routable network from an OpenStreetMap PBF extract
//! (walk, bike, car) and, optionally, GTFS feeds (public transport), and
//! computes travel-time matrices, accessibility and isochrones over it.
//!
//! This crate is the shared core: the R package (extendr) and the Python
//! package (PyO3) are thin bindings over the same functions, so all three
//! return the same numbers.

pub mod osm;

/// Version of this crate, as published.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
