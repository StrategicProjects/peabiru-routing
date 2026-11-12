//! The crate's central type, [`Network`], how it is built, and the
//! functions that route on it. The work of each function is done in its
//! module; this file holds the public signatures and their documentation.

use crate::transit::layer::TransitLayer;
use crate::transit::matrix::TransitData;
use crate::{
    Accessibility, AccessibilityParams, BuildOptions, Destinations, ExpandedMatrix, FareStructure,
    Itinerary, ItineraryNotes, ItineraryParams, Mode, NetworkError, ParetoFrontier, RouteError,
    RouteParams, Snap, StreetEdges, StreetMatrix, StreetNetwork, StreetVertices, TransitMatrix,
    TransitParams, TransitRoutes, TransitStops, TravelTimeSurface,
};
use std::path::Path;

/// A routable network: the street network built from an OpenStreetMap
/// extract and, when GTFS feeds were given, the public transport layer
/// built from them with its stops linked to those streets.
///
/// Every routing function of the crate takes a `&Network`. It is built once
/// ([`build_from_dir`], [`build_from_bytes`],
/// [`build_from_bytes_with_elevation`], or [`build_network`] for streets
/// only) and does not change afterwards, so it can be shared by threads
/// (`Network` is `Send` and `Sync`) and used for any number of calls. The
/// only state it keeps between calls are caches that make later calls
/// faster and never change their results (the contraction hierarchies of
/// [`street_travel_time_matrix`] and the grid links of
/// [`street_travel_time_surface`]); clones share them.
///
/// ```no_run
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let network = peabiru_core::build_from_dir("r/inst/extdata/spo", &Default::default())?;
/// println!("{} street vertices", network.street().n_vertices());
/// if let Some(transit) = network.transit() {
///     println!("{} stops from {:?}", transit.n_stops(), transit.feed_ids());
/// }
/// for w in network.build_warnings() {
///     eprintln!("warning: {w}");
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Network {
    street: StreetNetwork,
    /// Empty without feeds.
    layer: TransitLayer,
    /// Made once the stops are linked, when there is at least one feed.
    data: Option<TransitData>,
}

impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Network")
            .field("vertices", &self.street.n_vertices())
            .field("edges", &self.street.n_edges())
            .field("elevation", &self.street.elevation())
            .field("feeds", &self.layer.feed_ids())
            .field("stops", &self.layer.n_stops())
            .finish()
    }
}

impl Network {
    /// A network of `street` and `layer`, whose stops are already linked to
    /// those streets.
    pub(crate) fn new(street: StreetNetwork, layer: TransitLayer) -> Self {
        let data = (!layer.feed_ids().is_empty()).then(|| TransitData::new(&street, &layer));
        Network {
            street,
            layer,
            data,
        }
    }

    /// The street network: its size, box and elevation, as
    /// [`StreetNetwork`]'s methods report them.
    pub fn street(&self) -> &StreetNetwork {
        &self.street
    }

    /// The public transport layer, `None` when the network was built
    /// without GTFS feeds (then the functions that route by public
    /// transport return [`RouteError::Invalid`]).
    pub fn transit(&self) -> Option<&TransitLayer> {
        self.data.as_ref().map(|_| &self.layer)
    }

    /// The warnings of the build, one per line, for the caller to show: one
    /// per GTFS feed built from although r5r refuses to build a network
    /// from it (with [`BuildOptions::ignore_gtfs_errors`]; see
    /// [`TransitLayer::r5r_warnings`]), then those about the elevation raster
    /// (see [`StreetNetwork::elevation_warnings`]). Empty for a clean build.
    pub fn build_warnings(&self) -> Vec<String> {
        let mut w = match self.transit() {
            Some(t) => t.r5r_warnings(),
            None => Vec::new(),
        };
        w.extend(self.street.elevation_warnings().iter().cloned());
        w
    }

    /// The layer, empty without feeds.
    pub(crate) fn layer(&self) -> &TransitLayer {
        &self.layer
    }

    /// The layer and its data, `None` without feeds.
    pub(crate) fn transit_pair(&self) -> Option<(&TransitLayer, &TransitData)> {
        self.data.as_ref().map(|d| (&self.layer, d))
    }

    /// The layer and its data, an error without feeds.
    pub(crate) fn transit_parts(&self) -> Result<(&TransitLayer, &TransitData), RouteError> {
        self.transit_pair().ok_or_else(|| {
            RouteError::Invalid("the network has no public transport (no GTFS feed)".into())
        })
    }
}

/// Read an OpenStreetMap `.pbf` file and build the street network for
/// walking, cycling and driving, as R5 7.6 builds it for r5r, without
/// public transport ([`Network::transit`] is `None`; see [`build_from_dir`]
/// for a network with GTFS feeds).
///
/// A file that cannot be read or decoded, or that gives no street edge at
/// all (empty, a header only, or no routable way), is an error naming the
/// file. R5 fails on such a file too, with an
/// `ArrayIndexOutOfBoundsException` from `StreetLayer.loadFromOsm`.
/// [`BuildOptions::elevation`] needs a raster, so it is not used here
/// (see [`build_from_bytes_with_elevation`]).
pub fn build_network(
    path: impl AsRef<std::path::Path>,
    opts: &BuildOptions,
) -> Result<Network, NetworkError> {
    let street = crate::street::build::build_network(path, opts)?;
    Ok(Network::new(street, TransitLayer::default()))
}

/// Build a network from a directory holding one OpenStreetMap `.pbf` file
/// and any number of GTFS `.zip` feeds, as r5r's `build_network()` does,
/// and link the stops to the streets; with [`BuildOptions::elevation`],
/// apply slope costs from the directory's `.tif` raster (see
/// [`build_from_bytes_with_elevation`] for the rasters supported).
///
/// Files are recognised by their extension in lower case only (`.pbf`,
/// `.zip`), as r5r does; with several `.pbf` files the last one listed is
/// used, and the feeds are loaded in the order the operating system lists
/// them, which is the order R5 loads them in. A directory without GTFS
/// feeds gives a network without public transport ([`Network::transit`] is
/// `None`), good for street routing only.
///
/// As r5r does, the build fails when a feed has GTFS errors of priority
/// `HIGH`, or is one gtfs-lib cannot load at all
/// ([`NetworkError::high_priority_gtfs_errors`],
/// [`gtfs_errors`](crate::gtfs_errors)), unless
/// [`BuildOptions::ignore_gtfs_errors`] is set; what the build has to say
/// about the feeds and the raster is in [`Network::build_warnings`].
///
/// This reads the files and builds the network as [`build_from_bytes`]
/// does from the same bytes, with the feeds in the directory's order;
/// errors name the files by their path.
pub fn build_from_dir(dir: impl AsRef<Path>, opts: &BuildOptions) -> Result<Network, NetworkError> {
    let (street, layer) = crate::transit::build_from_dir(dir, opts)?;
    Ok(Network::new(street, layer))
}

/// Build a network from data held in memory: the bytes of an OpenStreetMap
/// `.pbf` file and of any number of GTFS `.zip` feeds, each with its file
/// name. The network is the one [`build_from_dir`] builds from a directory
/// holding the same files, and the stops are linked to the streets in the
/// same way.
///
/// The feeds are loaded in the order of `feeds`. That order matters: the
/// stops, patterns and, where two feeds offer equally good trips, the
/// routes chosen follow it. [`build_from_dir`] loads them in the order the
/// operating system lists the directory (as R5 does), which differs from
/// one file system to another; to reproduce a network built from a
/// directory, pass the feeds in the order of its
/// [`TransitLayer::feed_reports`].
///
/// A feed's name stands for its file name: it names the feed in errors
/// and in [`FeedReport`](crate::FeedReport)s and, when the feed has no
/// `feed_info.feed_id`, gives its feed id (the name without a trailing
/// `.zip`), which prefixes its stop ids. Use the names the files have on
/// disk (such as `"poa_eptc.zip"`) and keep them distinct. Errors about
/// the `.pbf` data name it `OpenStreetMap data`. With no feeds, the network
/// has no public transport, good for street routing only.
///
/// As r5r does, the build fails when a feed has GTFS errors of priority
/// `HIGH`, or is one gtfs-lib cannot load at all
/// ([`NetworkError::high_priority_gtfs_errors`],
/// [`gtfs_errors`](crate::gtfs_errors)), unless
/// [`BuildOptions::ignore_gtfs_errors`] is set.
///
/// ```no_run
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let pbf = std::fs::read("r/inst/extdata/spo/spo_osm.pbf")?;
/// let gtfs = std::fs::read("r/inst/extdata/spo/spo_gtfs.zip")?;
/// let opts = peabiru_core::BuildOptions::default();
/// let network = peabiru_core::build_from_bytes(&pbf, &[("spo_gtfs.zip", &gtfs)], &opts)?;
/// assert_eq!(network.transit().unwrap().feed_ids(), ["spo_gtfs"]);
/// # Ok(())
/// # }
/// ```
pub fn build_from_bytes(
    pbf: &[u8],
    feeds: &[(&str, &[u8])],
    opts: &BuildOptions,
) -> Result<Network, NetworkError> {
    let (street, layer) = crate::transit::build_from_bytes(pbf, feeds, opts)?;
    Ok(Network::new(street, layer))
}

/// [`build_from_bytes`] with an elevation raster: the bytes of a GeoTIFF
/// file, as [`build_from_dir`] reads the `.tif` file of a directory. The
/// slope costs of [`BuildOptions::elevation`] are applied to walking and
/// cycling (with [`Elevation::None`](crate::Elevation::None) the raster is
/// not read). Errors about the raster name it `elevation raster`.
///
/// The raster must be a GeoTIFF of one band of 8, 16 or 32-bit integers
/// or 32 or 64-bit floats, uncompressed or compressed with LZW,
/// Deflate or PackBits, north up, georeferenced by a tie point and a pixel
/// scale (what GDAL writes), in WGS 84 longitude and latitude (EPSG:4326)
/// or in another system r5r reads through GeoTools' coordinate transforms
/// and peabiru reproduces: SIRGAS 2000 (4674), SAD69 (4618), Corrego
/// Alegre 1970-72 (4225), their UTM zones used in Brazil, SIRGAS 2000 /
/// Brazil Polyconic (5880), WGS 84 / UTM (326xx, 327xx) and WGS 84 /
/// Pseudo-Mercator (3857). Other rasters are an error that says what to
/// change. In a system other than EPSG:4326, r5r reads one pixel or the
/// other for a sample within nanometres of the boundary between two pixels
/// of different values (or of the raster's edge), depending on the machine
/// it runs on; peabiru reads the one Java's `StrictMath` gives, on every
/// machine, and counts such samples in
/// [`StreetNetwork::elevation_warnings`] (and so in
/// [`Network::build_warnings`]). A sample that `StrictMath` itself puts on
/// the outer edge (inside the envelope, past the last pixel) is an error,
/// as it is in r5r.
pub fn build_from_bytes_with_elevation(
    pbf: &[u8],
    feeds: &[(&str, &[u8])],
    raster: &[u8],
    opts: &BuildOptions,
) -> Result<Network, NetworkError> {
    let (street, layer) =
        crate::transit::build_from_bytes_with_elevation(pbf, feeds, raster, opts)?;
    Ok(Network::new(street, layer))
}

/// Street travel-time matrix in whole minutes by walking, cycling or
/// driving, as r5r's `travel_time_matrix()` with a single street mode.
/// Points are `(lat, lon)` degrees. [`RouteError::Invalid`] when the
/// parameters are invalid (see [`RouteParams::validate`]),
/// [`RouteError::Cancelled`] when `params.cancel` is cancelled before the
/// matrix is complete.
///
/// `params.engine` chooses how the matrix is computed (see
/// [`StreetEngine`](crate::StreetEngine)); the result is the same either
/// way. A contraction hierarchy built here is kept with the network and
/// makes later matrices of the same mode and speeds faster.
pub fn street_travel_time_matrix(
    network: &Network,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    mode: Mode,
    params: &RouteParams,
) -> Result<StreetMatrix, RouteError> {
    crate::street::router::street_travel_time_matrix(
        &network.street,
        origins,
        destinations,
        mode,
        params,
    )
}

/// Public transport travel-time matrix, as r5r's `travel_time_matrix()` with
/// public transport in `mode` (by default `c("WALK", "TRANSIT")`; see
/// [`TransitParams::transit_modes`], [`TransitParams::access_mode`] and
/// [`TransitParams::egress_mode`]). Points are `(lat, lon)` degrees.
/// [`RouteError::Invalid`] when the network has no public transport or the
/// parameters are invalid (see [`TransitParams::validate`]),
/// [`RouteError::Cancelled`] when `p.cancel` is cancelled before every
/// origin is done.
pub fn transit_travel_time_matrix(
    network: &Network,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
) -> Result<TransitMatrix, RouteError> {
    let (layer, data) = network.transit_parts()?;
    crate::transit::matrix::transit_travel_time_matrix(
        &network.street,
        layer,
        data,
        origins,
        destinations,
        p,
    )
}

/// Accessibility by public transport (r5r `accessibility()` with public
/// transport in `mode`; the modes, access and egress as in
/// [`transit_travel_time_matrix`]). Opportunity counts that are zero,
/// negative or NaN contribute nothing (`TravelTimeReducer.java:251-253`).
/// `p.percentiles` are the travel-time percentiles; `p.max_trip_duration`
/// and `p.max_walk_time` must already be what r5r hands to R5 (for the
/// step function r5r sets `max_trip_duration` to the largest cutoff before
/// deriving the walk limit; see [`r5r_trip_limits`](crate::r5r_trip_limits)).
///
/// Every origin gets a value for every (opportunity, percentile, cutoff),
/// 0 when nothing is reachable or the origin cannot be linked. Points are
/// `(lat, lon)` degrees. [`RouteError::Invalid`] when the network has no
/// public transport, or the parameters, cutoffs or opportunity columns are
/// invalid, [`RouteError::Cancelled`] when `p.cancel` is cancelled before
/// every origin is done.
pub fn transit_accessibility(
    network: &Network,
    origins: &[(f64, f64)],
    destinations: &Destinations,
    p: &TransitParams,
    a: &AccessibilityParams,
) -> Result<Accessibility, RouteError> {
    let (layer, data) = network.transit_parts()?;
    crate::transit::accessibility::transit_accessibility(
        &network.street,
        layer,
        data,
        origins,
        destinations,
        p,
        a,
    )
}

/// Accessibility by a street mode alone (r5r `accessibility()` with `mode`
/// one of `WALK`, `BICYCLE`, `CAR`): one unvarying travel time per
/// destination, so every percentile gets the same value
/// (`TravelTimeReducer.recordUnvaryingTravelTimeAtTarget`).
/// `params.max_trip_duration_min` is the search limit r5r passes to R5,
/// and `percentiles` are checked as
/// [`validate_percentiles`](crate::validate_percentiles) does.
/// Errors as [`transit_accessibility`].
pub fn street_accessibility(
    network: &Network,
    origins: &[(f64, f64)],
    destinations: &Destinations,
    mode: Mode,
    params: &RouteParams,
    percentiles: &[i32],
    a: &AccessibilityParams,
) -> Result<Accessibility, RouteError> {
    crate::transit::accessibility::street_accessibility(
        &network.street,
        origins,
        destinations,
        mode,
        params,
        percentiles,
        a,
    )
}

/// r5r's `expanded_travel_time_matrix()`: per origin, the rows in r5r's
/// order (destination, departure minute, draw). For each departure minute
/// the transit records come first in `PATH_ORDER`, numbered 1.., then the
/// remaining draws repeat the walk-only record (or an unreachable, NA one)
/// up to the draws per minute (`populateTravelTimesBreakdown`). A
/// destination with no record at all has no rows. Points are `(lat, lon)`
/// degrees; `p.percentiles` are not used.
///
/// [`RouteError::Invalid`] when the network has no public transport, the
/// parameters are invalid (see [`TransitParams::validate`]) or there are
/// more than 5,000 destinations (R5's limit for paths);
/// [`RouteError::Cancelled`] when `p.cancel` is cancelled before every
/// origin is done.
pub fn expanded_travel_time_matrix(
    network: &Network,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
) -> Result<ExpandedMatrix, RouteError> {
    let (layer, data) = network.transit_parts()?;
    crate::transit::expanded::expanded_travel_time_matrix(
        &network.street,
        layer,
        data,
        origins,
        destinations,
        p,
    )
}

/// r5r's `arrival_travel_time_matrix()`. `p` must already hold the
/// departure side of the search: the service date and departure from
/// [`arrival_departure`](crate::arrival_departure) and
/// `time_window = max_trip_duration`, as the R wrapper sets them. For each
/// destination the latest departure minute with a record arriving by
/// `departure + max_trip_duration` wins: its first transit record in
/// `PATH_ORDER`, else the walk-only record (`filterLatestBeforeArrivalTime`,
/// offs 31-266). At most one row per destination. Errors as
/// [`expanded_travel_time_matrix`], and [`RouteError::Invalid`] when
/// `p.time_window` is not `p.max_trip_duration`.
pub fn arrival_travel_time_matrix(
    network: &Network,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
) -> Result<ExpandedMatrix, RouteError> {
    let (layer, data) = network.transit_parts()?;
    crate::transit::expanded::arrival_travel_time_matrix(
        &network.street,
        layer,
        data,
        origins,
        destinations,
        p,
    )
}

/// Travel-time surface by a street mode alone (r5r's `isochrone()` with
/// `mode` one of `WALK`, `BICYCLE`, `CAR` and `polygon_output = TRUE`): one
/// travel time per cell, the same at every percentile. The grid is
/// [`WebMercatorGrid::for_network`](crate::WebMercatorGrid::for_network) at
/// `zoom` (9 to 12). `params.max_trip_duration_min` is the search limit r5r
/// hands R5 (its `isochrone()` passes the largest cutoff plus 10 minutes).
/// Points are `(lat, lon)` degrees; an origin that cannot be linked reaches
/// nothing. [`RouteError::Invalid`] for invalid parameters, percentiles
/// (see [`validate_percentiles`](crate::validate_percentiles)) or zoom,
/// [`RouteError::Cancelled`] when `params.cancel` is cancelled.
///
/// Most of a first call from a few origins is linking the grid's cells to
/// the streets. The network keeps those links for later surfaces on the
/// same grid and mode (as R5 keeps its linkages; public transport
/// surfaces walk, so they share the walking ones): up to 9 grids and
/// modes, the least recently used dropped first beyond 64 MiB, at 40
/// bytes a cell (6.2 MiB for r5r's Porto Alegre sample at zoom 12, 0.4 MiB
/// at zoom 10). Clones of the network share them.
pub fn street_travel_time_surface(
    network: &Network,
    origins: &[(f64, f64)],
    mode: Mode,
    params: &RouteParams,
    percentiles: &[i32],
    zoom: i32,
) -> Result<TravelTimeSurface, RouteError> {
    crate::isochrone::street_travel_time_surface(
        &network.street,
        origins,
        mode,
        params,
        percentiles,
        zoom,
    )
}

/// Travel-time surface by public transport (r5r's `isochrone()` with
/// public transport in `mode`, its modes, access and egress as in
/// [`TransitParams`], and `polygon_output = TRUE`), at each of
/// `p.percentiles` over the departure window. `p.max_trip_duration` is the
/// limit r5r hands R5 (the largest cutoff plus 10 minutes). Otherwise as
/// [`street_travel_time_surface`], and [`RouteError::Invalid`] when the
/// network has no public transport.
pub fn transit_travel_time_surface(
    network: &Network,
    origins: &[(f64, f64)],
    p: &TransitParams,
    zoom: i32,
) -> Result<TravelTimeSurface, RouteError> {
    let (layer, data) = network.transit_parts()?;
    crate::isochrone::transit_travel_time_surface(&network.street, layer, data, origins, p, zoom)
}

/// The street edges of the network, as r5r's
/// `street_network_to_sf(r5r_network)$edges` (see [`StreetEdges`]).
pub fn street_edges(network: &Network) -> StreetEdges {
    crate::isochrone::street_edges(&network.street)
}

/// Detailed itineraries between each origin and the destination of the
/// same index (`origins[i]` to `destinations[i]`, `(lat, lon)` degrees), as
/// r5r 2.5.0's `detailed_itineraries()` with `all_to_all = FALSE` (pair the
/// points beforehand for `all_to_all`): per pair its itineraries in r5r's
/// order, none when no trip is found (r5r then has no rows).
///
/// Without public transport (`p.transit_modes` empty) the itinerary is the
/// street path by `p.mode`. With it, the network must have public transport;
/// with `shortest_path` and no fare structure the result is the faster of
/// the direct trip and the fastest public transport trip over the window
/// (FastRaptor), otherwise the trips McRAPTOR finds, with their fares when
/// `p.fare_structure` is given.
/// Errors: invalid parameters, a network with frequency-based trips (r5r
/// refuses them too), cancellation, and the rare cases noted in the error
/// message that R5 does not route reproducibly. The notes of
/// [`detailed_itineraries_noted`] are dropped.
pub fn detailed_itineraries(
    network: &Network,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &ItineraryParams,
) -> Result<Vec<Vec<Itinerary>>, RouteError> {
    detailed_itineraries_noted(network, origins, destinations, p).map(|(its, _)| its)
}

/// [`detailed_itineraries`], with the notes the packages turn into
/// warnings.
pub fn detailed_itineraries_noted(
    network: &Network,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &ItineraryParams,
) -> Result<(Vec<Vec<Itinerary>>, ItineraryNotes), RouteError> {
    crate::itinerary::detailed_itineraries_noted(
        &network.street,
        network.transit_pair(),
        origins,
        destinations,
        p,
    )
}

/// r5r 2.5.0's `pareto_frontier()`: the trade-off between travel time and
/// fare. For every cutoff of `fare_cutoffs`, in the order given, the
/// travel times are those of R5's fare router (McRAPTOR, see
/// [`TransitParams::fare_structure`]) with `max_fare` set to the cutoff
/// (`p.max_fare` is not used); a destination and percentile gets a row at
/// each cutoff where its travel time differs from that of the previous
/// cutoff with a row, and is below `max_trip_duration`.
///
/// `p.fare_structure` is required. As in r5r, `draws_per_minute` is 1
/// whatever `p.draws_per_minute` says, so that `time_window` departure
/// times are sampled. With `p.transit_modes` empty the trip is by
/// `p.access_mode` alone (r5r's `mode` without public transport): the
/// street travel time, the same at every cutoff, with
/// `p.max_trip_duration` as the limit (r5r lowers it to the mode's own
/// limit first); otherwise the network must have public transport
/// ([`RouteError::Invalid`]). Points are `(lat, lon)` degrees.
pub fn pareto_frontier(
    network: &Network,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
    fare_cutoffs: &[f64],
) -> Result<ParetoFrontier, RouteError> {
    let transit = if p.transit_modes.is_empty() {
        None
    } else {
        Some(network.transit_parts()?)
    };
    crate::transit::pareto::pareto_frontier(
        &network.street,
        transit,
        origins,
        destinations,
        p,
        fare_cutoffs,
    )
}

/// Where each point (`(lat, lon)` degrees) snaps to the streets `mode` may
/// use, as r5r's `find_snap()`: the nearest point of an edge within
/// `radius_m` metres, looked for first within 300 m when `radius_m` is
/// larger (R5 `StreetLayer.findSplit`). `None` for a point with no such
/// edge. [`RouteError::Invalid`] for a negative or non-finite radius.
pub fn find_snap(
    network: &Network,
    points: &[(f64, f64)],
    radius_m: f64,
    mode: Mode,
) -> Result<Vec<Option<Snap>>, RouteError> {
    crate::inspect::find_snap(&network.street, points, radius_m, mode)
}

/// The street vertices r5r's `street_network_to_sf()` lists (see
/// [`StreetVertices`]). r5r collects them in a Java `HashSet<Integer>`
/// while it walks the edges, and lists them in that set's iteration order,
/// which this reproduces: by bucket of the set's final table (the hash of
/// an `Integer` spread as `h ^ (h >>> 16)`), and within a bucket in the
/// order they were added.
pub fn street_vertices(network: &Network) -> StreetVertices {
    crate::inspect::street_vertices(&network.street)
}

/// The transit stops of a network (see [`TransitStops`]): the position is
/// that of the stop's vertex in the street network, the stop's coordinates
/// rounded down to 1e-7 degrees, as r5r reports it. Empty for a network
/// without public transport.
pub fn transit_stops(network: &Network) -> TransitStops {
    match network.transit() {
        Some(layer) => crate::inspect::transit_stops(&network.street, layer),
        None => TransitStops::default(),
    }
}

/// The transit routes of a network (see [`TransitRoutes`]), none for a
/// network without public transport. [`RouteError::Invalid`] for a route
/// type R5 does not know (r5r fails with Java's
/// `IllegalArgumentException` there).
pub fn transit_routes(network: &Network) -> Result<TransitRoutes, RouteError> {
    match network.transit() {
        Some(layer) => crate::inspect::transit_routes(&network.street, layer),
        None => Ok(TransitRoutes::default()),
    }
}

/// The dates (`yyyymmdd`, ascending) on which some service of a trip in
/// the timetable runs, as r5r's `check_transit_availability()` finds them
/// when no date is given (r5r's `TransitServiceDates.getDates`): every day
/// of each such service's calendar on which it runs, and every date of its
/// calendar exceptions on which it runs. Services no trip uses do not
/// count; none for a network without public transport.
/// [`RouteError::Invalid`] when such a service's calendar has a start or
/// end that is not a date, with r5r's message.
pub fn transit_service_dates(network: &Network) -> Result<Vec<u32>, RouteError> {
    match network.transit() {
        Some(layer) => crate::inspect::transit_service_dates(layer),
        None => Ok(Vec::new()),
    }
}

/// r5r's `setup_fare_structure()`: one fare type per mode (`by =
/// "MODE"`), per agency id (`"AGENCY_ID"`, or `"AGENCY"`) or name
/// (`"AGENCY_NAME"`), or a single `GENERIC` type (anything else); every
/// route, type and transfer between two types costs `base_fare`, read as a
/// Java `float`. The routes are in the network's route order (none
/// without public transport), the types and transfers in the order of
/// r5r's Java `HashSet`s.
pub fn setup_fare_structure(
    network: &Network,
    base_fare: f64,
    by: &str,
) -> Result<FareStructure, RouteError> {
    crate::fares::setup_fare_structure(network.layer(), base_fare, by)
}
