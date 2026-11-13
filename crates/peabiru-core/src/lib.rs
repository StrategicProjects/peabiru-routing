//! Multimodal routing engine behind the `peabiru` packages for R, Python and
//! JavaScript.
//!
//! The engine builds a routable [`Network`] in memory from an OpenStreetMap
//! PBF extract (walk, bike, car), optionally GTFS feeds (public transport)
//! and an elevation raster, and computes over it:
//!
//! - travel-time matrices by street ([`street_travel_time_matrix`]) and by
//!   public transport ([`transit_travel_time_matrix`]: any subset of the
//!   transit modes, reached and left on foot, by bicycle or by car), with
//!   percentiles and Monte Carlo draws for frequency-based feeds;
//! - accessibility with five decay functions ([`transit_accessibility`],
//!   [`street_accessibility`]);
//! - expanded and arrival-time matrices ([`expanded_travel_time_matrix`],
//!   [`arrival_travel_time_matrix`]);
//! - isochrones, as polygons or street segments (see [Isochrones](#isochrones));
//! - detailed itineraries, leg by leg ([`detailed_itineraries`]);
//! - fares: travel times within a budget, fares of itineraries and the
//!   Pareto frontier of time and fare (see [Fares](#fares));
//! - slope costs for walking and cycling from a GeoTIFF raster ([`Elevation`]);
//! - scenarios of car speeds and levels of traffic stress ([`Scenario`]);
//! - what a network holds, and the GTFS errors of each feed (see
//!   [Inspecting a network](#inspecting-a-network)).
//!
//! This crate is the shared core: the R package (extendr), the Python
//! package (PyO3) and the JavaScript package (wasm-bindgen) are thin
//! bindings over the same functions, so they all return the same numbers;
//! the `peabiru` command-line tool runs it on CSV files.
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
//!     Mode, RouteParams, TransitParams,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // the São Paulo sample of the repository (r/inst/extdata/spo)
//! let network = build_from_dir("r/inst/extdata/spo", &BuildOptions::default())?;
//! for w in network.build_warnings() {
//!     eprintln!("warning: {w}");
//! }
//! let points = [(-23.5563, -46.6290), (-23.5577, -46.6321)]; // (lat, lon)
//!
//! // walking, with the default parameters
//! let walk = street_travel_time_matrix(&network, &points, &points, Mode::Walk, &RouteParams::default())?;
//! println!("walk: {:?} min", walk.get(0, 1));
//!
//! // walking and public transport, Monday 2019-05-13 from 14:00
//! let mut params = TransitParams::default();
//! params.date = 20190513;
//! params.departure_time = 14 * 3600;
//! params.seed = Some(42);
//! let transit = transit_travel_time_matrix(&network, &points, &points, &params)?;
//! println!("transit: {:?} min at percentiles {:?}", transit.get(0, 1), transit.percentiles());
//! # Ok(())
//! # }
//! ```
//!
//! A [`Network`] is built once and shared by every call (and by threads):
//! every function takes it whole, so the streets and the public transport
//! layer linked to them always go together.
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
//! [`TransitParams`], [`AccessibilityParams`], [`ItineraryParams`]) starts
//! from `Default` ([`BuildOptions::elevation`] is off by default) and is
//! filled in by setting its public fields:
//!
//! ```
//! let mut p = peabiru_core::RouteParams::default();
//! p.walk_speed = 4.5;
//! p.max_trip_duration = 60;
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
//! band it with [`isochrone_lines`]. The functions say what they return and
//! what is left to the caller (repairing and clipping the polygons).
//!
//! # Inspecting a network
//!
//! What a network holds: where points snap to the streets
//! ([`find_snap`]), the box of the street network
//! ([`Network::street`], [`StreetNetwork::bbox`]), its vertices and edges ([`street_vertices`],
//! [`street_edges`]), the transit stops and the routes of each trip
//! pattern ([`transit_stops`], [`transit_routes`]), the dates with service
//! ([`transit_service_dates`], [`TransitLayer::n_services_active_on`]) and
//! the errors gtfs-lib reports loading a feed ([`gtfs_errors`]).
//!
//! A network is not built from a feed with GTFS errors of priority `HIGH`,
//! or from one gtfs-lib cannot load: the build fails, and
//! [`NetworkError::high_priority_gtfs_errors`] names the feeds. With
//! [`BuildOptions::ignore_gtfs_errors`] it is built from what can be read,
//! and [`Network::build_warnings`] says which feeds those are (with the
//! warnings about the elevation raster).
//!
//! # Itineraries
//!
//! [`detailed_itineraries`] routes each origin to the destination of the
//! same index and returns the trips leg by leg ([`Itinerary`],
//! [`ItineraryLeg`]), with their lines: by a street mode, or with public
//! transport the fastest trip over the departure window or, with
//! [`ItineraryParams::shortest_path`] `false`, the alternatives of the
//! multi-criteria router. Networks with frequency-based trips are refused
//! there. [`write_itineraries_csv`] writes them to CSV files.
//!
//! # Fares
//!
//! A [`FareStructure`] ([`setup_fare_structure`], [`read_fare_structure`],
//! [`write_fare_structure`]) in [`TransitParams::fare_structure`] routes
//! public transport with the multi-criteria router and fares; with
//! [`TransitParams::max_fare`] the matrices and accessibility keep the
//! fastest trip within the budget. [`pareto_frontier`] gives, per
//! destination, the travel time at each fare cutoff where it changes, and
//! [`ItineraryParams::fare_structure`] the fares of itineraries.
//!
//! # Elevation
//!
//! With [`BuildOptions::elevation`] set to [`Elevation::Tobler`] or
//! [`Elevation::Minetti`], walking and cycling times depend on the slope of
//! each street, from the `.tif` raster of the directory
//! ([`build_from_dir`]) or from its bytes
//! ([`build_from_bytes_with_elevation`], which lists the rasters and
//! coordinate systems supported). [`StreetNetwork::elevation_warnings`]
//! reports voids, out-of-range values and samples on a pixel boundary.
//!
//! # Results
//!
//! A matrix is returned in a type of its own ([`StreetMatrix`],
//! [`TransitMatrix`], [`ExpandedMatrix`], [`Accessibility`]), which gives
//! its dimensions, single values, rows, and its whole buffer without
//! copying (`into_vec`, `into_values`). [`CsvOutput`] writes them to one
//! CSV file per origin, formatted as `data.table::fwrite` formats them.
//!
//! # Scenarios
//!
//! [`RouteParams::scenario`], [`TransitParams::scenario`] and
//! [`ItineraryParams::scenario`] change the car speeds and the levels of
//! traffic stress of the streets for one call (see [`Scenario`]); the
//! network itself is left as it is.
//!
//! # Cancellation
//!
//! Every routing parameter struct holds a [`CancelToken`]: cancel a clone
//! of it from another thread and the computation returns
//! [`RouteError::Cancelled`].
//!
//! # Routing model and credits
//!
//! The routing model follows that of Conveyal's
//! [R5](https://github.com/conveyal/r5) (MIT), from which the routing logic
//! is ported; the repository checks the results against reference outputs
//! (see the [comparison with r5r](https://strategicprojects.github.io/peabiru-routing/rust/parity.html)).
//! The few places where results depend on the platform
//! (the order in which the operating system lists GTFS feeds, the C++
//! library isoband was built with) are documented with the functions
//! concerned.
//!
//! Polygon isochrones are a translation of the isoband algorithm (MIT);
//! parts of Apache Commons Math 3.0 (Apache-2.0) and fdlibm 5.3 are
//! translated, so that the numbers are the same on every platform. The
//! notices of these and of
//! the other works the crate derives from are in `LICENSE-THIRD-PARTY`.

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
mod network;
mod options;
mod osm;
mod output;
mod street;
mod sys;
mod transit;

#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod sample_data;

pub use cancel::{CancelToken, RouteError};
pub use elevation::Elevation;
pub use error::NetworkError;
pub use fares::{
    read_fare_structure, write_fare_structure, FarePerRoute, FarePerTransfer, FarePerType,
    FareStructure,
};
pub use gtfs::validate::{gtfs_errors, GtfsErrorPriority, GtfsFeedError};
pub use inspect::{Snap, StreetVertices, TransitRoutes, TransitStops};
pub use isochrone::{
    isochrone_cutoffs, isochrone_lines, isochrone_polygons, isochrone_trip_limit, IsochroneLine,
    IsochronePolygon, Polygon, StreetEdges, TravelTimeSurface, WebMercatorGrid,
};
pub use itinerary::{
    write_itineraries_csv, Itinerary, ItineraryLeg, ItineraryNotes, ItineraryParams,
};
pub use network::{
    arrival_travel_time_matrix, build_from_bytes, build_from_bytes_with_elevation, build_from_dir,
    build_network, detailed_itineraries, detailed_itineraries_noted, expanded_travel_time_matrix,
    find_snap, pareto_frontier, setup_fare_structure, street_accessibility, street_edges,
    street_travel_time_matrix, street_travel_time_surface, street_vertices, transit_accessibility,
    transit_routes, transit_service_dates, transit_stops, transit_travel_time_matrix,
    transit_travel_time_surface, Network,
};
pub use options::{trip_options, write_trip_options_csv, Criterion, TripOption, TripOptionsParams};
pub use output::CsvOutput;
pub use street::{
    BuildOptions, CarSpeeds, LtsChange, LtsLine, Mode, RouteParams, Scenario, SpeedPolygon,
    StreetEngine, StreetMatrix, StreetNetwork,
};
pub use transit::accessibility::{
    accessibility_trip_limits, Accessibility, AccessibilityParams, Destinations,
};
pub use transit::decay::DecayFunction;
pub use transit::expanded::{arrival_departure, Breakdown, ExpandedMatrix, ExpandedRow};
pub use transit::layer::{gtfs_files_in_dir, DroppedTrips, FeedReport, TransitLayer};
pub use transit::matrix::{validate_date, validate_percentiles, TransitMatrix, TransitParams};
pub use transit::pareto::{ParetoFrontier, ParetoRow};
pub use transit::raptor::TransitMode;

/// Version of this crate, as published.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Internals for this repository's own tools (the `peabiru` command-line
/// tool), **not part of the API**: anything here may change or go away in
/// any release, semantic versioning does not apply, and the items are left
/// out of the documentation on purpose. Do not use them from other crates.
#[doc(hidden)]
pub mod unstable {
    pub use crate::fares::zipio::{fwrite_csv, fwrite_csv_native, zip_deflated, FwriteColumn};
    pub use crate::gdal::{gdal_geojson_coordinate, gdal_geojson_text};

    /// The check [`CsvOutput::new`](crate::CsvOutput::new) and the writers
    /// of pairs make on the ids their file names are built from, for the
    /// packages to make before routing.
    pub fn check_output_ids<'a>(
        ids: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), crate::RouteError> {
        crate::output::check_file_ids(ids)
    }

    /// The files of [`write_fare_structure`](crate::write_fare_structure)'s
    /// archive, by name, with the platform's line ends.
    pub fn fare_structure_files(
        s: &crate::FareStructure,
    ) -> Result<Vec<(&'static str, String)>, crate::RouteError> {
        let csvs = crate::fares::zipio::fare_csvs(s, crate::output::LINE_END)?;
        Ok(crate::fares::zipio::FILES
            .iter()
            .copied()
            .zip(csvs)
            .collect())
    }
    pub use crate::isochrone::CxxLibrary;
    /// [`isochrone_polygons`](crate::isochrone_polygons) with the rings in
    /// the order isoband gives them when built with `lib`, whatever the
    /// platform (for comparing results across platforms).
    pub fn isochrone_polygons_in(
        surface: &crate::TravelTimeSurface,
        cutoffs: &[f64],
        lib: CxxLibrary,
    ) -> Result<Vec<crate::IsochronePolygon>, crate::RouteError> {
        crate::isochrone::isochrone_polygons_in(surface, cutoffs, lib)
    }
    pub use crate::osm::pbf::{read_pbf_file, tag, Extract, ReadOptions};
    pub use crate::street::scenario::{java_parse_float, java_parse_int, java_parse_long};

    /// The street edges of the network the street searches of a call with
    /// scenario `s` run on (`None`: the scenario changes nothing).
    pub fn scenario_street_edges(
        network: &crate::Network,
        s: &crate::Scenario,
    ) -> Result<Option<crate::StreetEdges>, crate::RouteError> {
        Ok(s.apply(network.street())?
            .map(|net| crate::isochrone::street_edges(&net)))
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
    /// is ambiguous (within nanometres of a pixel boundary, where R5's
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
