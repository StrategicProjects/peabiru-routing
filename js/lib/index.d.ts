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
 * module, as in a Worker that was sent them.
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
 * until `free()`. Built by `buildNetwork()`; it cannot be sent to another
 * Worker (build it there).
 */
export declare class Network {
  private constructor();
  /** Release the network's memory; the network cannot be used afterwards. */
  free(): void;
  info(): NetworkInfo;
  /**
   * Services in the network and services running on `date` (a Date, read
   * in local time, or a string `"YYYY-MM-DD"`).
   */
  transitAvailability(date: Date | string): { services: number; active: number };
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
   * `"WALK"` (default), `"BICYCLE"` or `"CAR"`, or `"TRANSIT"` (alone or
   * with `"WALK"`). Transit modes other than `"TRANSIT"`, and bicycle or
   * car access to transit, throw `UnsupportedError`.
   */
  mode?: string | string[];
  /** Only `"WALK"` (default) is supported with transit. */
  modeEgress?: string;
  /** Minutes; `Infinity` (default) means the trip limit. */
  maxWalkTime?: number;
  /** Minutes; `Infinity` (default) means the trip limit. */
  maxBikeTime?: number;
  /** Minutes; `Infinity` (default) means the trip limit. */
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
   * Checked as in r5r (null or Infinity = all cores, else a whole number
   * >= 1); this version always routes on one thread.
   */
  nThreads?: number | null;
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
  /** Not supported: must be null (default). */
  fareStructure?: null;
  /** Not supported: must be Infinity (default). */
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
): TravelTimeMatrix;

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
): AccessibilityTable;

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
): ExpandedMatrix;

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
): ExpandedMatrix;
