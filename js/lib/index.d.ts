// Type declarations of the peabiru package (lib/index.js), written by hand.
//
// The examples in the comments below are run by site/js/run_examples.mjs,
// after the setup in site/js/examples/sample-api.mjs (the `network` of the
// São Paulo sample, its `pbf` and `gtfs` bytes, and three `points`).

/**
 * The version of this package, such as `"0.1.0"`.
 *
 * ```js
 * console.log(peabiru.version);
 * ```
 */
export declare const version: string;

/**
 * The missing value (NA) of the `Int32Array` columns of the results:
 * -2147483648, the smallest 32-bit integer. The `Float64Array` columns use
 * `NaN`, and the string columns `null`.
 *
 * ```js
 * const ttm = await peabiru.travelTimeMatrix(network, points, points, {
 *   mode: ["WALK", "TRANSIT"], departureDatetime: "2019-05-13 14:00:00", seed: 42, percentiles: [5, 95],
 * });
 * const reached = ttm.travel_time_p05.filter((t) => t !== peabiru.NA_INT32).length;
 * console.log(reached, "of", ttm.from_id.length);
 * ```
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
 * Load and compile the WebAssembly module, once: later calls resolve at
 * once. Without `input` the module is `pkg/peabiru_bg.wasm` next to the
 * package (read from the file in Node, fetched elsewhere, at a URL that
 * bundlers such as Vite and webpack 5 recognise). `input` loads it from
 * somewhere else: a URL (string or `URL`), a `Request` or `Response`, the
 * bytes, a compiled `WebAssembly.Module`, or a Promise of one of these.
 * `buildNetwork()` and `getGtfsErrors()` call it themselves, so it is only
 * needed to load the module from elsewhere, or before `coreVersion()`,
 * `readFareStructure()` or `writeFareStructure()`.
 *
 * ```js
 * await peabiru.init(); // or: await peabiru.init(new URL("/static/peabiru_bg.wasm", location.href))
 * console.log(peabiru.coreVersion());
 * ```
 */
export declare function init(input?: InitInput): Promise<void>;

/**
 * Load the WebAssembly module synchronously from its bytes or a compiled
 * `WebAssembly.Module` (nothing happens when it is already loaded). Some
 * browsers do not compile large modules synchronously on the main thread:
 * prefer `init()` there.
 *
 * ```js
 * peabiru.initSync(await readFile("node_modules/peabiru/pkg/peabiru_bg.wasm"));
 * console.log(peabiru.coreVersion());
 * ```
 */
export declare function initSync(module: ArrayBuffer | ArrayBufferView | WebAssembly.Module): void;

/**
 * The version of the Rust routing core compiled into the module. Throws
 * before the module is loaded (see `init()`).
 *
 * ```js
 * console.log(`package ${peabiru.version}, core ${peabiru.coreVersion()}`);
 * ```
 */
export declare function coreVersion(): string;

/**
 * The error thrown when a call asks for something peabiru does not do: an
 * `outputDir`, an expanded or arrival matrix by a street mode alone, or a
 * fare structure that names one of R5's own fare calculators (a `type`).
 * peabiru never ignores an option it cannot honour.
 *
 * ```js
 * try {
 *   await peabiru.travelTimeMatrix(network, points, points, { outputDir: "results" });
 * } catch (e) {
 *   console.log(e instanceof peabiru.UnsupportedError, e.name);
 * }
 * ```
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
   * name without `.zip`), which prefixes its stop ids. Names must be
   * distinct.
   */
  name: string;
  /** The bytes of the `.zip` file. */
  data: Bytes;
}

/** Receives each warning, as a string; by default `console.warn`. */
export type WarningHandler = (message: string) => void;

/** The input of `buildNetwork()`. */
export interface BuildNetworkInput {
  /** The OpenStreetMap extract: the bytes of a `.pbf` file. */
  pbf: Bytes;
  /**
   * The GTFS feeds, loaded in this order (default: none, a street network
   * only). A feed with errors of priority `HIGH` (see `getGtfsErrors()`)
   * is refused: the build rejects with an `Error` naming it.
   */
  gtfs?: GtfsFeed[];
  /**
   * An elevation raster, the bytes of a GeoTIFF file (default: none):
   * walking and cycling times then get the slope costs of `elevation`.
   * One band, north up, uncompressed or compressed with LZW, Deflate or
   * PackBits, in WGS 84 longitude and latitude (EPSG:4326), SIRGAS 2000
   * (4674), SAD69 (4618), Corrego Alegre (4225), their UTM zones used in
   * Brazil, SIRGAS 2000 / Brazil Polyconic (5880), WGS 84 / UTM (326xx,
   * 327xx) or Pseudo-Mercator (3857); other rasters are an error. Voids
   * and values beyond +/-3276.7 m are stored wrapped around, as 16-bit
   * decimetres, with a warning; in a projected raster, a street point
   * within nanometres of a pixel boundary reads the pixel Java's
   * StrictMath gives, with a warning. With a raster, `travelTimeMatrix()`
   * never routes walking trips from the destinations.
   */
  tif?: Bytes;
  /**
   * The slope cost function when `tif` is given: `"TOBLER"` (default,
   * Tobler's hiking function), `"MINETTI"` (Minetti's energy cost of
   * walking) or `"NONE"`, in any case. Without `tif` it is `"NONE"`.
   */
  elevation?: string;
  /** Receives the warnings of the build (about the raster); by default `console.warn`. */
  onWarning?: WarningHandler;
  /**
   * Workers to route on. `1` (default): no workers, everything runs on the
   * calling thread. A whole number above 1: a pool of that many workers
   * (Web Workers in browsers, `worker_threads` in Node), each holding its
   * own copy of the network, all built at once. `0`, `null` or `Infinity`:
   * one worker per core (`navigator.hardwareConcurrency`, or Node's
   * `os.availableParallelism()`). Each worker costs about the memory of a
   * network built on the calling thread.
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

/** Sizes of a network, from `network.info()`. */
export interface NetworkInfo {
  /** Street vertices, including those of the connectors of transit stops. */
  vertices: number;
  /** Directed street edges, including the connectors of transit stops. */
  edges: number;
  /** The slope cost function in use: `"TOBLER"`, `"MINETTI"` or `"NONE"`. */
  elevation: string;
  /** The GTFS feed ids, in loading order. */
  feeds: string[];
  /** Transit stops. */
  stops: number;
  /** Trip patterns. */
  patterns: number;
  /** GTFS services (calendars). */
  services: number;
  /** Whether any trip is frequency-based. */
  frequencies: boolean;
}

/**
 * A transport network: the street network and, when GTFS feeds were given,
 * the public transport layer, held in the memory of the WebAssembly module
 * (or, for a network with workers, in each worker) until `free()`. Built by
 * `buildNetwork()`; it cannot be sent to another Worker (build it there).
 *
 * ```js
 * console.log(network.info().stops, "stops,", network.workers, "workers");
 * console.log(await network.transitAvailability("2019-05-13"));
 * network.free();
 * ```
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
  /** The sizes of the network (see `NetworkInfo`). */
  info(): NetworkInfo;
  /**
   * The number of services in the network and of those running on `date`
   * (a Date, read in local time, or a string `"YYYY-MM-DD"`): the check
   * the routing functions make on their departure date.
   */
  transitAvailability(date: Date | string): Promise<{ services: number; active: number }>;
  /**
   * The warnings of the build, which `buildNetwork()` also hands to
   * `onWarning`: those about the elevation raster (samples stored wrapped
   * around, beyond +/-3276.7 m, such as SRTM's -32768 voids, or equal to
   * the raster's no-data value; street points near a pixel boundary of a
   * projected raster).
   */
  r5rWarnings(): string[];
}

/**
 * Build a transport network from an OpenStreetMap extract, zero or more
 * GTFS feeds and an optional elevation raster, given as the bytes of the
 * files. Loads the WebAssembly module first if needed. Rejects when a file
 * cannot be read, and when a GTFS feed has errors of priority `HIGH`.
 *
 * ```js
 * const net = await peabiru.buildNetwork({ pbf, gtfs, nThreads: 2 });
 * console.log(net.info(), net.workers);
 * net.free();
 * ```
 *
 * Compatibility with r5r: `build_network()` with `elevation`, from bytes
 * rather than a directory (the feeds load in array order, where r5r uses
 * the order the operating system lists its directory; no `verbose`,
 * `temp_dir` or `overwrite`; nothing is written to disk); `nThreads` is
 * peabiru's own.
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
 * safer choice: the clock time of the GTFS feeds.
 */
export type DateTime = Date | string;

/** A table of a scenario: an object of columns, or an array of row objects. */
export type ScenarioTable = Record<string, ArrayLike<unknown>> | Array<Record<string, unknown>>;

/** A GeoJSON FeatureCollection (WGS 84). */
export interface GeoJsonFeatureCollection {
  type: "FeatureCollection";
  features: Array<{ type: "Feature"; properties: Record<string, unknown>; geometry: { type: string; coordinates: unknown } }>;
}

/** Options shared by the routing functions, with their defaults. */
export interface RoutingOptions {
  /**
   * `"WALK"` (default), `"BICYCLE"` or `"CAR"` alone, or public transport:
   * `"TRANSIT"` (every transit mode) or any of `"TRAM"`, `"SUBWAY"`,
   * `"RAIL"`, `"BUS"`, `"FERRY"`, `"CABLE_CAR"`, `"GONDOLA"`,
   * `"FUNICULAR"`, with at most one street mode to reach it (walking by
   * default; by bicycle or car, the trip goes on on foot from where the
   * vehicle is left). Case does not matter.
   */
  mode?: string | string[];
  /** `"WALK"` (default), `"BICYCLE"` or `"CAR"`: from public transport to the destinations. */
  modeEgress?: string;
  /** Minutes of walking, to or from public transport, or alone; `Infinity` (default) means the trip limit. */
  maxWalkTime?: number;
  /** Minutes by bicycle, to or from public transport, or alone; `Infinity` (default) means the trip limit. */
  maxBikeTime?: number;
  /** Minutes by car, to or from public transport, or alone; `Infinity` (default) means the trip limit. */
  maxCarTime?: number;
  /** The longest trip, in whole minutes (default 120). */
  maxTripDuration?: number;
  /** Walking speed, km/h (default 3.6). */
  walkSpeed?: number;
  /** Cycling speed, km/h (default 12). */
  bikeSpeed?: number;
  /** Most public transport vehicles boarded in a trip (default 3). */
  maxRides?: number;
  /** Highest level of traffic stress a cyclist rides on, 1 to 4 (default 2). */
  maxLts?: number;
  /**
   * A scenario of car speeds (default null): a table (an object of columns
   * or an array of rows) with `osm_id`, `max_speed` and `speed_type`
   * (`"scale"`, a factor for the car speed of the way's edges, or
   * `"km/h"`, a speed; 0 closes the way to cars), or a GeoJSON
   * FeatureCollection of Polygons or MultiPolygons with properties
   * `poly_id` (string), `scale` and `priority` (integer): each edge takes
   * the factor of the polygon it meets with the highest priority (on a
   * tie, the one it overlaps most). A speed that R5's 16-bit cm/s would
   * bring to 0 or less is an error.
   */
  newCarspeeds?: ScenarioTable | GeoJsonFeatureCollection | null;
  /** The factor for the car speeds of the edges `newCarspeeds` leaves out, at least 0 (default 1). */
  carspeedScale?: number;
  /**
   * A scenario of levels of traffic stress (default null): a table with
   * `osm_id` and `lts` (integer, 1 to 4), or a GeoJSON FeatureCollection
   * of LineStrings with properties `line_id`, `lts` (1 to 4) and
   * `priority` (not used): each edge takes the level of the closest line
   * among those whose bounding box meets its own. A scenario changes the
   * street search from the origins but not the egress from the stops; on
   * a network with elevation, a call with a scenario routes without slope
   * costs, with a warning.
   */
  newLts?: ScenarioTable | GeoJsonFeatureCollection | null;
  /** Monte Carlo draws per departure minute for frequency-based GTFS (default 5). */
  drawsPerMinute?: number;
  /**
   * Seed of the Monte Carlo draws, an integer. With frequency-based GTFS
   * the results are reproducible only with a seed; without one (null, the
   * default) a random seed is drawn for each call.
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
  /** Must be a boolean (default false); nothing is printed. */
  verbose?: boolean;
  /** Must be a boolean (default true); no progress is shown. */
  progress?: boolean;
  /** Writing the results to files: must be null (default); anything else throws `UnsupportedError`. */
  outputDir?: string | null;
  /** Receives the warnings of the call; by default `console.warn`. */
  onWarning?: WarningHandler;
}

/** Options of `travelTimeMatrix()`. */
export interface TravelTimeMatrixOptions extends RoutingOptions {
  /** Required with public transport; the current time otherwise (default null). */
  departureDatetime?: DateTime | null;
  /** Minutes of departure times, from `departureDatetime` on (default 10). */
  timeWindow?: number;
  /** 1 to 5 distinct whole numbers from 1 to 99 (default 50): the percentiles of the travel times over the window. */
  percentiles?: number | number[];
  /**
   * A fare structure (see `setupFareStructure()`), or null (default): no
   * fares. With one, public transport is routed with R5's multi-criteria
   * router (McRAPTOR) over `timeWindow * drawsPerMinute` sampled
   * departure times.
   */
  fareStructure?: FareStructure | null;
  /** The highest fare of a trip (default Infinity); a finite value needs a `fareStructure`. */
  maxFare?: number;
}

/**
 * Columns of a travel-time matrix: one row per origin and destination
 * reached, origin-major in input order. `travel_time_pXX` (one per
 * percentile, ascending, two digits: `travel_time_p05`) holds whole
 * minutes, `NA_INT32` where a percentile is not reached.
 */
export interface TravelTimeMatrix {
  from_id: (string | null)[];
  to_id: (string | null)[];
  [travelTime: `travel_time_p${string}`]: Int32Array;
}

/**
 * Travel times between every origin and every destination: on the streets
 * by one mode, or by public transport with the percentiles of the travel
 * times over the departure window (and, for frequency-based trips, over the
 * Monte Carlo draws). Every option is in `TravelTimeMatrixOptions` and
 * `RoutingOptions`. Pairs not reached within `maxTripDuration` are left
 * out.
 *
 * ```js
 * const ttm = await peabiru.travelTimeMatrix(network, points, points, {
 *   mode: ["WALK", "TRANSIT"],
 *   departureDatetime: "2019-05-13 14:00:00",
 *   percentiles: [25, 50, 75],
 *   seed: 42,
 * });
 * console.log(ttm.from_id[1], ttm.to_id[1], ttm.travel_time_p50[1]);
 * ```
 *
 * Compatibility with r5r: `travel_time_matrix()`, the same options in
 * camelCase, defaults and result columns; `outputDir` is not supported.
 */
export declare function travelTimeMatrix(
  network: Network,
  origins: Points,
  destinations: Points,
  options?: TravelTimeMatrixOptions,
): Promise<TravelTimeMatrix>;

/** Options of `accessibility()`. */
export interface AccessibilityOptions extends TravelTimeMatrixOptions {
  /** Opportunity columns of the destinations (default `"opportunities"`). Missing values count as 0, with a warning. */
  opportunitiesColnames?: string | string[];
  /** `"step"` (default), `"exponential"`, `"fixed_exponential"`, `"linear"` or `"logistic"`. */
  decayFunction?: "step" | "exponential" | "fixed_exponential" | "linear" | "logistic";
  /**
   * 1 to 12 distinct whole minutes from 1 to 120: the cutoffs of
   * `"step"`, and for the other functions the travel time at which the
   * weight is one half; null (default) only with `"fixed_exponential"`,
   * which takes none.
   */
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

/**
 * Access to opportunities from each origin: the opportunities of the
 * destinations (the columns `opportunitiesColnames`), weighted by a decay
 * function of the travel time to them and summed. Every option is in
 * `AccessibilityOptions`, `TravelTimeMatrixOptions` and `RoutingOptions`.
 *
 * ```js
 * const access = await peabiru.accessibility(network, points, points, {
 *   opportunitiesColnames: "jobs",
 *   mode: "WALK",
 *   cutoffs: [10, 20],
 * });
 * console.log(access.id, access.cutoff, access.accessibility);
 * ```
 *
 * Compatibility with r5r: `accessibility()`, the same options in
 * camelCase, defaults, decay functions and result columns (R5's exponential
 * as Apache Commons Math computes it); `outputDir` is not supported.
 */
export declare function accessibility(
  network: Network,
  origins: Points,
  destinations: Points,
  options?: AccessibilityOptions,
): Promise<AccessibilityTable>;

/** Options of `expandedTravelTimeMatrix()`. */
export interface ExpandedTravelTimeMatrixOptions extends RoutingOptions {
  /** Required: the first departure time. */
  departureDatetime?: DateTime | null;
  /** Minutes of departure times, from `departureDatetime` on (default 10). */
  timeWindow?: number;
  /** Break each trip down into its parts (default false). */
  breakdown?: boolean;
}

/** Options of `arrivalTravelTimeMatrix()`. */
export interface ArrivalTravelTimeMatrixOptions extends RoutingOptions {
  /** Required: the time by which trips must arrive. */
  arrivalDatetime: DateTime;
  /** Break each trip down into its parts (default false). */
  breakdown?: boolean;
}

/**
 * Columns of an expanded or arrival matrix: times are minutes with one
 * decimal, `NaN` for trips longer than `maxTripDuration` (whose `routes`
 * are null and `n_rides` `NA_INT32`). `departure_time` is `"HH:MM:SS"`
 * (hours not wrapped at 24); `draw_number` counts the draws of a departure
 * minute from 1; `routes` holds the route ids of the rides joined by
 * `"|"`, or `"[WALK]"` for a trip on foot. The breakdown columns are
 * present only with `breakdown: true`.
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
 * Travel times by public transport minute by minute: one row per origin,
 * destination, departure minute of the window and Monte Carlo draw,
 * instead of percentiles, with the routes taken and, with `breakdown`, the
 * parts of each trip. Every option is in `ExpandedTravelTimeMatrixOptions`
 * and `RoutingOptions`. A street mode alone throws `UnsupportedError` (use
 * `travelTimeMatrix()`).
 *
 * ```js
 * const etm = await peabiru.expandedTravelTimeMatrix(network, points, points, {
 *   mode: ["WALK", "TRANSIT"],
 *   departureDatetime: "2019-05-13 14:00:00",
 *   timeWindow: 2,
 *   seed: 42,
 *   breakdown: true,
 * });
 * console.log(etm.departure_time[0], etm.routes[0], etm.total_time[0]);
 * ```
 *
 * Compatibility with r5r: `expanded_travel_time_matrix()`, the same
 * options in camelCase, defaults and result columns; with a street mode
 * alone r5r answers with one trip from a departure time taken from the
 * clock, where peabiru throws; `outputDir` is not supported.
 */
export declare function expandedTravelTimeMatrix(
  network: Network,
  origins: Points,
  destinations: Points,
  options?: ExpandedTravelTimeMatrixOptions,
): Promise<ExpandedMatrix>;

/**
 * The latest trip by public transport that arrives by `arrivalDatetime`,
 * for each origin and destination: the departures of the
 * `maxTripDuration` minutes before it are searched, and each pair keeps the
 * one that leaves last and arrives in time. The columns are those of
 * `expandedTravelTimeMatrix()`. Every option is in
 * `ArrivalTravelTimeMatrixOptions` and `RoutingOptions`. A street mode
 * alone throws `UnsupportedError`.
 *
 * ```js
 * const atm = await peabiru.arrivalTravelTimeMatrix(network, points, points, {
 *   mode: ["WALK", "TRANSIT"],
 *   arrivalDatetime: "2019-05-13 15:00:00",
 *   seed: 42,
 * });
 * console.log(atm.from_id[0], atm.to_id[0], atm.departure_time[0], atm.total_time[0]);
 * ```
 *
 * Compatibility with r5r: `arrival_travel_time_matrix()`, the same
 * options in camelCase, defaults and result columns; peabiru throws with a
 * street mode alone; `outputDir` is not supported.
 */
export declare function arrivalTravelTimeMatrix(
  network: Network,
  origins: Points,
  destinations: Points,
  options: ArrivalTravelTimeMatrixOptions,
): Promise<ExpandedMatrix>;

/** Options of `detailedItineraries()`. */
export interface DetailedItinerariesOptions extends Omit<RoutingOptions, "drawsPerMinute" | "seed"> {
  /** Required with public transport; the current time otherwise. */
  departureDatetime?: DateTime | null;
  /** Minutes of departure times, from `departureDatetime` on (default 10). */
  timeWindow?: number;
  /**
   * With `shortestPath: false`, how many minutes after the best arrival at
   * a stop the multi-criteria search still keeps a state there, which
   * widens the set of alternatives (default 0). Must be 0 with
   * `shortestPath: true` or a `fareStructure`.
   */
  suboptimalMinutes?: number;
  /** Only the fastest itinerary of each pair (default true); `false` gives the alternatives, one per sequence of routes. */
  shortestPath?: boolean;
  /** Route from every origin to every destination (default false: pair them row by row, a single origin or destination repeated to match the other). */
  allToAll?: boolean;
  /** A fare structure (default null): trips routed with fares, and fare columns in the result. */
  fareStructure?: FareStructure | null;
  /** The highest fare of a trip (default Infinity); a finite value needs a `fareStructure`. */
  maxFare?: number;
  /** Plain rows instead of GeoJSON features (default false). */
  dropGeometry?: boolean;
  /**
   * Add the properties `osm_id_list`, `edge_id_list`, `board_stop_id` and
   * `alight_stop_id` to each row (default false; needs
   * `dropGeometry: false`): see `ItineraryRow`.
   */
  osmLinkIds?: boolean;
}

/**
 * One leg of an itinerary: durations and waits are minutes with one
 * decimal, distances metres; `route` is the GTFS `route_id` of a ride,
 * empty for a street leg.
 */
export interface ItineraryRow {
  from_id: string | null;
  from_lat: number;
  from_lon: number;
  to_id: string | null;
  to_lat: number;
  to_lon: number;
  /** The itinerary of the pair, from 1. */
  option: number;
  /** `"HH:MM:SS"`, when the itinerary leaves the origin. */
  departure_time: string;
  total_duration: number;
  total_distance: number;
  /** With a fare structure only. */
  total_fare?: number;
  /** The leg, from 1. */
  segment: number;
  /** `"WALK"`, `"BICYCLE"`, `"CAR"`, or the mode of a ride, such as `"BUS"`. */
  mode: string;
  /** With a fare structure only: the fare paid up to the end of the leg. */
  cumulative_fare?: number;
  segment_duration: number;
  wait: number;
  distance: number;
  route: string;
  /**
   * With `osmLinkIds`: the OpenStreetMap way ids of a street leg's edges,
   * as `"[id1, id2, ...]"` (`"[]"` for a ride; consecutive edges on a
   * roundabout give only their first ids, connectors none).
   */
  osm_id_list?: string;
  /** With `osmLinkIds`: the indices of those edges in the street network (`edge_index` of `streetNetworkToSf()`), as `"[i1, i2, ...]"`. */
  edge_id_list?: string;
  /** With `osmLinkIds`: a ride's boarding stop, `"<feed>:<stop_id>"`; empty for street legs. */
  board_stop_id?: string;
  /** With `osmLinkIds`: a ride's alighting stop, `"<feed>:<stop_id>"`; empty for street legs. */
  alight_stop_id?: string;
}

/** A leg with its line (longitude, latitude), as a GeoJSON feature. */
export interface ItineraryFeature {
  type: "Feature";
  properties: ItineraryRow;
  geometry: { type: "LineString"; coordinates: [number, number][] };
}

/**
 * The legs of the trips between pairs of origins and destinations: each
 * walk, ride by bicycle or car and public transport ride, with its
 * duration, wait, distance, route and line. Each pair gets its fastest
 * itinerary over the departure window (the trip by the street mode alone,
 * or by public transport, whichever is faster) or, with
 * `shortestPath: false`, the alternatives found by R5's multi-criteria
 * router (McRAPTOR). One row per leg, as a GeoJSON `FeatureCollection` or,
 * with `dropGeometry`, an array of rows; pairs without an itinerary have
 * none. Every option is in `DetailedItinerariesOptions` and
 * `RoutingOptions`. Public transport needs feeds with timetables: a
 * network with frequency-based trips is an error. The few pairs whose
 * itinerary depends on what peabiru does not reproduce (a car search
 * through a turn restriction with two or more via ways, where the order of
 * earlier searches matters; a ride along a shape cut by
 * `shape_dist_traveled`) reject with an error naming the pair.
 *
 * ```js
 * const back = { id: [...points.id].reverse(), lon: [...points.lon].reverse(), lat: [...points.lat].reverse() };
 * const legs = await peabiru.detailedItineraries(network, points, back, {
 *   mode: "BICYCLE",
 *   departureDatetime: "2019-05-13 14:00:00",
 *   dropGeometry: true,
 * });
 * console.log(legs.map((r) => `${r.from_id} > ${r.to_id}: ${r.total_duration} min, ${r.total_distance} m`));
 * ```
 *
 * Compatibility with r5r: `detailed_itineraries()`, the same options in
 * camelCase, defaults, rows and lines (GeoJSON for r5r's `sf`), with the
 * refusals above; `outputDir` is not supported.
 */
export declare function detailedItineraries(
  network: Network,
  origins: Points,
  destinations: Points,
  options?: DetailedItinerariesOptions,
): Promise<{ type: "FeatureCollection"; features: ItineraryFeature[] } | ItineraryRow[]>;

/** A criterion of `tripOptions()`. */
export type TripCriterion = "TIME" | "FARE" | "TRANSFERS" | "WALK";

export interface TripOptionsOptions extends RoutingOptions {
  /** Required with public transport. */
  departureDatetime?: DateTime | null;
  /** Minutes of departure, one at a time from `departureDatetime` (default 10). */
  timeWindow?: number;
  /**
   * Some of `"TIME"` (door-to-door minutes from the departure minute),
   * `"FARE"` (needs a `fareStructure`), `"TRANSFERS"` (rides minus one; 0
   * without public transport) and `"WALK"` (minutes on foot: walked access
   * and egress legs and walks between stops). Default null: all four with a
   * `fareStructure`, all but `"FARE"` without one.
   */
  criteria?: TripCriterion[] | TripCriterion | null;
  /** At most this many options per pair, the first in `option` order (default Infinity). */
  maxOptions?: number;
  /** Route from every origin to every destination (default false: pair them row by row). */
  allToAll?: boolean;
  /** Build the legs of each option (default true). */
  itineraries?: boolean;
  /** Add the options of every departure minute (default false). */
  byDeparture?: boolean;
  /** A fare structure (default null): needed for the `"FARE"` criterion, gives every option its fare. */
  fareStructure?: FareStructure | null;
  /** The highest fare of a trip (default Infinity). */
  maxFare?: number;
  /** Plain rows for the legs instead of GeoJSON features (default false). */
  dropGeometry?: boolean;
  /** Add r5r's `osm_link_ids` properties to each leg (default false), as in `detailedItineraries()`. */
  osmLinkIds?: boolean;
}

/**
 * One option of a pair: trips with the same street modes, the same routes
 * between the same stops and the same fare, transfers and minutes on foot,
 * not dominated at `n_departures` departure minutes. Times are those of its
 * best minute (the least total time); minutes have one decimal.
 */
export interface TripOptionRow {
  from_id: string | null;
  to_id: string | null;
  /** From 1, ordered by total time, then fare, transfers and minutes on foot. */
  option: number;
  total_time: number;
  /** With a fare structure only. */
  fare?: number;
  transfers: number;
  walk_time: number;
  departure_time: string;
  arrival_time: string;
  n_departures: number;
  first_departure: string;
  last_departure: string;
  /** The modes and route short names, as `"WALK > BUS 343 > WALK"`. */
  description: string;
}

/** One option on the frontier of one departure minute (`byDeparture`). */
export interface TripDepartureRow {
  from_id: string | null;
  to_id: string | null;
  option: number;
  departure_time: string;
  arrival_time: string;
  total_time: number;
}

/** One leg of an option, keyed by `from_id`, `to_id` and `option`, in the format of `detailedItineraries()`. */
export interface TripLegRow {
  from_id: string | null;
  to_id: string | null;
  option: number;
  segment: number;
  mode: string;
  /** With a fare structure only: the fare paid up to the end of the leg. */
  cumulative_fare?: number;
  segment_duration: number;
  wait: number;
  distance: number;
  route: string;
  osm_id_list?: string;
  edge_id_list?: string;
  board_stop_id?: string;
  alight_stop_id?: string;
}

/** A leg of an option with its line (longitude, latitude), as a GeoJSON feature. */
export interface TripLegFeature {
  type: "Feature";
  properties: TripLegRow;
  geometry: { type: "LineString"; coordinates: [number, number][] };
}

/** The result of `tripOptions()`. */
export interface TripOptions {
  options: TripOptionRow[];
  /** The legs (null with `itineraries: false`). */
  itineraries: { type: "FeatureCollection"; features: TripLegFeature[] } | TripLegRow[] | null;
  /** Every minute's options (null without `byDeparture`). */
  departures: TripDepartureRow[] | null;
}

/**
 * peabiru's own multi-criteria trip planner (r5r has no counterpart): for
 * each origin-destination pair, the trips that no other trip beats on all
 * the chosen criteria at once, at each departure minute of the window,
 * merged into options. Frequency-based routes are boarded at their expected
 * times (half a headway): no random draws.
 */
export declare function tripOptions(
  network: Network,
  origins: Points,
  destinations: Points,
  options?: TripOptionsOptions,
): Promise<TripOptions>;

/**
 * A fare structure: the global settings, three tables as objects of
 * columns, and the debug settings. Made by `setupFareStructure()` or read
 * by `readFareStructure()`, and changed by editing its columns.
 */
export interface FareStructure {
  /** Most transfers with a discount (default 1; null for none). */
  max_discounted_transfers: number | null;
  /** Minutes between the boardings of a discounted transfer (default 120). */
  transfer_time_allowance: number | null;
  /** Highest fare of a trip (Infinity: no cap). */
  fare_cap: number;
  /**
   * One row per fare type: its name, whether its transfers are unlimited,
   * whether a transfer to the same route is allowed, whether the route's
   * own fare is used, and its fare.
   */
  fares_per_type: {
    type: (string | null)[];
    unlimited_transfers: boolean[];
    allow_same_route_transfer: boolean[];
    use_route_fare: boolean[];
    fare: number[];
  };
  /** The fare of two rides together, for the pairs of fare types with a discount. */
  fares_per_transfer: { first_leg: (string | null)[]; second_leg: (string | null)[]; fare: number[] };
  /** The fare type of each GTFS route, matched by `agency_id` and `route_id`; a route of the network missing from it is an error. */
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
  /** Kept as given; no debug file is written. */
  debug_settings: { output_file: string; trip_info: string };
}

/** Options of `setupFareStructure()`. */
export interface SetupFareStructureOptions {
  /** One fare type per mode (`"MODE"`, the default), per agency (`"AGENCY_ID"`, `"AGENCY_NAME"`) or one for all routes (`"GENERIC"`). */
  by?: string;
  /** A `.csv` path, kept in `debug_settings` (default null). */
  debugPath?: string | null;
  /** `"MODE"`, `"ROUTE"` (the default with a `debugPath`) or `"MODE_ROUTE"`, kept in `debug_settings`; needs a `debugPath`. */
  debugInfo?: string | null;
}

/**
 * A fare structure for the routes of a network in which every route, fare
 * type and transfer between two types costs `baseFare` (at least 0, read
 * as a 32-bit float), with one fare type per mode, per agency or for all
 * routes (`by`). Edit its columns to set other fares.
 *
 * ```js
 * const fares = await peabiru.setupFareStructure(network, 4.4);
 * console.log(fares.fares_per_type.type, fares.fares_per_route.route_id.length, "routes");
 * ```
 *
 * Compatibility with r5r: `setup_fare_structure()`, the same structure
 * (with objects of columns for r5r's `data.table`s).
 */
export declare function setupFareStructure(
  network: Network,
  baseFare: number,
  options?: SetupFareStructureOptions,
): Promise<FareStructure>;

/**
 * A fare structure read from the bytes of a `.zip` file of fare structure
 * CSV files (written by `writeFareStructure()`, for example), whose text is in
 * `encoding`: `"UTF-8"` (default), `"Latin-1"` or `"unknown"`. Synchronous;
 * needs the module loaded (`init()` or `buildNetwork()` first).
 *
 * ```js
 * const bytes = peabiru.writeFareStructure(await peabiru.setupFareStructure(network, 4.4));
 * const fares = peabiru.readFareStructure(bytes);
 * console.log(fares.fare_cap, fares.fares_per_type.fare);
 * ```
 *
 * Compatibility with r5r: `read_fare_structure()`, from the file's bytes
 * rather than a path; it reads r5r's files.
 */
export declare function readFareStructure(
  bytes: Uint8Array | ArrayBuffer | ArrayBufferView,
  encoding?: "UTF-8" | "unknown" | "Latin-1",
): FareStructure;

/**
 * The bytes of a fare structure `.zip` file: one CSV file for the global
 * settings and one per table, stored uncompressed. Synchronous; needs the
 * module loaded. Throws when the structure is not one.
 *
 * ```js
 * const bytes = peabiru.writeFareStructure(await peabiru.setupFareStructure(network, 4.4));
 * console.log(bytes.length, "bytes"); // in Node: await writeFile("fares.zip", bytes)
 * ```
 *
 * Compatibility with r5r: `write_fare_structure()`, the same CSV files
 * (r5r reads them), returned as bytes rather than written to a path; the
 * archive's bytes differ, since r5r's also holds file dates.
 */
export declare function writeFareStructure(fareStructure: FareStructure): Uint8Array;

/** Options of `paretoFrontier()`. */
export interface ParetoFrontierOptions
  extends Omit<RoutingOptions, "drawsPerMinute" | "newCarspeeds" | "carspeedScale" | "newLts"> {
  /** As in the other functions, but `["WALK", "TRANSIT"]` by default. */
  mode?: string | string[];
  /** Required with public transport. */
  departureDatetime?: DateTime | null;
  /** Minutes of departure times, from `departureDatetime` on (default 10), sampled once a minute. */
  timeWindow?: number;
  /** 1 to 5 distinct whole numbers from 1 to 99 (default 50). */
  percentiles?: number | number[];
  /** Required: the fare structure. */
  fareStructure?: FareStructure;
  /** Required: the fare cutoffs, at least 0, distinct. */
  fareCutoffs?: number | number[];
}

/**
 * Columns of a Pareto frontier: one row per origin, destination,
 * percentile and fare cutoff at which the travel time changes.
 * `monetary_cost` is the cutoff as R5 reads it, a 32-bit float (4.4 is
 * 4.400000095367432).
 */
export interface ParetoFrontierTable {
  from_id: (string | null)[];
  to_id: (string | null)[];
  percentile: Int32Array;
  travel_time: Int32Array;
  monetary_cost: Float64Array;
}

/**
 * The trade-off between travel time and fare: for each origin, destination
 * and percentile, the travel time within each fare cutoff, one row per
 * cutoff at which it changes. Routed with R5's multi-criteria router
 * (McRAPTOR) over one departure time per minute of the window. Every
 * option is in `ParetoFrontierOptions` and `RoutingOptions`.
 *
 * ```js
 * const frontier = await peabiru.paretoFrontier(network, points, points, {
 *   departureDatetime: "2019-05-13 14:00:00",
 *   fareStructure: await peabiru.setupFareStructure(network, 4.4),
 *   fareCutoffs: [0, 4.4, 8.8],
 *   seed: 42,
 * });
 * console.log(frontier.from_id.length, "rows", frontier.monetary_cost[0]);
 * ```
 *
 * Compatibility with r5r: `pareto_frontier()`, the same options in
 * camelCase, defaults and result columns; `outputDir` is not supported.
 */
export declare function paretoFrontier(
  network: Network,
  origins: Points,
  destinations: Points,
  options?: ParetoFrontierOptions,
): Promise<ParetoFrontierTable>;

/**
 * Options of `isochrone()`: those of the routing functions (no fares,
 * scenarios or `outputDir`), plus its own.
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
   * The largest is the trip limit.
   */
  cutoffs?: number | number[];
  /** Web Mercator zoom level, 9 to 12, of the grid the polygons are made from (default 10). */
  zoom?: number;
  /** `true` (default) for polygons, `false` for the street segments reached. */
  polygonOutput?: boolean;
  /** Required with public transport; the current time otherwise (default null). */
  departureDatetime?: DateTime | null;
  /** Minutes of departure times, from `departureDatetime` on (default 10). */
  timeWindow?: number;
  /**
   * 1 to 5 distinct values from 1 to 99: one set of polygons per
   * percentile; null (default) means 50. Must be null for lines.
   */
  percentiles?: number | number[] | null;
  /** Accepted and not used: the trip limit comes from the cutoffs. */
  maxTripDuration?: number;
  /** Deprecated and not used: giving it only warns. */
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

/** Properties of a line isochrone: the street segment, with the properties of `StreetEdgeProperties`. */
export interface IsochroneLineProperties {
  id: string | null;
  /** The directed edge index, as a string. */
  edge_index: string;
  osm_id: number;
  /** The smallest cutoff at or above the travel time, minutes. */
  isochrone: number;
  /** The travel time to the segment, minutes. */
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
 * The areas, or the street segments, reached from each origin within each
 * cutoff. With `polygonOutput: true` (the default), one `MultiPolygon`
 * feature per origin, percentile (ascending) and cutoff (descending),
 * empty when nothing is reached, made by interpolating the travel times on
 * a Web Mercator grid (`zoom`) and tracing their contours with isoband's
 * algorithm; with `polygonOutput: false`, one `LineString` per street
 * segment reached, by origin, then band and travel time descending. Every
 * option is in `IsochroneOptions`.
 *
 * ```js
 * const iso = await peabiru.isochrone(network, points, {
 *   mode: "WALK",
 *   cutoffs: [5, 10],
 * });
 * console.log(iso.features.map((f) => `${f.properties.id} ${f.properties.isochrone} min`));
 * ```
 *
 * Compatibility with r5r: `isochrone()`, the same options in camelCase,
 * defaults and properties; the polygons are those r5r makes before
 * `sf::st_make_valid()`, which is not applied (the same areas, not snapped
 * to 1e-7 degrees, always MultiPolygons), with their rings in the order
 * isoband gives on macOS on every platform.
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

/** Options of `findSnap()`. */
export interface FindSnapOptions {
  /** The largest distance in metres within which to snap (default 1600). */
  radius?: number;
  /** The mode whose streets the points snap to (default `"WALK"`). */
  mode?: "WALK" | "BICYCLE" | "CAR";
  /** Receives the warnings about the points; by default `console.warn`. */
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
 * Where points snap to the street network: for each point, the nearest
 * point of a street the mode may use, first within 300 metres and then
 * within `radius`. A point that does not snap is not routed.
 *
 * ```js
 * const snap = await peabiru.findSnap(network, points, { mode: "CAR" });
 * console.log(snap.point_id, snap.distance, snap.found);
 * ```
 *
 * Compatibility with r5r: `find_snap()`, the same options, defaults and
 * columns.
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
 * The bounding box of the street network: R5's envelope of the street
 * layer, which covers every OpenStreetMap node of the ways streets were
 * made from. `output` is `"polygon"` (the default, a Polygon whose ring
 * starts at the south-west corner and goes counter-clockwise), `"bbox"`
 * (the GeoJSON bbox `[xmin, ymin, xmax, ymax]`) or `"vector"` (an object).
 *
 * ```js
 * console.log(await peabiru.streetNetworkBbox(network, "vector"));
 * ```
 *
 * Compatibility with r5r: `street_network_bbox()`, the same three outputs
 * (GeoJSON for r5r's `sfc` and `bbox`, an object for its named vector).
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

/** Properties of a directed street edge. */
export interface StreetEdgeProperties {
  /** The directed edge index in the network. */
  edge_index: number;
  /** The OpenStreetMap way the edge comes from. */
  osm_id: number;
  from_vertex: number;
  to_vertex: number;
  street_class: "MOTORWAY" | "PRIMARY" | "SECONDARY" | "TERTIARY" | "OTHER";
  /** Metres. */
  length: number;
  /** Whether walking may use the edge. */
  walk: boolean;
  /** Whether driving may use the edge. */
  car: boolean;
  /** km/h. */
  car_speed: number;
  /** Whether cycling may use the edge. */
  bicycle: boolean;
  /** The level of traffic stress for cyclists, 1 to 4. */
  bicycle_lts: number;
}

/** Options of the functions that only read the network. */
export interface InspectOptions {
  /** Aborts the call. */
  signal?: AbortSignal;
}

/**
 * The street network as GeoJSON: `vertices`, the Points at the ends of
 * street edges, and `edges`, every directed street edge as a LineString
 * (the connectors of transit stops and park-and-rides left out), in the
 * network's order.
 *
 * ```js
 * const { vertices, edges } = await peabiru.streetNetworkToSf(network);
 * console.log(vertices.features.length, edges.features.length, edges.features[0].properties);
 * ```
 *
 * Compatibility with r5r: `street_network_to_sf()`, the same features,
 * properties and order (GeoJSON for r5r's two `sf` tables).
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
  /** R5's mode of the GTFS `route_type`, such as `"BUS"`. */
  mode: string;
}

/**
 * The public transport network as GeoJSON: `stops`, one Point per stop (a
 * stop not linked to the streets has a null geometry), and `routes`, one
 * feature per trip pattern, whose line is the GTFS shape of the trip that
 * made the pattern or, without one, the positions of its linked stops.
 * Patterns without a line are left out, and a line whose points are all
 * the same is a Point.
 *
 * ```js
 * const { stops, routes } = await peabiru.transitNetworkToSf(network);
 * console.log(stops.features.length, "stops,", routes.features.length, "routes", routes.features[0].properties);
 * ```
 *
 * Compatibility with r5r: `transit_network_to_sf()`, the same features,
 * properties and order (a null geometry for r5r's `POINT EMPTY`).
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
 * The services active on dates: the given `dates`, every date from
 * `startDate` to `endDate`, or, with no date option, the dates on which
 * some trip runs, found from the network (ascending). `total_services`
 * counts every service, used by trips or not.
 *
 * ```js
 * const week = await peabiru.checkTransitAvailability(network, { startDate: "2019-05-12", endDate: "2019-05-18" });
 * console.log(week.date, week.active_services);
 * ```
 *
 * Compatibility with r5r: `check_transit_availability()`, the same
 * options and columns, with the dates as strings.
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
  /** `"HIGH"` (`buildNetwork()` refuses the feed), `"MEDIUM"`, `"LOW"` or `"UNKNOWN"`. */
  priority: string[];
}

/**
 * The errors gtfs-lib (R5's GTFS library) reports when it loads GTFS
 * feeds, one row per error, feed by feed in the order gtfs-lib finds them.
 * A Network does not keep its feeds: pass them as `buildNetwork()` takes
 * them (an array of `{name, data}`, or an object with a `gtfs` array).
 * Loads the module first if needed. Rejects with an Error naming the cause
 * for a feed on which gtfs-lib fails instead, and for a `locations.geojson`
 * (GTFS-Flex zones) that is not in UTF-8. GTFS-Flex zones and location
 * groups are read as gtfs-lib reads them, for its errors; on-demand trips
 * are not routed.
 *
 * ```js
 * const errors = await peabiru.getGtfsErrors(gtfs);
 * console.log(errors.type.length, "errors:", [...new Set(errors.type)]);
 * ```
 *
 * Compatibility with r5r: `get_gtfs_errors()`, the same rows and columns,
 * in a fixed order (r5r's changes from run to run), from the feeds' bytes
 * rather than the network.
 */
export declare function getGtfsErrors(feeds: GtfsFeed[] | { gtfs: GtfsFeed[] }): Promise<GtfsErrorsTable>;
