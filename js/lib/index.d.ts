// Type declarations of the peabiru package (lib/index.js), written by hand.

/** Version of this package. */
export declare const version: string;

/**
 * Missing value (NA) in the `Int32Array` columns of the results:
 * -2147483648, the smallest 32-bit integer.
 */
export declare const NA_INT32: -2147483648;

/** Where the WebAssembly module can be loaded from. */
export type InitInput =
  | string
  | URL
  | Request
  | Response
  | ArrayBuffer
  | ArrayBufferView
  | WebAssembly.Module
  | Promise<Response | ArrayBuffer | ArrayBufferView | WebAssembly.Module>;

/**
 * Load and compile the WebAssembly module (once; later calls return at
 * once). `buildNetwork()` calls it, so it is needed only to load the module
 * from somewhere other than next to this file, or before `coreVersion()`.
 */
export declare function init(input?: InitInput): Promise<void>;

/**
 * Load the WebAssembly module synchronously from its bytes or a compiled
 * module. Some browsers do not compile large modules synchronously on the
 * main thread: prefer `init()` there.
 */
export declare function initSync(module: ArrayBuffer | ArrayBufferView | WebAssembly.Module): void;

/** Version of the Rust routing core; throws before the module is loaded. */
export declare function coreVersion(): string;

/**
 * An option of r5r that peabiru does not support yet. peabiru never ignores
 * an argument it cannot honour: requesting it throws this error.
 */
export declare class UnsupportedError extends Error {
  constructor(message: string);
  readonly name: "UnsupportedError";
}

/** Bytes of a file. */
export type Bytes = Uint8Array | ArrayBuffer | ArrayBufferView;

/** One GTFS feed: the `.zip` file's name and its bytes. */
export interface GtfsFeed {
  /**
   * The file name, such as `"poa_eptc.zip"`. It names the feed in messages
   * and, when the feed has no `feed_info.feed_id`, gives its feed id (the
   * name without `.zip`). Names must be distinct.
   */
  name: string;
  data: Bytes;
}

/** Receives the warnings r5r would give; by default `console.warn`. */
export type WarningHandler = (message: string) => void;

export interface BuildNetworkInput {
  /** The OpenStreetMap extract (`.pbf` file). */
  pbf: Bytes;
  /**
   * The GTFS feeds, loaded in this order (r5r loads them in the order the
   * operating system lists its data directory; see the README).
   */
  gtfs?: GtfsFeed[];
  onWarning?: WarningHandler;
  /**
   * Workers to route on. `1` (default): no workers, everything runs on the
   * calling thread. A whole number above 1: a pool of that many workers
   * (Web Workers in browsers, `worker_threads` in Node), each holding its
   * own copy of the network, all built at once. `0`, `null` or `Infinity`:
   * one worker per core (`navigator.hardwareConcurrency`, or Node's
   * `os.availableParallelism()`). Each worker costs about the memory of a
   * network built on the calling thread (see the README).
   */
  nThreads?: number | null;
  /**
   * Route in workers even with one thread, so that a call never blocks
   * the calling thread and can be aborted (default: `true` when
   * `nThreads` is given and is not 1, else `false`; must not be `false`
   * with more than one thread).
   */
  useWorkers?: boolean;
}

/** Sizes of a network. */
export interface NetworkInfo {
  vertices: number;
  edges: number;
  /** The GTFS feed ids, in loading order. */
  feeds: string[];
  stops: number;
  patterns: number;
  services: number;
  /** Whether any trip is frequency-based. */
  frequencies: boolean;
}

/**
 * A transport network: the street network and, when GTFS feeds were given,
 * the public transport layer, held in the memory of the WebAssembly module
 * (or, for a network with workers, in each worker) until `free()`. Built by
 * `buildNetwork()`; it cannot be sent to another Worker (build it there).
 */
export declare class Network {
  private constructor();
  /**
   * Release the network's memory and stop its workers; calls still running
   * on them reject. The network cannot be used afterwards. In Node, idle
   * workers do not keep a program alive, so a program may also end without
   * calling it.
   */
  free(): void;
  /** Number of workers of the network; 0 when it routes on the calling thread. */
  readonly workers: number;
  info(): NetworkInfo;
  /**
   * Services in the network and services running on `date` (a Date, read
   * in local time, or a string `"YYYY-MM-DD"`).
   */
  transitAvailability(date: Date | string): Promise<{ services: number; active: number }>;
  /** Warnings for the GTFS feeds that r5r refuses to build a network from. */
  r5rWarnings(): string[];
}

/**
 * Build a transport network from an OpenStreetMap extract and zero or more
 * GTFS feeds, as r5r's `build_network()` does from a directory (without
 * elevation). Loads the WebAssembly module first if needed.
 */
export declare function buildNetwork(input: BuildNetworkInput): Promise<Network>;

/** Numbers as a plain array or any typed array. */
export type Numbers = ArrayLike<number | null | undefined>;

/**
 * Points: columns `id`, `lon` and `lat` (WGS 84), or an array of
 * `{id, lon, lat}` objects. Ids that are not strings are converted with
 * `String()` (with a warning); points with missing coordinates (`null`,
 * `NaN`) are not routed (with a warning); duplicated ids are an error.
 * For `accessibility()`, the destinations also hold the opportunity
 * columns (or properties).
 */
export type Points =
  | { id: ArrayLike<string | number | null>; lon: Numbers; lat: Numbers; [column: string]: unknown }
  | Array<{ id: string | number | null; lon: number | null; lat: number | null; [column: string]: unknown }>;

/**
 * A date and clock time: a `Date` (its local date and clock time in the
 * JavaScript runtime's time zone), or a string `"YYYY-MM-DD HH:MM:SS"`
 * (also with `T`, or without seconds), taken as it reads, which is the
 * safer choice.
 */
export type DateTime = Date | string;

/** Options shared by every routing function, with r5r's defaults. */
export interface RoutingOptions {
  /**
   * `"WALK"` (default), `"BICYCLE"` or `"CAR"` alone, or public transport:
   * `"TRANSIT"` (every transit mode) or any of `"TRAM"`, `"SUBWAY"`,
   * `"RAIL"`, `"BUS"`, `"FERRY"`, `"CABLE_CAR"`, `"GONDOLA"`,
   * `"FUNICULAR"`, with at most one street mode to reach it (walking by
   * default; by bicycle or car the trip goes on on foot from where the
   * vehicle got, as in R5).
   */
  mode?: string | string[];
  /** `"WALK"` (default), `"BICYCLE"` or `"CAR"`: from public transport to the destinations. */
  modeEgress?: string;
  /** Minutes; `Infinity` (default) means the trip limit. */
  maxWalkTime?: number;
  /** Minutes by bicycle to or from public transport, or alone; `Infinity` (default) means the trip limit. */
  maxBikeTime?: number;
  /** Minutes by car to or from public transport, or alone; `Infinity` (default) means the trip limit. */
  maxCarTime?: number;
  /** Minutes (default 120). */
  maxTripDuration?: number;
  /** km/h (default 3.6). */
  walkSpeed?: number;
  /** km/h (default 12). */
  bikeSpeed?: number;
  /** Most transit vehicles boarded (default 3). */
  maxRides?: number;
  /** Highest level of traffic stress a cyclist rides on, 1 to 4 (default 2). */
  maxLts?: number;
  /** Not supported: must be null (default). */
  newCarspeeds?: null;
  /** Not supported: must be 1 (default). */
  carspeedScale?: number;
  /** Not supported: must be null (default). */
  newLts?: null;
  /** Monte Carlo draws per minute for frequency-based GTFS (default 5). */
  drawsPerMinute?: number;
  /**
   * Seed of the Monte Carlo draws. With frequency-based GTFS the results
   * are reproducible (and equal to r5r's) only with a seed; without one
   * (null, the default) a random seed is drawn for each call.
   */
  seed?: number | null;
  /**
   * Workers of the network this call uses: a whole number >= 1, or null or
   * Infinity (default) for all of them. More than the network has means
   * all it has; a network without workers routes on the calling thread
   * whatever this is. The origins are shared out in contiguous blocks, one
   * per worker; the results do not depend on how.
   */
  nThreads?: number | null;
  /**
   * Stops the call, which then rejects with the signal's reason (an
   * `AbortError` `DOMException` for `controller.abort()`). On a network
   * with workers, the call's workers are stopped at once and replaced, in
   * the background, by new ones built from the same input; the network
   * stays usable (later calls wait for the new workers). On the calling
   * thread a running call cannot be interrupted: the signal is checked only
   * before it starts.
   */
  signal?: AbortSignal | null;
  /** Checked as in r5r; has no effect. */
  verbose?: boolean;
  /** Checked as in r5r; peabiru shows no progress. */
  progress?: boolean;
  /** Not supported: must be null (default). */
  outputDir?: string | null;
  onWarning?: WarningHandler;
}

export interface TravelTimeMatrixOptions extends RoutingOptions {
  /** Required with transit; the current time otherwise (default null). */
  departureDatetime?: DateTime | null;
  /** Minutes of departure times, from `departureDatetime` (default 10). */
  timeWindow?: number;
  /** 1 to 5 distinct values from 1 to 99 (default 50). */
  percentiles?: number | number[];
  /**
   * A fare structure (see `setupFareStructure()`), or null (default): no
   * fares. With one, public transport is routed with R5's multi-criteria
   * router (McRAPTOR) over `timeWindow * drawsPerMinute` sampled departure
   * times, as r5r does.
   */
  fareStructure?: FareStructure | null;
  /** The highest fare of a trip (default Infinity); a finite value needs a `fareStructure`. */
  maxFare?: number;
}

/**
 * Columns of a travel-time matrix: one row per origin and destination
 * reached, origin-major in input order. `travel_time_pXX` (one per
 * percentile, ascending, two digits: `travel_time_p05`) holds whole
 * minutes, `NA_INT32` where r5r gives NA.
 */
export interface TravelTimeMatrix {
  from_id: (string | null)[];
  to_id: (string | null)[];
  [travelTime: `travel_time_p${string}`]: Int32Array;
}

/** Travel times between origins and destinations, as r5r's `travel_time_matrix()`. */
export declare function travelTimeMatrix(
  network: Network,
  origins: Points,
  destinations: Points,
  options?: TravelTimeMatrixOptions,
): Promise<TravelTimeMatrix>;

export interface AccessibilityOptions extends TravelTimeMatrixOptions {
  /** Opportunity columns of the destinations (default "opportunities"). Missing values count as 0. */
  opportunitiesColnames?: string | string[];
  /** `"step"` (default), `"exponential"`, `"fixed_exponential"`, `"linear"` or `"logistic"`. */
  decayFunction?: "step" | "exponential" | "fixed_exponential" | "linear" | "logistic";
  /** 1 to 12 distinct minutes from 1 to 120; null (default) only with `"fixed_exponential"`. */
  cutoffs?: number | number[] | null;
  /**
   * The decay constant (`"fixed_exponential"`, between 0 and 1), the width
   * in whole minutes (`"linear"`, 1 to 59) or the standard deviation in
   * minutes (`"logistic"`, at least 1); null (default) otherwise.
   */
  decayValue?: number | null;
}

/**
 * Columns of an accessibility table: one row per origin, opportunity,
 * percentile and cutoff, in that order. No `cutoff` with
 * `"fixed_exponential"`.
 */
export interface AccessibilityTable {
  id: (string | null)[];
  opportunity: string[];
  percentile: Int32Array;
  cutoff?: Int32Array;
  accessibility: Float64Array;
}

/** Access to opportunities, as r5r's `accessibility()`. */
export declare function accessibility(
  network: Network,
  origins: Points,
  destinations: Points,
  options?: AccessibilityOptions,
): Promise<AccessibilityTable>;

export interface ExpandedTravelTimeMatrixOptions extends RoutingOptions {
  /** Required (transit only). */
  departureDatetime?: DateTime | null;
  /** Minutes of departure times, from `departureDatetime` (default 10). */
  timeWindow?: number;
  /** Break each trip down into its parts (default false). */
  breakdown?: boolean;
}

export interface ArrivalTravelTimeMatrixOptions extends RoutingOptions {
  /** Required: the time by which trips must arrive. */
  arrivalDatetime: DateTime;
  /** Break each trip down into its parts (default false). */
  breakdown?: boolean;
}

/**
 * Columns of an expanded or arrival matrix, as r5r's: times are minutes
 * with one decimal, `NaN` for trips longer than `maxTripDuration` (whose
 * `routes` are null and `n_rides` `NA_INT32`). `departure_time` is
 * `"HH:MM:SS"` (hours not wrapped at 24); `routes` holds the route ids
 * joined by `"|"`, or `"[WALK]"`. The breakdown columns are present only
 * with `breakdown: true`.
 */
export interface ExpandedMatrix {
  from_id: (string | null)[];
  to_id: (string | null)[];
  departure_time: string[];
  draw_number: Int32Array;
  access_time?: Float64Array;
  wait_time?: Float64Array;
  ride_time?: Float64Array;
  transfer_time?: Float64Array;
  egress_time?: Float64Array;
  routes: (string | null)[];
  n_rides?: Int32Array;
  total_time: Float64Array;
}

/**
 * Minute-by-minute travel times by public transport, as r5r's
 * `expanded_travel_time_matrix()`. Street modes alone throw
 * `UnsupportedError`.
 */
export declare function expandedTravelTimeMatrix(
  network: Network,
  origins: Points,
  destinations: Points,
  options?: ExpandedTravelTimeMatrixOptions,
): Promise<ExpandedMatrix>;

/**
 * The latest trip that arrives by `arrivalDatetime`, as r5r's
 * `arrival_travel_time_matrix()`. Street modes alone throw
 * `UnsupportedError`.
 */
export declare function arrivalTravelTimeMatrix(
  network: Network,
  origins: Points,
  destinations: Points,
  options: ArrivalTravelTimeMatrixOptions,
): Promise<ExpandedMatrix>;

export interface DetailedItinerariesOptions extends RoutingOptions {
  /** Required with public transport. */
  departureDatetime?: DateTime | null;
  /** Minutes of departure times, from `departureDatetime` (default 10). */
  timeWindow?: number;
  /** With `shortestPath: false`, how many minutes slower than the best an alternative may be; must be 0 (the default) with `shortestPath: true` or a `fareStructure`. */
  suboptimalMinutes?: number;
  /** Only the fastest itinerary (default true); `false` gives McRAPTOR's alternatives, one per sequence of routes. */
  shortestPath?: boolean;
  /** Route from every origin to every destination (default false: pair them row by row). */
  allToAll?: boolean;
  /** A fare structure (default null): trips routed with fares, and fare columns in the result. */
  fareStructure?: FareStructure | null;
  /** The highest fare of a trip (default Infinity). */
  maxFare?: number;
  /** Plain rows instead of GeoJSON features (default false). */
  dropGeometry?: boolean;
  /** `true` throws `UnsupportedError` (not supported yet). */
  osmLinkIds?: boolean;
}

/**
 * One leg of an itinerary, as r5r's row: durations and waits are minutes
 * with one decimal, distances metres; `route` is the GTFS `route_id` of a
 * ride, empty for a street leg.
 */
export interface ItineraryRow {
  from_id: string | null;
  from_lat: number;
  from_lon: number;
  to_id: string | null;
  to_lat: number;
  to_lon: number;
  option: number;
  departure_time: string;
  total_duration: number;
  total_distance: number;
  /** With a fare structure only. */
  total_fare?: number;
  segment: number;
  mode: string;
  /** With a fare structure only: the fare paid up to the end of the leg. */
  cumulative_fare?: number;
  segment_duration: number;
  wait: number;
  distance: number;
  route: string;
}

/** A leg with its line (longitude, latitude), as a GeoJSON feature. */
export interface ItineraryFeature {
  type: "Feature";
  properties: ItineraryRow;
  geometry: { type: "LineString"; coordinates: [number, number][] };
}

/**
 * Detailed itineraries between origin-destination pairs, as r5r's
 * `detailed_itineraries()`: the direct trip by the street mode and, with
 * public transport, the fastest trip over the window (or, with
 * `shortestPath: false`, McRAPTOR's alternatives). One
 * row per leg, as a GeoJSON `FeatureCollection` or, with `dropGeometry`,
 * plain rows; pairs without an itinerary have none.
 */
export declare function detailedItineraries(
  network: Network,
  origins: Points,
  destinations: Points,
  options?: DetailedItinerariesOptions,
): Promise<{ type: "FeatureCollection"; features: ItineraryFeature[] } | ItineraryRow[]>;

/**
 * A fare structure, r5r's object (see `setupFareStructure()`): the global
 * settings, three tables as objects of columns, and r5r's debug settings.
 */
export interface FareStructure {
  /** Most transfers with a discount (default 1; null where r5r has NA). */
  max_discounted_transfers: number | null;
  /** Minutes between the boardings of a discounted transfer (default 120). */
  transfer_time_allowance: number | null;
  /** Highest fare of a trip (Infinity: no cap). */
  fare_cap: number;
  fares_per_type: {
    type: (string | null)[];
    unlimited_transfers: boolean[];
    allow_same_route_transfer: boolean[];
    use_route_fare: boolean[];
    fare: number[];
  };
  fares_per_transfer: { first_leg: (string | null)[]; second_leg: (string | null)[]; fare: number[] };
  fares_per_route: {
    agency_id: (string | null)[];
    agency_name: (string | null)[];
    route_id: (string | null)[];
    route_short_name: (string | null)[];
    route_long_name: (string | null)[];
    mode: (string | null)[];
    route_fare: number[];
    fare_type: (string | null)[];
  };
  /** r5r 2.5.0 writes no debug file whatever these say, and neither does peabiru. */
  debug_settings: { output_file: string; trip_info: string };
}

export interface SetupFareStructureOptions {
  /** `"MODE"` (default), `"AGENCY_ID"`, `"AGENCY_NAME"` or `"GENERIC"`. */
  by?: string;
  /** A `.csv` path, kept in `debug_settings` (null by default). */
  debugPath?: string | null;
  /** `"MODE"`, `"ROUTE"` or `"MODE_ROUTE"`, kept in `debug_settings`. */
  debugInfo?: string | null;
}

/**
 * A fare structure as r5r's `setup_fare_structure()`: every route, fare type
 * and transfer between two types costs `baseFare` (read as a Java float).
 */
export declare function setupFareStructure(
  network: Network,
  baseFare: number,
  options?: SetupFareStructureOptions,
): Promise<FareStructure>;

/**
 * A fare structure read from the bytes of a `.zip` file, as r5r's
 * `read_fare_structure()` (after `init()` or `buildNetwork()`).
 */
export declare function readFareStructure(
  bytes: Uint8Array | ArrayBuffer | ArrayBufferView,
  encoding?: "UTF-8" | "unknown" | "Latin-1",
): FareStructure;

/**
 * The bytes of a fare structure `.zip` file: the CSV files r5r's
 * `write_fare_structure()` writes, stored uncompressed.
 */
export declare function writeFareStructure(fareStructure: FareStructure): Uint8Array;

export interface ParetoFrontierOptions
  extends Omit<RoutingOptions, "drawsPerMinute" | "newCarspeeds" | "carspeedScale" | "newLts"> {
  /** Required with transit. */
  departureDatetime?: DateTime | null;
  /** Minutes of departure times, from `departureDatetime` (default 10), sampled once a minute. */
  timeWindow?: number;
  /** 1 to 5 distinct values from 1 to 99 (default 50). */
  percentiles?: number | number[];
  /** Required. */
  fareStructure?: FareStructure;
  /** Required: the fare cutoffs, at least 0, distinct. */
  fareCutoffs?: number | number[];
}

/**
 * Columns of a Pareto frontier: one row per origin, destination,
 * percentile and fare cutoff at which the travel time changes.
 * `monetary_cost` is the cutoff as R5 reads it, a Java float.
 */
export interface ParetoFrontierTable {
  from_id: (string | null)[];
  to_id: (string | null)[];
  percentile: Int32Array;
  travel_time: Int32Array;
  monetary_cost: Float64Array;
}

/** The trade-off between travel time and fare, as r5r's `pareto_frontier()`. */
export declare function paretoFrontier(
  network: Network,
  origins: Points,
  destinations: Points,
  options?: ParetoFrontierOptions,
): Promise<ParetoFrontierTable>;

/**
 * Options of `isochrone()`, with r5r's defaults: those of the routing
 * functions that r5r's `isochrone()` takes (no fares, scenarios or
 * `outputDir`), plus its own.
 */
export interface IsochroneOptions
  extends Omit<RoutingOptions, "newCarspeeds" | "carspeedScale" | "newLts" | "outputDir" | "maxTripDuration"> {
  /** As in the other functions, but `"transit"` by default (case does not matter). */
  mode?: string | string[];
  /** `"walk"` by default (case does not matter). */
  modeEgress?: string;
  /**
   * Minutes that delimit the isochrones (default `[0, 15, 30]`): finite and
   * at least 0, sorted and made distinct, 0 added; one must be above 0.
   */
  cutoffs?: number | number[];
  /** Web Mercator zoom level, 9 to 12, of the grid polygons are made from (default 10). */
  zoom?: number;
  /** `true` (default) for polygons, `false` for the street segments reached. */
  polygonOutput?: boolean;
  /** Required with transit; the current time otherwise (default null). */
  departureDatetime?: DateTime | null;
  /** Minutes of departure times, from `departureDatetime` (default 10). */
  timeWindow?: number;
  /**
   * 1 to 5 distinct values from 1 to 99: one set of polygons per
   * percentile; null (default) means 50. Must be null for lines.
   */
  percentiles?: number | number[] | null;
  /** Ignored, as in r5r: the trip limit comes from the cutoffs. */
  maxTripDuration?: number;
  /** Deprecated in r5r and ignored: giving it only warns. */
  sampleSize?: unknown;
}

/** Properties of a polygon isochrone. */
export interface IsochronePolygonProperties {
  id: string | null;
  /** The cutoff, minutes. */
  isochrone: number;
  /** Such as `"p50"`. */
  percentile: string;
}

/** Properties of a line isochrone: the segment as r5r's `street_network_to_sf()` lists it. */
export interface IsochroneLineProperties {
  id: string | null;
  /** The directed edge index, as a string (as r5r gives it). */
  edge_index: string;
  osm_id: number;
  /** The smallest cutoff at or above the travel time, minutes. */
  isochrone: number;
  travel_time_p50: number;
  from_vertex: number;
  to_vertex: number;
  street_class: "MOTORWAY" | "PRIMARY" | "SECONDARY" | "TERTIARY" | "OTHER";
  /** Metres. */
  length: number;
  walk: boolean;
  car: boolean;
  /** km/h. */
  car_speed: number;
  bicycle: boolean;
  bicycle_lts: number;
}

/** A GeoJSON feature (coordinates `[lon, lat]`, WGS 84). */
export interface Feature<G, P> {
  type: "Feature";
  properties: P;
  geometry: G;
}

/** A GeoJSON MultiPolygon: polygons of rings (shell first, then holes), every ring closed. */
export interface MultiPolygon {
  type: "MultiPolygon";
  coordinates: [number, number][][][];
}

/** A GeoJSON LineString. */
export interface LineString {
  type: "LineString";
  coordinates: [number, number][];
}

/** A GeoJSON FeatureCollection. */
export interface FeatureCollection<G, P> {
  type: "FeatureCollection";
  features: Feature<G, P>[];
}

/**
 * Isochrones, as r5r's `isochrone()`: with `polygonOutput: true` (the
 * default) one `MultiPolygon` per origin, percentile (ascending) and cutoff
 * (descending), empty when nothing is reached; with `polygonOutput: false`
 * one `LineString` per street segment reached, by origin, then band and
 * travel time descending. The polygons are those r5r makes with isoband
 * before `sf::st_make_valid()`, which is not reproduced.
 */
export declare function isochrone(
  network: Network,
  origins: Points,
  options: IsochroneOptions & { polygonOutput: false },
): Promise<FeatureCollection<LineString, IsochroneLineProperties>>;
export declare function isochrone(
  network: Network,
  origins: Points,
  options?: IsochroneOptions,
): Promise<FeatureCollection<MultiPolygon, IsochronePolygonProperties>>;

/** Options of `findSnap()`, with r5r's defaults. */
export interface FindSnapOptions {
  /** The largest distance in metres within which to snap. Default 1600. */
  radius?: number;
  /** The mode whose streets the points snap to. Default `"WALK"`. */
  mode?: "WALK" | "BICYCLE" | "CAR";
  /** Called with each warning (about the points); `console.warn` by default. */
  onWarning?: WarningHandler;
  /** Aborts the call. */
  signal?: AbortSignal;
}

/** Where points snap to the streets: one row per point. */
export interface SnapTable {
  point_id: (string | null)[];
  lat: Float64Array;
  lon: Float64Array;
  /** Where the point snaps; NaN where it does not. */
  snap_lat: Float64Array;
  snap_lon: Float64Array;
  /** Metres from the point to where it snaps (haversine on R5's sphere of radius 6,367,450 m); NaN where it does not snap. */
  distance: Float64Array;
  found: boolean[];
}

/**
 * Where points snap to the street network, as r5r's `find_snap()`: the
 * nearest point of a street the mode may use, first within 300 metres and
 * then within `radius`. A point that does not snap is not routed.
 */
export declare function findSnap(network: Network, points: Points, options?: FindSnapOptions): Promise<SnapTable>;

/** A GeoJSON Polygon: rings of `[lon, lat]`, shell first, every ring closed. */
export interface Polygon {
  type: "Polygon";
  coordinates: [number, number][][];
}

/** A GeoJSON Point. */
export interface Point {
  type: "Point";
  coordinates: [number, number];
}

/**
 * The bounding box of the street network, as r5r's
 * `street_network_bbox()`: R5's envelope of the street layer, which covers
 * every OpenStreetMap node of the ways streets were made from. `"polygon"`
 * (the default) gives a Polygon with sf's ring, `"bbox"` the GeoJSON bbox
 * `[xmin, ymin, xmax, ymax]`, `"vector"` an object.
 */
export declare function streetNetworkBbox(network: Network, output: "vector"): Promise<{ xmin: number; ymin: number; xmax: number; ymax: number }>;
export declare function streetNetworkBbox(network: Network, output: "bbox"): Promise<[number, number, number, number]>;
export declare function streetNetworkBbox(network: Network, output?: "polygon"): Promise<Polygon>;

/** Properties of a street vertex. */
export interface StreetVertexProperties {
  /** The vertex index in the network. */
  index: number;
  park_and_ride: boolean;
  /** Always false: bike sharing is not loaded. */
  bike_sharing: boolean;
}

/** Properties of a directed street edge, as r5r's `street_network_to_sf()` lists it. */
export interface StreetEdgeProperties {
  edge_index: number;
  osm_id: number;
  from_vertex: number;
  to_vertex: number;
  street_class: "MOTORWAY" | "PRIMARY" | "SECONDARY" | "TERTIARY" | "OTHER";
  /** Metres. */
  length: number;
  walk: boolean;
  car: boolean;
  /** km/h. */
  car_speed: number;
  bicycle: boolean;
  bicycle_lts: number;
}

/** Options of the functions that only read the network. */
export interface InspectOptions {
  /** Aborts the call. */
  signal?: AbortSignal;
}

/**
 * The street network as GeoJSON, as r5r's `street_network_to_sf()`: the
 * vertices at the ends of street edges and every directed street edge
 * (the connectors of transit stops and park-and-rides left out), in r5r's
 * order.
 */
export declare function streetNetworkToSf(network: Network, options?: InspectOptions): Promise<{
  vertices: FeatureCollection<Point, StreetVertexProperties>;
  edges: FeatureCollection<LineString, StreetEdgeProperties>;
}>;

/** Properties of a transit stop. */
export interface TransitStopProperties {
  stop_index: number;
  /** Prefixed by the feed id (`feed:stop`). */
  stop_id: string;
  /** null when blank. */
  stop_name: string | null;
  linked_to_street: boolean;
}

/** Properties of a transit route (one per trip pattern); null where blank. */
export interface TransitRouteProperties {
  agency_id: string | null;
  agency_name: string | null;
  route_id: string;
  long_name: string | null;
  short_name: string | null;
  /** R5's mode of the GTFS route_type, such as `"BUS"`. */
  mode: string;
}

/**
 * The public transport network as GeoJSON, as r5r's
 * `transit_network_to_sf()`. A stop not linked to the streets has a null
 * geometry (r5r: `POINT EMPTY`). There is one route per trip pattern; its
 * line is the GTFS shape of the trip that made the pattern or, without
 * one, the positions of its linked stops. Patterns without a line are left
 * out, and a line whose points are all the same is the Point
 * `sf::st_make_valid()` makes of it in r5r.
 */
export declare function transitNetworkToSf(network: Network, options?: InspectOptions): Promise<{
  stops: FeatureCollection<Point | null, TransitStopProperties>;
  routes: FeatureCollection<LineString | Point, TransitRouteProperties>;
}>;

/** Options of `checkTransitAvailability()`: none (discovery), `dates`, or `startDate` and `endDate`. */
export interface TransitAvailabilityOptions {
  /** Dates to check: `"YYYY-MM-DD"` strings or Dates (their local date). */
  dates?: string | Date | Array<string | Date>;
  /** The first date of a range to check. */
  startDate?: string | Date;
  /** The last date of a range to check. */
  endDate?: string | Date;
  /** Aborts the call. */
  signal?: AbortSignal;
}

/** Services active on each date. */
export interface TransitAvailabilityTable {
  /** `"YYYY-MM-DD"`. */
  date: string[];
  total_services: Int32Array;
  active_services: Int32Array;
  /** `active_services / total_services`, 0 without services. */
  pct_active: Float64Array;
}

/**
 * Services active on dates, as r5r's `check_transit_availability()`. With
 * no date option, the dates with an active service used by some trip are
 * discovered from the network (ascending); `total_services` counts every
 * service, used by trips or not.
 */
export declare function checkTransitAvailability(network: Network, options?: TransitAvailabilityOptions): Promise<TransitAvailabilityTable>;

/** The errors gtfs-lib reports in GTFS feeds: one row per error. */
export interface GtfsErrorsTable {
  /** The table, without `.txt`. */
  file: (string | null)[];
  /** The record, the header being line 1; 0 for a whole table. */
  line: Int32Array;
  /** gtfs-lib's error class, such as `"DuplicateKeyError"`. */
  type: string[];
  field: (string | null)[];
  /** The key concerned, for duplicate keys and suspect stop locations. */
  id: (string | null)[];
  /** `"HIGH"` (r5r refuses the network), `"MEDIUM"`, `"LOW"` or `"UNKNOWN"`. */
  priority: string[];
}

/**
 * The errors gtfs-lib (R5's GTFS library) reports when it loads GTFS feeds,
 * as r5r's `get_gtfs_errors()` returns them (from the `gtfs_errors.csv` its
 * `build_network()` writes). A Network does not keep its feeds: pass them
 * as `buildNetwork()` takes them. Rows come feed by feed in the order
 * gtfs-lib finds them (r5r's order changes from run to run). Rejects with
 * an Error naming the cause for a feed on which gtfs-lib fails instead
 * (r5r's `build_network()` fails then too), and for a feed with GTFS-Flex
 * zones (`locations.geojson`), which peabiru does not read.
 */
export declare function getGtfsErrors(feeds: GtfsFeed[] | { gtfs: GtfsFeed[] }): Promise<GtfsErrorsTable>;
