//! The command line: commands, options and their help.
//!
//! The options take the names of the arguments of the R and Python
//! packages' functions in kebab-case (`max_walk_time` is `--max-walk-time`)
//! and the same defaults; their values are checked as the packages check
//! them, before any routing.

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

const AFTER_HELP: &str = "\
Networks: a directory with one OpenStreetMap .pbf file, any number of GTFS
.zip feeds and an optional .tif elevation raster (as r5r's build_network()
reads it), or a .pbf file with --gtfs and --tif. Points: CSV files with a
header and the columns id, lon and lat (WGS 84). Results: CSV (or GeoJSON)
on standard output or in --output; progress and warnings on standard error.

Exit status: 0 done, 1 a file or network that cannot be read or written, 2 a
bad argument, 130 interrupted (Ctrl-C).

Run `peabiru <command> --help` for a command's options and an example.";

/// peabiru: multimodal routing on OpenStreetMap and GTFS, from the shell.
#[derive(Parser)]
#[command(
    name = "peabiru",
    bin_name = "peabiru",
    version,
    about = "Multimodal routing on OpenStreetMap and GTFS: travel-time matrices, accessibility, \
             isochrones, itineraries and fares",
    after_help = AFTER_HELP,
    disable_help_subcommand = true,
    max_term_width = 100
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Build a network and describe it: size, feeds, elevation, warnings;
    /// its service dates, or the GTFS errors of its feeds
    #[command(visible_alias = "build", after_help = INFO_EXAMPLES)]
    Info(InfoArgs),
    /// Travel-time matrix between origins and destinations, by a street
    /// mode or by public transport (travel_time_matrix())
    #[command(after_help = MATRIX_EXAMPLES)]
    Matrix(MatrixArgs),
    /// Every trip of a public transport matrix, by departure minute and
    /// Monte Carlo draw (expanded_travel_time_matrix())
    #[command(after_help = EXPANDED_EXAMPLES)]
    Expanded(ExpandedArgs),
    /// The latest departure arriving by a given time, per pair
    /// (arrival_travel_time_matrix())
    #[command(after_help = ARRIVAL_EXAMPLES)]
    Arrival(ArrivalArgs),
    /// Opportunities reachable from each origin, cumulative or weighted by
    /// a decay function (accessibility())
    #[command(after_help = ACCESSIBILITY_EXAMPLES)]
    Accessibility(AccessibilityArgs),
    /// Areas (or street segments) reachable within each cutoff
    /// (isochrone())
    #[command(after_help = ISOCHRONE_EXAMPLES)]
    Isochrone(IsochroneArgs),
    /// Trips leg by leg between pairs of points, with their lines
    /// (detailed_itineraries())
    #[command(after_help = ITINERARIES_EXAMPLES)]
    Itineraries(ItinerariesArgs),
    /// Travel time against fare at each fare cutoff (pareto_frontier())
    #[command(after_help = PARETO_EXAMPLES)]
    Pareto(ParetoArgs),
    /// The trips not dominated on time, fare, transfers and walking
    /// (trip_options())
    #[command(after_help = OPTIONS_EXAMPLES)]
    Options(OptionsArgs),
    /// Where points snap to the street network (find_snap())
    #[command(after_help = SNAP_EXAMPLES)]
    Snap(SnapArgs),
    /// Export the street or transit network: edges, vertices, stops,
    /// routes or its bounding box (street_network_to_sf(),
    /// transit_network_to_sf(), street_network_bbox())
    #[command(after_help = NETWORK_EXAMPLES)]
    Network(NetworkExportArgs),
    /// Fare structures: set one up for a network, read or check one
    #[command(subcommand)]
    Fares(FaresCommand),
    /// Shell completions for bash, zsh, fish, elvish or PowerShell
    #[command(
        after_help = "Example:\n  peabiru completions bash > ~/.local/share/bash-completion/completions/peabiru"
    )]
    Completions {
        /// The shell
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

const INFO_EXAMPLES: &str = "\
Examples:
  peabiru info data/poa
  peabiru info data/poa --service-dates -o dates.csv
  peabiru info data/poa --dates 2019-05-13,2019-05-18
  peabiru info data/poa --gtfs-errors -o errors.csv";
const MATRIX_EXAMPLES: &str = "\
Examples:
  peabiru matrix data/poa --origins points.csv --mode WALK -o walk.csv
  peabiru matrix data/poa --origins points.csv --mode WALK,TRANSIT \\
      --departure 2019-05-13T14:00 --percentiles 25,50,75 -o ttm.csv";
const EXPANDED_EXAMPLES: &str = "\
Example:
  peabiru expanded data/poa --origins o.csv --destinations d.csv --mode WALK,BUS \\
      --departure 2019-05-13T14:00 --breakdown -o expanded.csv";
const ARRIVAL_EXAMPLES: &str = "\
Example:
  peabiru arrival data/poa --origins o.csv --destinations d.csv --mode WALK,TRANSIT \\
      --arrival 2019-05-13T15:00 -o arrival.csv";
const ACCESSIBILITY_EXAMPLES: &str = "\
Examples:
  peabiru accessibility data/poa --origins points.csv --opportunities jobs,schools \\
      --mode WALK,TRANSIT --departure 2019-05-13T14:00 --cutoffs 30,60 -o access.csv
  peabiru accessibility data/poa --origins points.csv --opportunities healthcare \\
      --mode BICYCLE --decay-function logistic --decay-value 5 --cutoffs 30";
const ISOCHRONE_EXAMPLES: &str = "\
Examples:
  peabiru isochrone data/poa --origins one.csv --departure 2019-05-13T14:00 \\
      --cutoffs 15,30,45 -o isochrones.geojson
  peabiru isochrone data/poa --origins one.csv --mode WALK --lines --cutoffs 10,20";
const ITINERARIES_EXAMPLES: &str = "\
Example:
  peabiru itineraries data/poa --origins o.csv --destinations d.csv --mode WALK,TRANSIT \\
      --departure 2019-05-13T14:00 --format geojson -o trips.geojson";
const PARETO_EXAMPLES: &str = "\
Example:
  peabiru fares setup data/poa --base-fare 4.8 -o fares.zip
  peabiru pareto data/poa --origins o.csv --destinations d.csv --departure 2019-05-13T14:00 \\
      --fare-structure fares.zip --fare-cutoffs 0,4.8,9.6 -o frontier.csv";
const OPTIONS_EXAMPLES: &str = "\
Example:
  peabiru options data/poa --origins o.csv --destinations d.csv --departure 2019-05-13T14:00 \\
      --fare-structure fares.zip --legs legs.csv -o options.csv";
const SNAP_EXAMPLES: &str = "\
Example:
  peabiru snap data/poa --points points.csv --mode CAR -o snapped.csv";
const NETWORK_EXAMPLES: &str = "\
Examples:
  peabiru network data/poa --layer edges -o edges.geojson
  peabiru network data/poa --layer stops --format csv
  peabiru network data/poa --layer bbox";

/// How the network is built.
#[derive(Args, Clone)]
pub struct NetworkArgs {
    /// A directory with one .pbf file, GTFS .zip feeds and an optional
    /// .tif raster, or an OpenStreetMap .pbf file
    #[arg(value_name = "NETWORK")]
    pub network: PathBuf,
    /// A GTFS feed to load with a .pbf NETWORK (repeat for several; they
    /// load in the order given)
    #[arg(long, value_name = "FEED.zip", help_heading = "Network")]
    pub gtfs: Vec<PathBuf>,
    /// An elevation raster for a .pbf NETWORK
    #[arg(long, value_name = "RASTER.tif", help_heading = "Network")]
    pub tif: Option<PathBuf>,
    /// Slope costs for walking and cycling from the raster [default: TOBLER
    /// with a raster, NONE without]
    #[arg(long, value_enum, ignore_case = true, help_heading = "Network")]
    pub elevation: Option<ElevationArg>,
    /// Build the network from feeds with GTFS errors of priority HIGH,
    /// which r5r refuses (a warning per feed)
    #[arg(long, help_heading = "Network")]
    pub ignore_gtfs_errors: bool,
    /// Worker threads; 0 uses every core
    #[arg(long, value_name = "N", default_value_t = 0, help_heading = "Network")]
    pub threads: usize,
    /// No progress lines on standard error (warnings are still shown)
    #[arg(long, short, global = true)]
    pub quiet: bool,
}

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
#[value(rename_all = "UPPER")]
pub enum ElevationArg {
    None,
    Tobler,
    Minetti,
}

/// Origins and destinations.
#[derive(Args, Clone)]
pub struct PointsArgs {
    /// Origins: CSV with id, lon, lat
    #[arg(long, value_name = "FILE.csv", help_heading = "Points")]
    pub origins: PathBuf,
    /// Destinations: CSV with id, lon, lat [default: the origins]
    #[arg(long, value_name = "FILE.csv", help_heading = "Points")]
    pub destinations: Option<PathBuf>,
}

/// Modes and departure.
#[derive(Args, Clone)]
pub struct ModeArgs {
    /// Comma-separated modes: one of WALK, BICYCLE, CAR, and TRANSIT (every
    /// transit mode) or any of TRAM, SUBWAY, RAIL, BUS, FERRY, CABLE_CAR,
    /// GONDOLA, FUNICULAR [default: WALK, or the command's]
    #[arg(
        long,
        value_name = "MODES",
        value_delimiter = ',',
        help_heading = "Routing"
    )]
    pub mode: Option<Vec<String>>,
    /// The mode from the last stop to the destination: WALK, BICYCLE or CAR
    #[arg(
        long,
        value_name = "MODE",
        default_value = "WALK",
        help_heading = "Routing"
    )]
    pub mode_egress: String,
}

/// The street parts of a trip.
#[derive(Args, Clone)]
pub struct StreetArgs {
    /// Longest walk in minutes (Inf: the trip limit)
    #[arg(
        long,
        value_name = "MIN",
        default_value = "Inf",
        help_heading = "Limits"
    )]
    pub max_walk_time: String,
    /// Longest bicycle ride in minutes (Inf: the trip limit)
    #[arg(
        long,
        value_name = "MIN",
        default_value = "Inf",
        help_heading = "Limits"
    )]
    pub max_bike_time: String,
    /// Longest drive in minutes (Inf: the trip limit)
    #[arg(
        long,
        value_name = "MIN",
        default_value = "Inf",
        help_heading = "Limits"
    )]
    pub max_car_time: String,
    /// Longest trip in minutes
    #[arg(
        long,
        value_name = "MIN",
        default_value = "120",
        help_heading = "Limits"
    )]
    pub max_trip_duration: String,
    /// Walking speed, km/h
    #[arg(
        long,
        value_name = "KMH",
        default_value = "3.6",
        help_heading = "Limits"
    )]
    pub walk_speed: String,
    /// Cycling speed, km/h
    #[arg(
        long,
        value_name = "KMH",
        default_value = "12",
        help_heading = "Limits"
    )]
    pub bike_speed: String,
    /// Highest level of traffic stress (1 to 4) a cyclist rides on
    #[arg(long, value_name = "LTS", default_value = "2", help_heading = "Limits")]
    pub max_lts: String,
}

/// Public transport.
#[derive(Args, Clone)]
pub struct TransitArgs {
    /// Departure date and time, YYYY-MM-DDTHH:MM (seconds optional) (local time of the
    /// network); required with public transport
    #[arg(long, value_name = "DATETIME", help_heading = "Routing")]
    pub departure: Option<String>,
    /// Minutes of departures from --departure over which trips are routed
    #[arg(
        long,
        value_name = "MIN",
        default_value = "10",
        help_heading = "Routing"
    )]
    pub time_window: String,
    /// Most public transport rides in a trip
    #[arg(long, value_name = "N", default_value = "3", help_heading = "Limits")]
    pub max_rides: String,
}

/// Monte Carlo draws of frequency-based feeds.
#[derive(Args, Clone)]
pub struct DrawArgs {
    /// Monte Carlo draws per departure minute (frequency-based GTFS)
    #[arg(long, value_name = "N", default_value = "5", help_heading = "Routing")]
    pub draws_per_minute: String,
    /// Seed of the Monte Carlo draws (results with frequencies are
    /// reproducible only with one)
    #[arg(
        long,
        value_name = "N",
        allow_hyphen_values = true,
        help_heading = "Routing"
    )]
    pub seed: Option<String>,
}

/// Fares.
#[derive(Args, Clone)]
pub struct FareArgs {
    /// A fare structure (.zip, as `peabiru fares setup` or r5r's
    /// write_fare_structure() write it): routes with fares
    #[arg(long, value_name = "FILE.zip", help_heading = "Fares")]
    pub fare_structure: Option<PathBuf>,
    /// Highest fare a trip may cost (needs --fare-structure)
    #[arg(
        long,
        value_name = "FARE",
        default_value = "Inf",
        help_heading = "Fares"
    )]
    pub max_fare: String,
}

/// Scenarios of car speeds and traffic stress.
#[derive(Args, Clone)]
pub struct ScenarioArgs {
    /// New car speeds: CSV with osm_id, max_speed, speed_type (km/h or
    /// scale), or GeoJSON polygons with poly_id, scale, priority
    #[arg(long, value_name = "FILE", help_heading = "Scenario")]
    pub new_carspeeds: Option<PathBuf>,
    /// Factor of the car speeds --new-carspeeds leaves out (all of them
    /// without it)
    #[arg(long, value_name = "X", default_value = "1", help_heading = "Scenario")]
    pub carspeed_scale: String,
    /// New levels of traffic stress: CSV with osm_id, lts, or GeoJSON lines
    /// with line_id, lts, priority
    #[arg(long, value_name = "FILE", help_heading = "Scenario")]
    pub new_lts: Option<PathBuf>,
}

/// Where a table goes.
#[derive(Args, Clone)]
pub struct OutputArgs {
    /// Write the result to this file [default: standard output]
    #[arg(long, short, value_name = "FILE", help_heading = "Output")]
    pub output: Option<PathBuf>,
}

/// One CSV file per origin instead of one table.
#[derive(Args, Clone)]
pub struct OutputDirArgs {
    /// Write one CSV file per origin to this existing directory, as r5r's
    /// output_dir (origins routed a slice at a time)
    #[arg(
        long,
        value_name = "DIR",
        help_heading = "Output",
        conflicts_with = "output"
    )]
    pub output_dir: Option<PathBuf>,
}

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Csv,
    Geojson,
}

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Engine {
    /// A contraction hierarchy when it pays off
    Auto,
    /// One search per origin
    Search,
    /// A contraction hierarchy
    Hierarchy,
}

#[derive(Args)]
pub struct InfoArgs {
    #[command(flatten)]
    pub net: NetworkArgs,
    /// List the dates with service: date, total_services, active_services,
    /// pct_active (check_transit_availability())
    #[arg(long)]
    pub service_dates: bool,
    /// Services running on these dates (comma-separated YYYY-MM-DD)
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "DATES",
        conflicts_with = "service_dates"
    )]
    pub dates: Option<Vec<String>>,
    /// Services running on every date from this one (with --end-date)
    #[arg(long, value_name = "DATE", requires = "end_date", conflicts_with_all = ["dates", "service_dates"])]
    pub start_date: Option<String>,
    /// Last date of --start-date's range
    #[arg(long, value_name = "DATE", requires = "start_date")]
    pub end_date: Option<String>,
    /// The errors gtfs-lib records loading each feed (get_gtfs_errors());
    /// the network is not built, so feeds it refuses can be inspected
    #[arg(long, conflicts_with_all = ["service_dates", "dates", "start_date"])]
    pub gtfs_errors: bool,
    #[command(flatten)]
    pub out: OutputArgs,
}

#[derive(Args)]
pub struct MatrixArgs {
    #[command(flatten)]
    pub net: NetworkArgs,
    #[command(flatten)]
    pub points: PointsArgs,
    #[command(flatten)]
    pub mode: ModeArgs,
    #[command(flatten)]
    pub transit: TransitArgs,
    /// Up to 5 travel-time percentiles over the time window, 1 to 99
    #[arg(
        long,
        value_name = "P",
        value_delimiter = ',',
        default_value = "50",
        help_heading = "Routing"
    )]
    pub percentiles: Vec<String>,
    #[command(flatten)]
    pub street: StreetArgs,
    #[command(flatten)]
    pub draws: DrawArgs,
    #[command(flatten)]
    pub fares: FareArgs,
    #[command(flatten)]
    pub scenario: ScenarioArgs,
    /// How street matrices are computed (the result is the same)
    #[arg(long, value_enum, default_value = "auto", help_heading = "Routing")]
    pub street_engine: Engine,
    #[command(flatten)]
    pub out: OutputArgs,
    #[command(flatten)]
    pub dir: OutputDirArgs,
}

#[derive(Args)]
pub struct ExpandedArgs {
    #[command(flatten)]
    pub net: NetworkArgs,
    #[command(flatten)]
    pub points: PointsArgs,
    #[command(flatten)]
    pub mode: ModeArgs,
    #[command(flatten)]
    pub transit: TransitArgs,
    /// Add the access, wait, ride, transfer and egress times and the
    /// number of rides of each trip
    #[arg(long, help_heading = "Output")]
    pub breakdown: bool,
    #[command(flatten)]
    pub street: StreetArgs,
    #[command(flatten)]
    pub draws: DrawArgs,
    #[command(flatten)]
    pub scenario: ScenarioArgs,
    #[command(flatten)]
    pub out: OutputArgs,
    #[command(flatten)]
    pub dir: OutputDirArgs,
}

#[derive(Args)]
pub struct ArrivalArgs {
    #[command(flatten)]
    pub net: NetworkArgs,
    #[command(flatten)]
    pub points: PointsArgs,
    #[command(flatten)]
    pub mode: ModeArgs,
    /// Arrival date and time, YYYY-MM-DDTHH:MM (seconds optional): trips leave from
    /// --max-trip-duration minutes before and must arrive by it
    #[arg(long, value_name = "DATETIME", help_heading = "Routing")]
    pub arrival: String,
    /// Add the access, wait, ride, transfer and egress times and the
    /// number of rides of each trip
    #[arg(long, help_heading = "Output")]
    pub breakdown: bool,
    /// Most public transport rides in a trip
    #[arg(long, value_name = "N", default_value = "3", help_heading = "Limits")]
    pub max_rides: String,
    #[command(flatten)]
    pub street: StreetArgs,
    #[command(flatten)]
    pub draws: DrawArgs,
    #[command(flatten)]
    pub scenario: ScenarioArgs,
    #[command(flatten)]
    pub out: OutputArgs,
    #[command(flatten)]
    pub dir: OutputDirArgs,
}

#[derive(Args)]
pub struct AccessibilityArgs {
    #[command(flatten)]
    pub net: NetworkArgs,
    #[command(flatten)]
    pub points: PointsArgs,
    /// Columns of the destinations with the opportunities to count
    #[arg(
        long,
        value_name = "COLUMNS",
        value_delimiter = ',',
        default_value = "opportunities",
        help_heading = "Accessibility"
    )]
    pub opportunities: Vec<String>,
    /// step, exponential, fixed_exponential, linear or logistic
    #[arg(
        long,
        value_name = "FUNCTION",
        default_value = "step",
        help_heading = "Accessibility"
    )]
    pub decay_function: String,
    /// Travel-time cutoffs in minutes, 1 to 120 (not with fixed_exponential)
    #[arg(
        long,
        value_name = "MIN",
        value_delimiter = ',',
        help_heading = "Accessibility"
    )]
    pub cutoffs: Option<Vec<String>>,
    /// The decay function's parameter (fixed_exponential, linear, logistic)
    #[arg(
        long,
        value_name = "X",
        allow_hyphen_values = true,
        help_heading = "Accessibility"
    )]
    pub decay_value: Option<String>,
    #[command(flatten)]
    pub mode: ModeArgs,
    #[command(flatten)]
    pub transit: TransitArgs,
    /// Up to 5 travel-time percentiles over the time window, 1 to 99
    #[arg(
        long,
        value_name = "P",
        value_delimiter = ',',
        default_value = "50",
        help_heading = "Routing"
    )]
    pub percentiles: Vec<String>,
    #[command(flatten)]
    pub street: StreetArgs,
    #[command(flatten)]
    pub draws: DrawArgs,
    #[command(flatten)]
    pub fares: FareArgs,
    #[command(flatten)]
    pub scenario: ScenarioArgs,
    #[command(flatten)]
    pub out: OutputArgs,
    #[command(flatten)]
    pub dir: OutputDirArgs,
}

#[derive(Args)]
pub struct IsochroneArgs {
    #[command(flatten)]
    pub net: NetworkArgs,
    /// Origins: CSV with id, lon, lat
    #[arg(long, value_name = "FILE.csv", help_heading = "Points")]
    pub origins: PathBuf,
    /// Comma-separated modes, as for `peabiru matrix`
    #[arg(
        long,
        value_name = "MODES",
        value_delimiter = ',',
        default_value = "TRANSIT",
        help_heading = "Routing"
    )]
    pub mode: Vec<String>,
    /// The mode from the last stop to the destination: WALK, BICYCLE or CAR
    #[arg(
        long,
        value_name = "MODE",
        default_value = "WALK",
        help_heading = "Routing"
    )]
    pub mode_egress: String,
    /// Cutoffs in minutes (0 is added: the bands start at 0)
    #[arg(
        long,
        value_name = "MIN",
        value_delimiter = ',',
        default_value = "0,15,30",
        help_heading = "Isochrone"
    )]
    pub cutoffs: Vec<String>,
    /// Zoom of the Web Mercator grid of polygons, 9 to 12
    #[arg(
        long,
        value_name = "Z",
        default_value = "10",
        help_heading = "Isochrone"
    )]
    pub zoom: String,
    /// Street segments reached instead of polygons (polygon_output = FALSE)
    #[arg(long, help_heading = "Isochrone")]
    pub lines: bool,
    /// Travel-time percentiles of the polygons [default: 50]
    #[arg(
        long,
        value_name = "P",
        value_delimiter = ',',
        help_heading = "Isochrone"
    )]
    pub percentiles: Option<Vec<String>>,
    #[command(flatten)]
    pub transit: TransitArgs,
    #[command(flatten)]
    pub street: StreetArgs,
    #[command(flatten)]
    pub draws: DrawArgs,
    /// GeoJSON, or CSV with the geometry as WKT
    #[arg(long, value_enum, default_value = "geojson", help_heading = "Output")]
    pub format: Format,
    /// Order the rings of the polygons as isoband built with this C++
    /// library does (r5r on another platform)
    #[arg(long, value_name = "LIB", hide = true)]
    pub cxx_library: Option<String>,
    #[command(flatten)]
    pub out: OutputArgs,
}

#[derive(Args)]
pub struct ItinerariesArgs {
    #[command(flatten)]
    pub net: NetworkArgs,
    #[command(flatten)]
    pub points: PointsArgs,
    /// Route every origin to every destination (by default origin i to
    /// destination i, or one point to all the others)
    #[arg(long, help_heading = "Points")]
    pub all_to_all: bool,
    #[command(flatten)]
    pub mode: ModeArgs,
    #[command(flatten)]
    pub transit: TransitArgs,
    /// false: the alternatives of the multi-criteria router instead of the
    /// fastest trip
    #[arg(long, value_name = "BOOL", default_value = "true", action = clap::ArgAction::Set, help_heading = "Routing")]
    pub shortest_path: bool,
    /// With --shortest-path false: alternatives up to this many minutes
    /// slower than the fastest
    #[arg(
        long,
        value_name = "MIN",
        default_value = "0",
        help_heading = "Routing"
    )]
    pub suboptimal_minutes: String,
    #[command(flatten)]
    pub street: StreetArgs,
    #[command(flatten)]
    pub fares: FareArgs,
    #[command(flatten)]
    pub scenario: ScenarioArgs,
    /// Leave the lines of the legs out
    #[arg(long, help_heading = "Output")]
    pub drop_geometry: bool,
    /// Add the OSM ways, street edges and stops of each leg
    #[arg(long, help_heading = "Output")]
    pub osm_link_ids: bool,
    /// CSV (lines as WKT) or GeoJSON
    #[arg(long, value_enum, default_value = "csv", help_heading = "Output")]
    pub format: Format,
    #[command(flatten)]
    pub out: OutputArgs,
    /// Write one CSV file per pair to this existing directory, as r5r's
    /// output_dir
    #[arg(
        long,
        value_name = "DIR",
        help_heading = "Output",
        conflicts_with = "output"
    )]
    pub output_dir: Option<PathBuf>,
}

#[derive(Args)]
pub struct ParetoArgs {
    #[command(flatten)]
    pub net: NetworkArgs,
    #[command(flatten)]
    pub points: PointsArgs,
    /// Comma-separated modes, as for `peabiru matrix`
    #[arg(
        long,
        value_name = "MODES",
        value_delimiter = ',',
        default_value = "WALK,TRANSIT",
        help_heading = "Routing"
    )]
    pub mode: Vec<String>,
    /// The mode from the last stop to the destination: WALK, BICYCLE or CAR
    #[arg(
        long,
        value_name = "MODE",
        default_value = "WALK",
        help_heading = "Routing"
    )]
    pub mode_egress: String,
    #[command(flatten)]
    pub transit: TransitArgs,
    /// Up to 5 travel-time percentiles over the time window, 1 to 99
    #[arg(
        long,
        value_name = "P",
        value_delimiter = ',',
        default_value = "50",
        help_heading = "Routing"
    )]
    pub percentiles: Vec<String>,
    /// Seed of the departure times sampled in the window
    #[arg(
        long,
        value_name = "N",
        allow_hyphen_values = true,
        help_heading = "Routing"
    )]
    pub seed: Option<String>,
    #[command(flatten)]
    pub street: StreetArgs,
    /// The fare structure (.zip)
    #[arg(long, value_name = "FILE.zip", help_heading = "Fares")]
    pub fare_structure: PathBuf,
    /// The fares at which the travel time is computed
    #[arg(
        long,
        value_name = "FARES",
        value_delimiter = ',',
        required = true,
        help_heading = "Fares"
    )]
    pub fare_cutoffs: Vec<String>,
    #[command(flatten)]
    pub out: OutputArgs,
}

#[derive(Args)]
pub struct OptionsArgs {
    #[command(flatten)]
    pub net: NetworkArgs,
    #[command(flatten)]
    pub points: PointsArgs,
    /// Route every origin to every destination
    #[arg(long, help_heading = "Points")]
    pub all_to_all: bool,
    /// Comma-separated modes, as for `peabiru matrix`
    #[arg(
        long,
        value_name = "MODES",
        value_delimiter = ',',
        default_value = "WALK,TRANSIT",
        help_heading = "Routing"
    )]
    pub mode: Vec<String>,
    /// The mode from the last stop to the destination: WALK, BICYCLE or CAR
    #[arg(
        long,
        value_name = "MODE",
        default_value = "WALK",
        help_heading = "Routing"
    )]
    pub mode_egress: String,
    #[command(flatten)]
    pub transit: TransitArgs,
    /// Criteria: TIME, FARE, TRANSFERS, WALK [default: all, FARE only with
    /// --fare-structure]
    #[arg(
        long,
        value_name = "CRITERIA",
        value_delimiter = ',',
        help_heading = "Options"
    )]
    pub criteria: Option<Vec<String>>,
    /// Most options per pair (Inf: all)
    #[arg(
        long,
        value_name = "N",
        default_value = "Inf",
        help_heading = "Options"
    )]
    pub max_options: String,
    /// Seconds to which walking is rounded when trips are compared
    #[arg(long, value_name = "S", default_value = "60", help_heading = "Options")]
    pub walk_resolution: String,
    #[command(flatten)]
    pub street: StreetArgs,
    #[command(flatten)]
    pub fares: FareArgs,
    #[command(flatten)]
    pub scenario: ScenarioArgs,
    /// Write the legs of each option to this file (CSV, or GeoJSON with
    /// --format geojson)
    #[arg(long, value_name = "FILE", help_heading = "Output")]
    pub legs: Option<PathBuf>,
    /// Write each option's departure minutes to this CSV file
    #[arg(long, value_name = "FILE", help_heading = "Output")]
    pub departures: Option<PathBuf>,
    /// Leave the lines of the legs out
    #[arg(long, help_heading = "Output")]
    pub drop_geometry: bool,
    /// Add the OSM ways, street edges and stops of each leg
    #[arg(long, help_heading = "Output")]
    pub osm_link_ids: bool,
    /// Format of --legs: CSV (lines as WKT) or GeoJSON
    #[arg(long, value_enum, default_value = "csv", help_heading = "Output")]
    pub format: Format,
    #[command(flatten)]
    pub out: OutputArgs,
    /// Write one CSV file per pair to this existing directory, as r5r's
    /// output_dir
    #[arg(long, value_name = "DIR", help_heading = "Output", conflicts_with_all = ["output", "legs", "departures"])]
    pub output_dir: Option<PathBuf>,
}

#[derive(Args)]
pub struct SnapArgs {
    #[command(flatten)]
    pub net: NetworkArgs,
    /// Points: CSV with id, lon, lat
    #[arg(long, value_name = "FILE.csv")]
    pub points: PathBuf,
    /// Metres within which to look for a street
    #[arg(long, value_name = "M", default_value = "1600")]
    pub radius: String,
    /// The mode the street must allow: WALK, BICYCLE or CAR
    #[arg(long, default_value = "WALK")]
    pub mode: String,
    /// CSV, or GeoJSON of the snapped points
    #[arg(long, value_enum, default_value = "csv")]
    pub format: Format,
    #[command(flatten)]
    pub out: OutputArgs,
}

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Directed street edges, with their lines
    Edges,
    /// Street vertices
    Vertices,
    /// Transit stops
    Stops,
    /// One line per route and trip pattern
    Routes,
    /// The box of the street network
    Bbox,
}

#[derive(Args)]
pub struct NetworkExportArgs {
    #[command(flatten)]
    pub net: NetworkArgs,
    /// What to export
    #[arg(long, value_enum, default_value = "edges")]
    pub layer: Layer,
    /// GeoJSON, or CSV (lines as WKT, points as lon/lat)
    #[arg(long, value_enum, default_value = "geojson")]
    pub format: Format,
    #[command(flatten)]
    pub out: OutputArgs,
}

#[derive(Subcommand)]
pub enum FaresCommand {
    /// A fare structure for a network where every ride and every discounted
    /// transfer costs --base-fare (setup_fare_structure())
    #[command(
        after_help = "Example:\n  peabiru fares setup data/poa --base-fare 4.8 --by MODE -o fares.zip"
    )]
    Setup(FaresSetupArgs),
    /// Check a fare structure (.zip) and print its settings and tables, or
    /// write its CSV files to a directory (read_fare_structure())
    #[command(
        after_help = "Examples:\n  peabiru fares read fares.zip\n  peabiru fares read fares.zip --output-dir fares/"
    )]
    Read(FaresReadArgs),
    /// Make a fare structure (.zip) from a directory of its CSV files,
    /// checked and written as r5r writes it (write_fare_structure())
    #[command(
        after_help = "Example:\n  peabiru fares write fares/ -o fares.zip --network data/poa"
    )]
    Write(FaresWriteArgs),
}

#[derive(Args)]
pub struct FaresSetupArgs {
    #[command(flatten)]
    pub net: NetworkArgs,
    /// The fare of every ride and discounted transfer
    #[arg(long, value_name = "FARE")]
    pub base_fare: String,
    /// One fare type per MODE, AGENCY_ID or AGENCY_NAME, or a single GENERIC one
    #[arg(long, default_value = "MODE")]
    pub by: String,
    /// The .zip file to write
    #[arg(long, short, value_name = "FILE.zip")]
    pub output: PathBuf,
}

#[derive(Args)]
pub struct FaresReadArgs {
    /// The fare structure (.zip)
    #[arg(value_name = "FILE.zip")]
    pub file: PathBuf,
    /// Encoding of the CSV files: UTF-8, Latin-1 or unknown
    #[arg(long, default_value = "UTF-8")]
    pub encoding: String,
    /// Also check it against a network's routes
    #[command(flatten)]
    pub check: CheckNetwork,
    /// Write the five CSV files of the structure to this directory
    #[arg(long, value_name = "DIR")]
    pub output_dir: Option<PathBuf>,
}

#[derive(Args)]
pub struct CheckNetwork {
    /// A network (directory or .pbf) to check the structure's routes against
    #[arg(long, value_name = "NETWORK")]
    pub network: Option<PathBuf>,
    /// GTFS feeds of a .pbf --network
    #[arg(long, value_name = "FEED.zip")]
    pub gtfs: Vec<PathBuf>,
}

#[derive(Args)]
pub struct FaresWriteArgs {
    /// A directory with the structure's CSV files (global_settings.csv,
    /// fares_per_type.csv, fares_per_transfer.csv, fares_per_route.csv)
    #[arg(value_name = "DIR")]
    pub dir: PathBuf,
    /// Encoding of the CSV files: UTF-8, Latin-1 or unknown
    #[arg(long, default_value = "UTF-8")]
    pub encoding: String,
    #[command(flatten)]
    pub check: CheckNetwork,
    /// The .zip file to write
    #[arg(long, short, value_name = "FILE.zip")]
    pub output: PathBuf,
}
