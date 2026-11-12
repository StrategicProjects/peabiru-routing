//! Multimodal routing engine behind the `peabiru` packages for R and Python.
//!
//! The engine builds a routable network from an OpenStreetMap PBF extract
//! (walk, bike, car) and, optionally, GTFS feeds (public transport), and
//! computes travel-time matrices, accessibility, expanded and
//! arrival-time matrices, and isochrones over it.
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
//! [`build_from_bytes`] builds the same network from data held in memory
//! (the `.pbf` and the feeds' `.zip` files as bytes), for callers without
//! a file system, such as the JavaScript package on WebAssembly.
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
//! # Isochrones
//!
//! Polygon isochrones take two calls: a travel-time surface to the cells of
//! a Web Mercator grid ([`street_travel_time_surface`],
//! [`transit_travel_time_surface`]), then [`isochrone_polygons`] on it.
//! Line isochrones route a matrix to the centroids of [`street_edges`] and
//! band it with [`isochrone_lines`]. See the functions for what is r5r's
//! exactly and what r5r adds afterwards.
//!
//! # Inspecting a network
//!
//! What r5r's network functions report: where points snap to the streets
//! ([`find_snap`]), the box of the street network
//! ([`StreetNetwork::bbox`]), its vertices and edges ([`street_vertices`],
//! [`street_edges`]), the transit stops and the routes of each trip
//! pattern ([`transit_stops`], [`transit_routes`]), the dates with service
//! ([`transit_service_dates`], [`TransitLayer::n_services_active_on`]) and
//! the errors gtfs-lib reports loading a feed ([`gtfs_errors`]).
//!
//! # Results
//!
//! A matrix is returned in a type of its own ([`StreetMatrix`],
//! [`TransitMatrix`], [`ExpandedMatrix`], [`Accessibility`]), which gives
//! its dimensions, single values, rows, and its whole buffer without
//! copying (`into_vec`, `into_values`). [`CsvOutput`] writes them to one
//! CSV file per origin, byte for byte as r5r does with `output_dir`.
//!
//! # Scenarios
//!
//! [`RouteParams::scenario`] and [`TransitParams::scenario`] change the car
//! speeds and the levels of traffic stress of the streets for one call, as
//! r5r's `new_carspeeds`, `carspeed_scale` and `new_lts` do (see
//! [`Scenario`]); the network itself is left as it is.
//!
//! # Cancellation
//!
//! Every routing parameter struct holds a [`CancelToken`]: cancel a clone
//! of it from another thread and the computation returns
//! [`RouteError::Cancelled`].

#![warn(missing_docs)]

mod cancel;
mod elevation;
mod error;
mod fares;
mod fastmath;
mod fastmath_trig;
mod fdlibm;
mod gdal;
mod gtfs;
mod inspect;
mod isochrone;
mod itinerary;
mod jcompat;
mod options;
mod osm;
mod output;
mod street;
mod sys;
mod transit;

#[cfg(test)]
mod review_tests;

pub use cancel::{CancelToken, RouteError};
pub use elevation::Elevation;
pub use error::NetworkError;
pub use fares::{
    read_fare_structure, setup_fare_structure, write_fare_structure, FarePerRoute, FarePerTransfer,
    FarePerType, FareStructure,
};
pub use gtfs::validate::{gtfs_errors, GtfsErrorPriority, GtfsFeedError};
pub use inspect::{
    find_snap, street_vertices, transit_routes, transit_service_dates, transit_stops, Snap,
    StreetVertices, TransitRoutes, TransitStops,
};
pub use isochrone::{
    isochrone_cutoffs, isochrone_lines, isochrone_polygons, isochrone_trip_limit, street_edges,
    street_travel_time_surface, transit_travel_time_surface, IsochroneLine, IsochronePolygon,
    Polygon, StreetEdges, TravelTimeSurface, WebMercatorGrid,
};
pub use itinerary::{
    detailed_itineraries, write_itineraries_csv, Itinerary, ItineraryLeg, ItineraryParams,
};
pub use options::{trip_options, write_trip_options_csv, Criterion, TripOption, TripOptionsParams};
pub use output::CsvOutput;
pub use street::{
    build_network, street_travel_time_matrix, BuildOptions, CarSpeeds, LtsChange, LtsLine, Mode,
    RouteParams, Scenario, SpeedPolygon, StreetEngine, StreetMatrix, StreetNetwork,
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
pub use transit::layer::{gtfs_files_in_dir, DroppedTrips, FeedReport, TransitLayer};
pub use transit::matrix::{
    transit_travel_time_matrix, validate_date, validate_percentiles, TransitData, TransitMatrix,
    TransitParams,
};
pub use transit::pareto::{pareto_frontier, ParetoFrontier, ParetoRow};
pub use transit::raptor::TransitMode;
pub use transit::{build_from_bytes, build_from_bytes_with_elevation, build_from_dir};

/// Version of this crate, as published.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Internals for this repository's own tools (the `peabiru` command-line
/// tool), **not part of the API**: anything here may change or go away in
/// any release, semantic versioning does not apply, and the items are left
/// out of the documentation on purpose. Do not use them from other crates.
#[doc(hidden)]
pub mod unstable {
    pub use crate::fares::zipio::{fwrite_csv, zip_stored, FwriteColumn};
    pub use crate::gdal::{gdal_geojson_coordinate, gdal_geojson_text};
    pub use crate::isochrone::CxxLibrary;
    /// [`isochrone_polygons`](crate::isochrone_polygons) with the rings in
    /// the order isoband gives them when built with `lib`, whatever the
    /// platform (for comparing with r5r on another one).
    pub fn isochrone_polygons_in(
        surface: &crate::TravelTimeSurface,
        cutoffs: &[f64],
        lib: CxxLibrary,
    ) -> Result<Vec<crate::IsochronePolygon>, crate::RouteError> {
        crate::isochrone::isochrone_polygons_in(surface, cutoffs, lib)
    }
    pub use crate::osm::pbf::{read_pbf_file, tag, Extract, ReadOptions};
    pub use crate::street::scenario::{java_parse_float, java_parse_int, java_parse_long};

    /// The network the street searches of a call with scenario `s` run on
    /// (`None`: the scenario changes nothing).
    pub fn scenario_network(
        net: &crate::StreetNetwork,
        s: &crate::Scenario,
    ) -> Result<Option<crate::StreetNetwork>, crate::RouteError> {
        s.apply(net)
    }
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

    /// The elevations R5 would sample on `net` from a GeoTIFF raster, in
    /// decimetres: one per vertex, and per edge pair the points every
    /// 10 m inside it (for comparing with R5's `ElevationCostField`).
    pub fn elevation_samples(
        net: &StreetNetwork,
        raster: &[u8],
    ) -> Result<(Vec<i16>, Vec<Vec<i16>>), String> {
        crate::elevation::samples_of(net, raster).map(|s| (s.vertex_dm, s.profile_dm))
    }

    /// A sample of [`raster_samples`]: the value read and whether the point
    /// is ambiguous (within nanometres of a pixel boundary, where r5r's
    /// pixel depends on the machine), or why R5 fails there.
    pub type RasterSample = Result<(f64, bool), String>;

    /// R5's `RasterDataSourceSampler.readElevation` on a GeoTIFF raster, for
    /// each `(lon, lat)`: the value of the nearest pixel, 0 outside the
    /// raster (as Java's StrictMath places the point), or why R5 (or this
    /// reader) fails there. An error for a raster that cannot be read.
    pub fn raster_samples(
        raster: &[u8],
        points: &[(f64, f64)],
    ) -> Result<Vec<RasterSample>, String> {
        crate::elevation::read_points(raster, points)
    }

    /// The transform R5 (GeoTools) applies to WGS 84 `(lon, lat)` points
    /// for a raster in EPSG `code`, `None` for a system peabiru does not
    /// support (for comparing with GeoTools).
    pub fn crs_transform(code: u32, points: &[(f64, f64)]) -> Option<Vec<(f64, f64)>> {
        let c = crate::elevation::crs::lookup(code)?;
        Some(points.iter().map(|&(lon, lat)| c.apply(lon, lat)).collect())
    }

    /// The slope factor of directed edge `e` (`None` without elevation).
    pub fn edge_slope(net: &StreetNetwork, e: usize) -> Option<f32> {
        net.edge_slope.get(e).copied()
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
