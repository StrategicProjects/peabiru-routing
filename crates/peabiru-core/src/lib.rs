//! Multimodal routing engine behind the `peabiru` packages for R and Python.
//!
//! The engine builds a routable network from an OpenStreetMap PBF extract
//! (walk, bike, car) and, optionally, GTFS feeds (public transport), and
//! computes travel-time matrices, accessibility, and expanded and
//! arrival-time matrices over it.
//!
//! This crate is the shared core: the R package (extendr) and the Python
//! package (PyO3) are thin bindings over the same functions, so all three
//! return the same numbers.
//!
//! The routing logic is a port of Conveyal's [R5](https://github.com/conveyal/r5)
//! (MIT), version 7.6, and returns the results of
//! [r5r](https://github.com/ipea/r5r) 2.5.0. The notices of R5 and of the
//! other works this crate derives from are in `LICENSE-THIRD-PARTY`.

mod cancel;
pub mod fastmath;
pub mod gtfs;
pub mod jcompat;
pub mod osm;
pub mod street;
pub mod transit;

pub use cancel::{CancelToken, RouteError};

/// Version of this crate, as published.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
