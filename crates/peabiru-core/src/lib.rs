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
//!
//! # Example
//!
//! Build a network from a directory holding one OpenStreetMap `.pbf` and
//! the GTFS `.zip` feeds, then compute a street and a public transport
//! matrix between the same points:
//!
//! ```no_run
//! use peabiru_core::{
//!     build_from_dir, street_travel_time_matrix, transit_travel_time_matrix, BuildOptions,
//!     Mode, RouteParams, TransitData, TransitParams,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // the São Paulo sample of the repository (r/inst/extdata/spo)
//! let (net, layer) = build_from_dir("r/inst/extdata/spo", &BuildOptions::default())?;
//! let points = [(-23.5563, -46.6290), (-23.5577, -46.6321)]; // (lat, lon)
//!
//! // walking, r5r's defaults
//! let walk = street_travel_time_matrix(&net, &points, &points, Mode::Walk, &RouteParams::default())?;
//! println!("walk: {:?} min", walk.get(0, 1));
//!
//! // walking and public transport, Monday 2019-05-13 from 14:00
//! let data = TransitData::new(&net, &layer); // once per network
//! let mut params = TransitParams::default();
//! params.date = 20190513;
//! params.departure_time = 14 * 3600;
//! params.seed = Some(42);
//! let transit = transit_travel_time_matrix(&net, &layer, &data, &points, &points, &params)?;
//! println!("transit: {:?} min at percentiles {:?}", transit.get(0, 1), transit.percentiles());
//! # Ok(())
//! # }
//! ```
//!
//! # Threads
//!
//! Each parameter struct has an `n_threads` field: the work is shared out
//! among that many worker threads (0 = all cores), and the results never
//! depend on it. On WebAssembly targets, where `std` cannot start threads,
//! everything runs on the calling thread whatever `n_threads` says.
//!
//! # Parameters
//!
//! Every parameter struct ([`BuildOptions`], [`RouteParams`],
//! [`TransitParams`], [`AccessibilityParams`]) starts from `Default`, which
//! holds the defaults of r5r, and is filled in by setting its public
//! fields:
//!
//! ```
//! let mut p = peabiru_core::RouteParams::default();
//! p.walk_speed_kmh = 4.5;
//! p.max_trip_duration_min = 60;
//! ```
//!
//! The structs are `#[non_exhaustive]`, so later versions can add fields
//! without breaking this code; for the same reason they cannot be written
//! as struct expressions outside this crate, not even with
//! `..Default::default()`. Invalid values are rejected with
//! [`RouteError::Invalid`] before any routing.
//!
//! # Results
//!
//! A matrix is returned in a type of its own ([`StreetMatrix`],
//! [`TransitMatrix`], [`ExpandedMatrix`], [`Accessibility`]), which gives
//! its dimensions, single values, rows, and its whole buffer without
//! copying (`into_vec`, `into_values`).
//!
//! # Cancellation
//!
//! Every routing parameter struct holds a [`CancelToken`]: cancel a clone
//! of it from another thread and the computation returns
//! [`RouteError::Cancelled`].

#![warn(missing_docs)]

mod cancel;
mod error;
mod fastmath;
mod gtfs;
mod jcompat;
mod osm;
mod street;
mod sys;
mod transit;

#[cfg(test)]
mod review_tests;

pub use cancel::{CancelToken, RouteError};
pub use error::NetworkError;
pub use street::{
    build_network, street_travel_time_matrix, BuildOptions, Mode, RouteParams, StreetEngine,
    StreetMatrix, StreetNetwork,
};
pub use transit::accessibility::{
    r5r_trip_limits, street_accessibility, transit_accessibility, Accessibility,
    AccessibilityParams, Destinations,
};
pub use transit::decay::DecayFunction;
pub use transit::expanded::{
    arrival_departure, arrival_travel_time_matrix, expanded_travel_time_matrix, Breakdown,
    ExpandedMatrix, ExpandedRow,
};
pub use transit::layer::{DroppedTrips, FeedReport, TransitLayer};
pub use transit::matrix::{
    transit_travel_time_matrix, validate_date, validate_percentiles, TransitData, TransitMatrix,
    TransitParams,
};
pub use transit::build_from_dir;

/// Version of this crate, as published.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Internals for this repository's own tools (the `peabiru` command-line
/// tool), **not part of the API**: anything here may change or go away in
/// any release, semantic versioning does not apply, and the items are left
/// out of the documentation on purpose. Do not use them from other crates.
#[doc(hidden)]
pub mod unstable {
    pub use crate::osm::pbf::{read_pbf_file, tag, Extract, ReadOptions};
    pub use crate::street::flags;
    pub use crate::street::labels::{cms_to_ms, lts_of};
    use crate::StreetNetwork;

    /// One directed edge of a street network, as stored.
    #[derive(Debug, Clone, Copy)]
    pub struct Edge {
        /// OSM way id (negative for a generated connector).
        pub way: i64,
        /// Length in millimetres.
        pub length_mm: i32,
        /// Bit set of [`flags`].
        pub flags: u32,
        /// Car speed in cm/s, as stored.
        pub speed_cms: i16,
        /// Start and end vertex as `(lat, lon)`, fixed-point 1e-7 degrees.
        pub from: (i32, i32),
        pub to: (i32, i32),
    }

    /// Directed edge `e` of `net` (`0..net.n_edges()`).
    pub fn edge(net: &StreetNetwork, e: usize) -> Edge {
        let (v0, v1) = (net.edge_from(e), net.edge_to(e));
        Edge {
            way: net.pair_way[e / 2],
            length_mm: net.pair_length_mm[e / 2],
            flags: net.edge_flags[e],
            speed_cms: net.edge_speed_cms[e],
            from: (net.v_lat[v0], net.v_lon[v0]),
            to: (net.v_lat[v1], net.v_lon[v1]),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
