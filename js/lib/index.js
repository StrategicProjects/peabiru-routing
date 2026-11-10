// peabiru: multimodal routing on OpenStreetMap and GTFS networks, in
// WebAssembly.
//
// The functions mirror the Python and R packages of peabiru, which mirror
// r5r 2.5.0: the same arguments (in camelCase), the same defaults, the
// same checks and the same results. The argument checks, the trip limits
// and the assembly of the result columns live here; the routing runs in
// the Rust core, compiled to WebAssembly (pkg/peabiru_bg.wasm), on the
// calling thread.

import wasmInit, {
  initSync as wasmInitSync,
  Network as RawNetwork,
  accessibility as rawAccessibility,
  core_version as rawCoreVersion,
  expanded as rawExpanded,
  street_ttm as rawStreetTtm,
  transit_ttm as rawTransitTtm,
} from "../pkg/peabiru.js";

/** Version of this package. */
export const version = "0.0.1";

/** Missing value in the Int32Array columns of the results. */
export const NA_INT32 = -2147483648;

const INT_MAX = 2147483647;
// checkmate's default tolerance for "integerish"
const TOL = Math.sqrt(Number.EPSILON);

// ---------------------------------------------------------------------------
// Loading the WebAssembly module

let ready = false;
let loading = null;

/**
 * Load and compile the WebAssembly module. `buildNetwork()` calls it, so it
 * is only needed to load the module from somewhere else (a URL, a Response,
 * bytes or a WebAssembly.Module) or before `coreVersion()`.
 */
export function init(input) {
  if (ready) return Promise.resolve();
  if (!loading) {
    loading = load(input).then(
      () => {
        ready = true;
      },
      (e) => {
        loading = null;
        throw e;
      },
    );
  }
  return loading;
}

async function load(input) {
  if (input === undefined) {
    const url = new URL("../pkg/peabiru_bg.wasm", import.meta.url);
    if (url.protocol === "file:") {
      // Node (and Deno, Bun): fetch() does not read file: URLs
      const fs = await import(/* webpackIgnore: true */ /* @vite-ignore */ "node:fs/promises");
      input = await fs.readFile(url);
    } else {
      input = url;
    }
  }
  await wasmInit({ module_or_path: input });
}

/**
 * Load the WebAssembly module synchronously from its bytes or a compiled
 * WebAssembly.Module (in a Worker that received them, for instance).
 */
export function initSync(module) {
  if (ready) return;
  wasmInitSync({ module });
  ready = true;
}

function needInit() {
  if (!ready) {
    throw new Error("the WebAssembly module is not loaded: await init() or buildNetwork() first");
  }
}

/** Version of the Rust routing core (after `init()` or `buildNetwork()`). */
export function coreVersion() {
  needInit();
  return rawCoreVersion();
}

// ---------------------------------------------------------------------------
// Errors and checks (r5r uses checkmate; these are its few assertions)

/**
 * An option of r5r that peabiru does not support yet. peabiru never
 * ignores an argument it cannot honour: requesting it throws this error.
 */
export class UnsupportedError extends Error {
  constructor(message) {
    super(message);
    this.name = "UnsupportedError";
  }
}

function unsupported(what) {
  throw new UnsupportedError(`${what} is not supported by peabiru yet.`);
}

function fail(name, msg, Kind = RangeError) {
  throw new Kind(`Assertion on '${name}' failed: ${msg}.`);
}

function fmt(x) {
  if (x === Infinity) return "Inf";
  if (x === -Infinity) return "-Inf";
  return String(x);
}

const isNumber = (x) => typeof x === "number";
const isIntegerish = (x) => Number.isFinite(x) && Math.abs(x - Math.round(x)) < TOL;

function assertNumber(x, name, { lower = -Infinity, upper = Infinity, finite = false, nullOk = false } = {}) {
  if (x === null || x === undefined) {
    if (nullOk) return;
    fail(name, "Must be of type 'number', not 'null'", TypeError);
  }
  if (!isNumber(x)) fail(name, "Must be of type 'number'", TypeError);
  if (Number.isNaN(x)) fail(name, "May not be NA");
  if (finite && !Number.isFinite(x)) fail(name, "Must be finite");
  if (x < lower) fail(name, `Element 1 is not >= ${fmt(lower)}`);
  if (x > upper) fail(name, `Element 1 is not <= ${fmt(upper)}`);
}

function assertCount(x, name, positive = false) {
  if (!isNumber(x)) fail(name, "Must be of type 'count'", TypeError);
  if (Number.isNaN(x)) fail(name, "May not be NA");
  if (!isIntegerish(x) || Math.abs(x) > INT_MAX) fail(name, "Must be of type 'count'", TypeError);
  if (x < (positive ? 1 : 0)) fail(name, `Must be >= ${positive ? 1 : 0}`);
}

function assertInt(x, name, nullOk = false) {
  if ((x === null || x === undefined) && nullOk) return;
  if (!isNumber(x) || !isIntegerish(x) || Math.abs(x) > INT_MAX) {
    fail(name, "Must be a single integerish value", TypeError);
  }
}

function numberList(x, name, kind) {
  if (isNumber(x)) return [x];
  if (x === null || typeof x !== "object" || typeof x[Symbol.iterator] !== "function") {
    fail(name, `Must be of type '${kind}'`, TypeError);
  }
  const values = Array.from(x);
  if (!values.every(isNumber)) fail(name, `Must be of type '${kind}'`, TypeError);
  return values;
}

function assertIntegerish(x, name, { lower = -Infinity, upper = Infinity, minLen = 0, maxLen = Infinity, unique = false, nullOk = false } = {}) {
  if (x === null || x === undefined) {
    if (nullOk) return null;
    fail(name, "Must be of type 'integerish', not 'null'", TypeError);
  }
  const values = numberList(x, name, "integerish");
  if (!values.every((v) => Number.isNaN(v) || isIntegerish(v))) fail(name, "Must be of type 'integerish'", TypeError);
  if (values.some(Number.isNaN)) fail(name, "Contains missing values");
  if (values.length < minLen) fail(name, `Must have length >= ${minLen}`);
  if (values.length > maxLen) fail(name, `Must have length <= ${fmt(maxLen)}`);
  if (values.some((v) => v < lower)) fail(name, `All elements must be >= ${fmt(lower)}`);
  if (values.some((v) => v > upper)) fail(name, `All elements must be <= ${fmt(upper)}`);
  if (unique && new Set(values).size < values.length) fail(name, "Contains duplicated values");
  return values.map((v) => Math.round(v));
}

function assertFlag(x, name) {
  if (typeof x !== "boolean") fail(name, "Must be a single true or false", TypeError);
}

function assertString(x, name) {
  if (typeof x !== "string") fail(name, "Must be a single string", TypeError);
}

function asStrings(x, name) {
  if (typeof x === "string") return [x];
  const msg = "Must be of type 'character' with at least 1 element and no missing values";
  if (x === null || typeof x !== "object" || typeof x[Symbol.iterator] !== "function") fail(name, msg, TypeError);
  const values = Array.from(x);
  if (values.length < 1 || !values.every((v) => typeof v === "string")) fail(name, msg, TypeError);
  return values;
}

const uniq = (values) => [...new Set(values)];

function warner(options) {
  const f = options && options.onWarning;
  if (f === undefined || f === null) return (m) => console.warn(m);
  if (typeof f !== "function") fail("onWarning", "Must be a function", TypeError);
  return f;
}

function optionsObject(options, name = "options") {
  if (options === undefined || options === null) return {};
  if (typeof options !== "object" || Array.isArray(options)) fail(name, "Must be an object", TypeError);
  return options;
}

// ---------------------------------------------------------------------------
// Inputs

function bytesOf(x, name) {
  if (x instanceof Uint8Array) return x;
  if (x instanceof ArrayBuffer) return new Uint8Array(x);
  if (ArrayBuffer.isView(x)) return new Uint8Array(x.buffer, x.byteOffset, x.byteLength);
  fail(name, "Must be a Uint8Array, an ArrayBuffer or another ArrayBuffer view", TypeError);
}

function column(points, key) {
  return Array.isArray(points) ? points.map((r) => (r === null || r === undefined ? undefined : r[key])) : points[key];
}

/**
 * r5r assign_points_input(): columns `id`, `lon`, `lat` (an object of
 * arrays), or an array of `{id, lon, lat}` objects.
 */
function pointsInput(points, name, warn) {
  if (points === null || typeof points !== "object") {
    fail(name, "Must be an object of columns {id, lon, lat} or an array of {id, lon, lat} objects", TypeError);
  }
  const missing = Array.isArray(points) ? [] : ["id", "lat", "lon"].filter((c) => points[c] === undefined);
  if (missing.length) {
    fail(name, `Names must include the elements {'id','lat','lon'}, but is missing elements {${missing.map((c) => `'${c}'`).join(",")}}`);
  }
  const idCol = column(points, "id");
  const n = idCol.length;
  const coords = {};
  for (const c of ["lon", "lat"]) {
    const col = column(points, c);
    if (col === null || typeof col !== "object" || typeof col.length !== "number") fail(`${name}.${c}`, "Must be of type 'numeric'", TypeError);
    if (col.length !== n) fail(name, `Columns 'id' and '${c}' must have the same length`);
    const out = new Float64Array(n);
    for (let i = 0; i < n; i++) {
      const v = col[i];
      if (v === null || v === undefined) out[i] = NaN;
      else if (typeof v === "number") out[i] = v;
      else fail(`${name}.${c}`, "Must be of type 'numeric'", TypeError);
    }
    coords[c] = out;
  }
  const ids = new Array(n);
  let cast = false;
  for (let i = 0; i < n; i++) {
    const v = idCol[i];
    if (v === null || v === undefined) ids[i] = null;
    else if (typeof v === "string") ids[i] = v;
    else {
      ids[i] = String(v);
      cast = true;
    }
  }
  if (cast) warn(`'${name}.id' forcefully cast to character.`);
  const na = [];
  for (let i = 0; i < n; i++) if (Number.isNaN(coords.lon[i]) || Number.isNaN(coords.lat[i])) na.push(ids[i]);
  if (na.length) {
    warn(`${na.length} point(s) in '${name}' have missing lon/lat coordinates and will not be routed. Affected id(s): ${na.map((i) => `"${i}"`).join(", ")}.`);
  }
  const seen = new Set();
  const dups = [];
  for (const id of ids) {
    if (seen.has(id)) dups.push(id);
    else seen.add(id);
  }
  if (dups.length) {
    throw new RangeError(`'${name}' must not contain duplicated ids. Duplicated id(s): ${uniq(dups).map((i) => `"${i}"`).join(", ")}.`);
  }
  return { ids, lat: coords.lat, lon: coords.lon, points, length: n };
}

const TRANSIT_MODES = ["TRANSIT", "TRAM", "SUBWAY", "RAIL", "BUS", "FERRY", "CABLE_CAR", "GONDOLA", "FUNICULAR"];
const DIRECT_MODES = ["WALK", "BICYCLE", "CAR", "BICYCLE_RENT", "CAR_PARK"];

/** r5r assign_mode(), plus the restriction to what the core supports. */
function assignMode(mode, modeEgress) {
  const all = [...TRANSIT_MODES, ...DIRECT_MODES];
  const modes = uniq(asStrings(mode, "mode").map((m) => m.toUpperCase()));
  if (!modes.every((m) => all.includes(m))) fail("mode", `Must be a subset of {${all.map((m) => `'${m}'`).join(",")}}`);
  if (Array.isArray(modeEgress) && modeEgress.every((m) => typeof m === "string")) {
    const repeated = uniq(modeEgress.map((m) => m.toUpperCase()));
    if (repeated.length === 1) modeEgress = repeated[0];
  }
  assertString(modeEgress, "modeEgress");
  const egress = modeEgress.toUpperCase();
  if (!["WALK", "BICYCLE", "CAR"].includes(egress)) fail("modeEgress", "Must be a subset of {'WALK','BICYCLE','CAR'}");

  let direct = modes.filter((m) => DIRECT_MODES.includes(m));
  const transit = modes.filter((m) => TRANSIT_MODES.includes(m));
  if (direct.length > 1) throw new RangeError(`Please use only 1 of {${direct.map((m) => `'${m}'`).join(",")}} when routing.`);
  if (direct.includes("CAR_PARK") || direct.includes("BICYCLE_RENT")) {
    throw new RangeError("CAR_PARK and BICYCLE_RENT are currently unsupported by r5r.");
  }
  let access = [...direct];
  let transitModes = [""];
  let egressMode = "";
  if (transit.length) {
    transitModes = transit.includes("TRANSIT") ? [...TRANSIT_MODES] : transit;
    if (!direct.length) access = direct = ["WALK"];
    egressMode = egress;
  }
  const ml = {
    directModes: direct.join(";"),
    accessMode: access.join(";"),
    egressMode,
    transit: !(transitModes.length === 1 && transitModes[0] === ""),
  };
  if (ml.transit) {
    if (transitModes.length !== TRANSIT_MODES.length) {
      unsupported(`Routing on specific transit modes (${modes.filter((m) => !DIRECT_MODES.includes(m)).join(", ")}); use 'TRANSIT'`);
    }
    if (ml.accessMode !== "WALK") unsupported(`Transit with ${ml.accessMode} access`);
    if (ml.egressMode !== "WALK") unsupported(`Transit with ${ml.egressMode} egress`);
  }
  return ml;
}

const pad = (n, w = 2) => String(n).padStart(w, "0");

/**
 * A date and clock time: a Date (its local date and clock time, in the
 * time zone of the JavaScript runtime) or a string "YYYY-MM-DD HH:MM:SS"
 * (or with "T", or without seconds), taken as it reads.
 */
function clockParts(x, name) {
  if (x instanceof Date) {
    if (Number.isNaN(x.getTime())) fail(name, "Must be a valid Date or a string 'YYYY-MM-DD HH:MM:SS'", TypeError);
    return { y: x.getFullYear(), mo: x.getMonth() + 1, d: x.getDate(), h: x.getHours(), mi: x.getMinutes(), s: x.getSeconds() };
  }
  if (typeof x === "string") {
    const m = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2})(?::(\d{2}))?$/.exec(x.trim());
    if (m) {
      const [y, mo, d, h, mi, s] = m.slice(1).map((v) => (v === undefined ? 0 : Number(v)));
      const t = new Date(Date.UTC(y, mo - 1, d, h, mi, s));
      if (h < 24 && mi < 60 && s < 60 && t.getUTCFullYear() === y && t.getUTCMonth() === mo - 1 && t.getUTCDate() === d) {
        return { y, mo, d, h, mi, s };
      }
    }
  }
  fail(name, "Must be a valid Date or a string 'YYYY-MM-DD HH:MM:SS'", TypeError);
}

function departureOf(p) {
  return {
    date: `${pad(p.y, 4)}-${pad(p.mo)}-${pad(p.d)}`,
    yyyymmdd: p.y * 10000 + p.mo * 100 + p.d,
    seconds: p.h * 3600 + p.mi * 60 + p.s,
  };
}

/** r5r assign_departure(). */
function assignDeparture(datetime, modeList, name = "departureDatetime") {
  if (datetime === null || datetime === undefined) {
    if (!modeList.transit) datetime = new Date();
    else {
      throw new RangeError(
        `You must pass a '${name}' for public transport routing. Results depend on the departure time. Supply an explicit datetime value to make results reproducible.`,
      );
    }
  }
  return departureOf(clockParts(datetime, name));
}

/**
 * `datetime - minutes`: on the absolute time for a Date (as R does with
 * POSIXct), on the clock time for a string.
 */
function minusMinutes(datetime, minutes, name) {
  if (datetime instanceof Date) {
    clockParts(datetime, name);
    return new Date(datetime.getTime() - minutes * 60000);
  }
  const p = clockParts(datetime, name);
  const t = new Date(Date.UTC(p.y, p.mo - 1, p.d, p.h, p.mi, p.s) - minutes * 60000);
  return `${pad(t.getUTCFullYear(), 4)}-${pad(t.getUTCMonth() + 1)}-${pad(t.getUTCDate())} ${pad(t.getUTCHours())}:${pad(t.getUTCMinutes())}:${pad(t.getUTCSeconds())}`;
}

/** r5r assign_max_street_time(). */
function maxStreetTime(maxTime, speed, maxTripDuration, mode) {
  const v = `max${mode[0].toUpperCase()}${mode.slice(1)}Time`;
  assertNumber(maxTime, v, { lower: 1 });
  if (Number.isFinite(maxTime)) assertCount(maxTime, v, true);
  assertNumber(speed, `${mode}Speed`, { finite: true, nullOk: true });
  assertCount(maxTripDuration, "maxTripDuration", true);
  if (speed !== null && speed !== undefined && speed <= 0) {
    throw new RangeError(`Assertion on '${mode}Speed' failed: Must have value greater than 0.`);
  }
  return Math.round(Number.isFinite(maxTime) ? maxTime : maxTripDuration);
}

/** r5r assign_max_trip_duration(): a street mode alone is capped by its own limit. */
function streetLimits(o, modeList) {
  const walk = maxStreetTime(o.maxWalkTime, o.walkSpeed, o.maxTripDuration, "walk");
  const bike = maxStreetTime(o.maxBikeTime, o.bikeSpeed, o.maxTripDuration, "bike");
  const car = maxStreetTime(o.maxCarTime, null, o.maxTripDuration, "car");
  assertCount(o.maxTripDuration, "maxTripDuration", true);
  let trip = Math.round(o.maxTripDuration);
  if (!modeList.transit) {
    if (modeList.directModes === "WALK" && walk < trip) trip = walk;
    if (modeList.directModes === "BICYCLE" && bike < trip) trip = bike;
    if (modeList.directModes === "CAR" && car < trip) trip = car;
  }
  return { walk, bike, car, trip };
}

function assignPercentiles(percentiles) {
  const v = assertIntegerish(percentiles, "percentiles", { lower: 1, upper: 99, minLen: 1, maxLen: 5, unique: true });
  return v.sort((a, b) => a - b);
}

/** r5r set_n_threads(): null or Infinity mean all cores. */
function checkThreads(nThreads) {
  if (nThreads === null || nThreads === undefined) return;
  assertNumber(nThreads, "nThreads", { lower: 1 });
  if (Number.isFinite(nThreads)) assertCount(nThreads, "nThreads", true);
}

function checkSpeed(speed, mode) {
  const name = `${mode}Speed`;
  assertNumber(speed, name, { finite: true });
  if (speed <= 0) throw new RangeError(`Assertion on '${name}' failed: Must have value greater than 0.`);
}

function checkMaxLts(maxLts) {
  assertCount(maxLts, "maxLts", true);
  if (maxLts < 1 || maxLts > 4) {
    throw new RangeError(`${maxLts} is not a valid value for the maximum Level of Transit Stress (LTS).\nPlease enter a value between 1 and 4.`);
  }
}

function checkFares(fareStructure, maxFare) {
  if (fareStructure !== null && fareStructure !== undefined) unsupported("'fareStructure' (fare calculation)");
  assertNumber(maxFare, "maxFare", { lower: 0 });
  if (Number.isFinite(maxFare)) {
    throw new RangeError(
      `'maxFare' requires a 'fareStructure': 'maxFare' is ${maxFare} but 'fareStructure' is null, so no fares are calculated and the limit would be ignored. Leave 'maxFare' as Infinity.`,
    );
  }
}

function checkScenarios(o) {
  assertNumber(o.carspeedScale, "carspeedScale", { lower: 0, finite: true });
  if ((o.newCarspeeds !== null && o.newCarspeeds !== undefined) || o.carspeedScale !== 1) {
    unsupported("Changing car speeds ('newCarspeeds', 'carspeedScale')");
  }
  if (o.newLts !== null && o.newLts !== undefined) unsupported("Changing LTS levels ('newLts')");
}

function checkOutputDir(outputDir) {
  if (outputDir !== null && outputDir !== undefined) {
    assertString(outputDir, "outputDir");
    unsupported("Writing results to 'outputDir'");
  }
}

/** The checks every routing function makes; returns the seed for the core. */
function commonChecks(o) {
  assertCount(o.drawsPerMinute, "drawsPerMinute", true);
  assertInt(o.seed, "seed", true);
  checkSpeed(o.walkSpeed, "walk");
  checkSpeed(o.bikeSpeed, "bike");
  assertCount(o.maxRides, "maxRides", true);
  checkMaxLts(o.maxLts);
  checkThreads(o.nThreads);
  assertFlag(o.verbose, "verbose");
  assertFlag(o.progress, "progress");
  // Without a seed the draws differ from call to call, as in r5r (which
  // seeds them from the clock): a seed drawn at random here.
  if (o.seed === null || o.seed === undefined) return (Math.random() * 4294967296) | 0;
  return Math.round(o.seed);
}

/** r5r check_transit_availability_on_date(). */
function checkTransitAvailability(network, departure, warn) {
  const [total, active] = network._raw().transit_availability(departure.yyyymmdd);
  const pct = total === 0 ? 0 : active / total;
  if (pct === 0) {
    throw new RangeError(
      `There are no transit services available on the selected departure date: "${departure.date}". Please ensure there is a GTFS in your data & that the departure date falls within the GTFS calendar.`,
    );
  }
  if (pct < 0.2) warn("Less than 20% of the transit services in the GTFS are running on the selected departure date.");
}

function assertNetwork(x) {
  if (!(x instanceof Network)) {
    throw new TypeError("Assertion on 'network' failed: Must be a peabiru Network (see buildNetwork()).");
  }
}

const DEFAULTS = {
  mode: "WALK",
  modeEgress: "WALK",
  departureDatetime: null,
  timeWindow: 10,
  percentiles: 50,
  maxWalkTime: Infinity,
  maxBikeTime: Infinity,
  maxCarTime: Infinity,
  maxTripDuration: 120,
  walkSpeed: 3.6,
  bikeSpeed: 12,
  maxRides: 3,
  maxLts: 2,
  fareStructure: null,
  maxFare: Infinity,
  newCarspeeds: null,
  carspeedScale: 1,
  newLts: null,
  drawsPerMinute: 5,
  seed: null,
  nThreads: null,
  verbose: false,
  progress: true,
  outputDir: null,
};

/** Options with the defaults; unknown names are an error (never ignored). */
function withDefaults(options, extra, known) {
  const o = optionsObject(options);
  const names = new Set([...known, ...Object.keys(extra), "onWarning"]);
  for (const k of Object.keys(o)) {
    if (!names.has(k)) throw new TypeError(`unknown option '${k}'`);
  }
  const out = { ...extra };
  for (const k of known) out[k] = DEFAULTS[k];
  for (const [k, v] of Object.entries(o)) if (v !== undefined) out[k] = v;
  return out;
}

const COMMON = [
  "mode", "modeEgress", "maxWalkTime", "maxBikeTime", "maxCarTime", "maxTripDuration", "walkSpeed",
  "bikeSpeed", "maxRides", "maxLts", "newCarspeeds", "carspeedScale", "newLts", "drawsPerMinute", "seed",
  "nThreads", "verbose", "progress", "outputDir",
];

// ---------------------------------------------------------------------------
// The network

/**
 * A transport network built by `buildNetwork()`: the street network and,
 * when GTFS feeds were given, the public transport layer, in the memory of
 * the WebAssembly module. Call `free()` when done with it.
 */
export class Network {
  #raw;

  /** @internal use buildNetwork() */
  constructor(raw) {
    if (!(raw instanceof RawNetwork)) throw new TypeError("use buildNetwork() to build a Network");
    this.#raw = raw;
  }

  /** @internal */
  _raw() {
    if (!this.#raw) throw new Error("this Network was freed");
    return this.#raw;
  }

  /** Release the network's memory; the network cannot be used afterwards. */
  free() {
    if (this.#raw) {
      this.#raw.free();
      this.#raw = null;
    }
  }

  /** Sizes of the network. */
  info() {
    const raw = this._raw();
    const [stops, patterns, services, frequencies] = raw.transit_sizes();
    return {
      vertices: raw.n_vertices(),
      edges: raw.n_edges(),
      feeds: Array.from(raw.feed_ids()),
      stops,
      patterns,
      services,
      frequencies: frequencies === 1,
    };
  }

  /**
   * Services in the network and services running on a date (a Date, or a
   * string "YYYY-MM-DD"), as r5r counts them before routing.
   */
  transitAvailability(date) {
    let p;
    if (typeof date === "string" && /^\d{4}-\d{2}-\d{2}$/.test(date.trim())) p = clockParts(`${date.trim()} 00:00:00`, "date");
    else p = clockParts(date, "date");
    const [services, active] = this._raw().transit_availability(departureOf(p).yyyymmdd);
    return { services, active };
  }

  /** Warnings for the GTFS feeds that r5r refuses to build a network from. */
  r5rWarnings() {
    return Array.from(this._raw().r5r_warnings());
  }
}

/**
 * Build a transport network from an OpenStreetMap extract (`pbf`) and zero
 * or more GTFS feeds, as r5r's build_network() does from a directory.
 */
export async function buildNetwork(input) {
  if (input === null || typeof input !== "object") fail("input", "Must be an object {pbf, gtfs}", TypeError);
  const o = optionsObject(input, "input");
  for (const k of Object.keys(o)) {
    if (!["pbf", "gtfs", "onWarning"].includes(k)) throw new TypeError(`unknown option '${k}'`);
  }
  const warn = warner(o);
  const pbf = bytesOf(o.pbf, "pbf");
  const gtfs = o.gtfs === undefined || o.gtfs === null ? [] : o.gtfs;
  if (!Array.isArray(gtfs)) fail("gtfs", "Must be an array of {name, data}", TypeError);
  const feeds = gtfs.map((f, i) => {
    if (f === null || typeof f !== "object") fail(`gtfs[${i}]`, "Must be an object {name, data}", TypeError);
    assertString(f.name, `gtfs[${i}].name`);
    return [f.name, bytesOf(f.data, `gtfs[${i}].data`)];
  });
  const names = feeds.map((f) => f[0]);
  if (new Set(names).size < names.length) fail("gtfs", "Feed names must be distinct");
  await init();
  const network = new Network(RawNetwork.build(pbf, feeds));
  for (const w of network.r5rWarnings()) warn(w);
  return network;
}

// ---------------------------------------------------------------------------
// Routing

function ids(idx, idList) {
  const out = new Array(idx.length);
  for (let k = 0; k < idx.length; k++) out[k] = idList[idx[k]];
  return out;
}

/**
 * Travel-time matrix between origins and destinations, as r5r's
 * travel_time_matrix().
 */
export function travelTimeMatrix(network, origins, destinations, options) {
  const opt = withDefaults(options, {}, [...COMMON, "departureDatetime", "timeWindow", "percentiles", "fareStructure", "maxFare"]);
  const warn = warner(opt);
  let o = pointsInput(origins, "origins", warn);
  let d = pointsInput(destinations, "destinations", warn);
  const modeList = assignMode(opt.mode, opt.modeEgress);
  const departure = assignDeparture(opt.departureDatetime, modeList);
  assertNetwork(network);
  if (modeList.transit) checkTransitAvailability(network, departure, warn);
  // r5r reverse_if_direct_mode(): walking alone with more origins than
  // destinations routes from the destinations (there is no elevation
  // raster here, which would turn it off)
  const swapped = !modeList.transit && modeList.directModes === "WALK" && o.length > d.length;
  if (swapped) [o, d] = [d, o];
  const lim = streetLimits(opt, modeList);
  assertCount(opt.timeWindow, "timeWindow", true);
  const pcts = assignPercentiles(opt.percentiles);
  const seed = commonChecks(opt);
  checkFares(opt.fareStructure, opt.maxFare);
  checkOutputDir(opt.outputDir);
  checkScenarios(opt);

  const raw = network._raw();
  let cols;
  if (modeList.transit) {
    cols = rawTransitTtm(
      raw, o.lat, o.lon, d.lat, d.lon, departure.yyyymmdd, departure.seconds, Math.round(opt.timeWindow),
      Int32Array.from(pcts), lim.walk, lim.trip, opt.walkSpeed, Math.round(opt.maxRides),
      Math.round(opt.drawsPerMinute), seed,
    );
  } else {
    const [from, to, tt] = rawStreetTtm(
      raw, o.lat, o.lon, d.lat, d.lon, modeList.directModes, opt.walkSpeed, opt.bikeSpeed,
      Math.round(opt.maxLts), lim.trip,
    );
    // a street mode has one travel time, reported for every percentile
    cols = [from, to, ...pcts.map((_, k) => (k === 0 ? tt : tt.slice()))];
  }
  const [from, to] = cols;
  const out = swapped
    ? { from_id: ids(to, d.ids), to_id: ids(from, o.ids) }
    : { from_id: ids(from, o.ids), to_id: ids(to, d.ids) };
  pcts.forEach((p, k) => {
    const t = cols[2 + k];
    // r5r: travel times of non-viable trips become NA
    for (let i = 0; i < t.length; i++) if (t[i] > lim.trip) t[i] = NA_INT32;
    out[`travel_time_p${pad(p)}`] = t;
  });
  return out;
}

const DECAY_FUNCTIONS = ["step", "exponential", "fixed_exponential", "linear", "logistic"];

/** r5r assign_decay_function(). */
function assignDecay(fun, value) {
  assertString(fun, "decayFunction");
  if (!DECAY_FUNCTIONS.includes(fun)) fail("decayFunction", `Must be a subset of {${DECAY_FUNCTIONS.map((f) => `'${f}'`).join(",")}}`);
  assertNumber(value, "decayValue", { finite: true, nullOk: true });
  const none = value === null || value === undefined;
  if (!none && (fun === "step" || fun === "exponential")) {
    throw new RangeError(`Assertion on decayValue failed: must be null when decayFunction is ${fun}.`);
  }
  if (none && ["fixed_exponential", "linear", "logistic"].includes(fun)) {
    throw new RangeError(`Assertion on decayValue failed: must not be null when decayFunction is ${fun}.`);
  }
  if (fun === "fixed_exponential" && (value <= 0 || value >= 1)) {
    throw new RangeError("Assertion on decayValue failed: must be a number between 0 and 1 (exclusive) when decayFunction is fixed_exponential.");
  }
  if ((fun === "logistic" || fun === "linear") && value < 1) {
    throw new RangeError(`Assertion on decayValue failed: must be a number greater than or equal to 1 when decayFunction is ${fun}.`);
  }
  // R5 takes the linear width as whole minutes, under one hour
  if (fun === "linear") assertIntegerish(value, "decayValue", { lower: 1, upper: 59 });
  // r5r passes the placeholder 0 when decay_value is NULL (R5 ignores it)
  return none ? 0 : value;
}

/** r5r set_cutoffs(): ascending, or the placeholder 0 for fixed_exponential. */
function assignCutoffs(cutoffs, fun) {
  const v = assertIntegerish(cutoffs, "cutoffs", { lower: 1, upper: 120, minLen: 1, maxLen: 12, unique: true, nullOk: true });
  if (v !== null && fun === "fixed_exponential") {
    throw new RangeError(`Assertion on cutoffs failed: must be null when decayFunction is ${fun}.`);
  }
  if (v === null && fun !== "fixed_exponential") {
    throw new RangeError(`Assertion on cutoffs failed: must not be null when decayFunction is ${fun}.`);
  }
  return v === null ? [0] : v.sort((a, b) => a - b);
}

/** r5r assign_opportunities(): numeric columns, NA counted as 0. */
function opportunityColumns(d, colnames, warn) {
  const msg = "Must be a character vector of unique names without missing values";
  const names = colnames === null || colnames === undefined ? [] : asStrings(colnames, "opportunitiesColnames");
  if (!names.length || new Set(names).size < names.length) fail("opportunitiesColnames", msg);
  const flat = new Float64Array(names.length * d.length);
  const missing = names.filter((c) => (Array.isArray(d.points) ? d.length > 0 && !(c in (d.points[0] || {})) : d.points[c] === undefined));
  if (missing.length) {
    fail("destinations", `Names must include the elements {${names.map((c) => `'${c}'`).join(",")}}, but is missing elements {${missing.map((c) => `'${c}'`).join(",")}}`);
  }
  names.forEach((c, k) => {
    const col = column(d.points, c);
    if (col === null || typeof col !== "object" || col.length !== d.length) fail(c, "Must be of type 'numeric'", TypeError);
    let na = 0;
    for (let i = 0; i < d.length; i++) {
      let v = col[i];
      if (v === null || v === undefined) v = NaN;
      else if (typeof v !== "number") fail(c, "Must be of type 'numeric'", TypeError);
      if (v === Infinity || v === -Infinity) fail(c, "Must be finite");
      if (Number.isNaN(v)) {
        na++;
        v = 0;
      }
      flat[k * d.length + i] = v;
    }
    if (na) warn(`'${c}' has ${na} missing value(s), treated as 0.`);
  });
  return { names, flat };
}

/** Access to opportunities, as r5r's accessibility(). */
export function accessibility(network, origins, destinations, options) {
  const opt = withDefaults(
    options,
    { opportunitiesColnames: "opportunities", decayFunction: "step", cutoffs: null, decayValue: null },
    [...COMMON, "departureDatetime", "timeWindow", "percentiles", "fareStructure", "maxFare"],
  );
  const warn = warner(opt);
  assertNetwork(network);
  const o = pointsInput(origins, "origins", warn);
  const d = pointsInput(destinations, "destinations", warn);
  const opp = opportunityColumns(d, opt.opportunitiesColnames, warn);
  const modeList = assignMode(opt.mode, opt.modeEgress);
  const departure = assignDeparture(opt.departureDatetime, modeList);
  if (modeList.transit) checkTransitAvailability(network, departure, warn);
  const fun = opt.decayFunction;
  const value = assignDecay(fun, opt.decayValue);
  const cutoffs = assignCutoffs(opt.cutoffs, fun);
  assertNumber(opt.maxTripDuration, "maxTripDuration", { lower: 1, finite: true });
  if (opt.cutoffs !== null && opt.cutoffs !== undefined) {
    const top = Math.max(...cutoffs);
    if (opt.maxTripDuration < top) throw new RangeError("'maxTripDuration' cannot be shorter than 'max(cutoffs)'");
    // only the step function ignores trips longer than the cutoffs
    if (fun === "step") opt.maxTripDuration = top;
  }
  const lim = streetLimits(opt, modeList);
  assertCount(opt.timeWindow, "timeWindow", true);
  const pcts = assignPercentiles(opt.percentiles);
  const seed = commonChecks(opt);
  checkFares(opt.fareStructure, opt.maxFare);
  checkOutputDir(opt.outputDir);
  checkScenarios(opt);

  const values = rawAccessibility(
    network._raw(), o.lat, o.lon, d.lat, d.lon, opp.flat, opp.names.length,
    modeList.transit ? "TRANSIT" : modeList.directModes, fun, value, Int32Array.from(cutoffs),
    Int32Array.from(pcts), departure.yyyymmdd, departure.seconds, Math.round(opt.timeWindow), lim.walk,
    lim.trip, opt.walkSpeed, opt.bikeSpeed, Math.round(opt.maxLts), Math.round(opt.maxRides),
    Math.round(opt.drawsPerMinute), seed,
  );
  // one row per origin, opportunity, percentile and cutoff, in that order
  const [nk, np, nc] = [opp.names.length, pcts.length, cutoffs.length];
  const n = values.length;
  const out = {
    id: new Array(n),
    opportunity: new Array(n),
    percentile: new Int32Array(n),
    cutoff: new Int32Array(n),
    accessibility: values,
  };
  let r = 0;
  for (let i = 0; i < o.length; i++) {
    for (let k = 0; k < nk; k++) {
      for (let p = 0; p < np; p++) {
        for (let c = 0; c < nc; c++, r++) {
          out.id[r] = o.ids[i];
          out.opportunity[r] = opp.names[k];
          out.percentile[r] = pcts[p];
          out.cutoff[r] = cutoffs[c];
        }
      }
    }
  }
  if (fun === "fixed_exponential") delete out.cutoff;
  return out;
}

function checkExpandedMode(modeList) {
  if (!modeList.transit) {
    unsupported(`An expanded or arrival travel time matrix by ${modeList.directModes} alone (use travelTimeMatrix() for street modes)`);
  }
}

const BREAKDOWN = ["access_time", "wait_time", "ride_time", "transfer_time", "egress_time"];

/** Column assembly of r5r's expanded and arrival matrices. */
function expandedResult(res, o, d, breakdown, maxTripDuration) {
  const [from, to, departure, draw, access, wait, ride, transfer, egress, total, nRides, codes, levels] = res;
  const times = { access_time: access, wait_time: wait, ride_time: ride, transfer_time: transfer, egress_time: egress, total_time: total };
  const n = from.length;
  const routes = new Array(n);
  const hhmmss = new Map();
  const departureTime = new Array(n);
  for (let i = 0; i < n; i++) {
    // r5r: replace travel times of non-viable trips with NAs
    const over = total[i] > maxTripDuration;
    if (over) {
      for (const k in times) times[k][i] = NaN;
      nRides[i] = NA_INT32;
    }
    routes[i] = over || codes[i] < 0 ? null : levels[codes[i]];
    const s = departure[i];
    let t = hhmmss.get(s);
    if (t === undefined) {
      t = `${pad(Math.floor(s / 3600))}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`;
      hhmmss.set(s, t);
    }
    departureTime[i] = t;
  }
  const out = { from_id: ids(from, o.ids), to_id: ids(to, d.ids), departure_time: departureTime, draw_number: draw };
  if (breakdown) for (const k of BREAKDOWN) out[k] = times[k];
  out.routes = routes;
  if (breakdown) out.n_rides = nRides;
  out.total_time = total;
  return out;
}

/** Minute-by-minute travel times, as r5r's expanded_travel_time_matrix(). */
export function expandedTravelTimeMatrix(network, origins, destinations, options) {
  const opt = withDefaults(options, { breakdown: false }, [...COMMON, "departureDatetime", "timeWindow"]);
  const warn = warner(opt);
  const o = pointsInput(origins, "origins", warn);
  const d = pointsInput(destinations, "destinations", warn);
  const modeList = assignMode(opt.mode, opt.modeEgress);
  checkExpandedMode(modeList);
  const departure = assignDeparture(opt.departureDatetime, modeList);
  assertNetwork(network);
  checkTransitAvailability(network, departure, warn);
  const lim = streetLimits(opt, modeList);
  assertCount(opt.timeWindow, "timeWindow", true);
  const seed = commonChecks(opt);
  checkOutputDir(opt.outputDir);
  assertFlag(opt.breakdown, "breakdown");
  checkScenarios(opt);
  const res = rawExpanded(
    network._raw(), o.lat, o.lon, d.lat, d.lon, false, departure.yyyymmdd, departure.seconds,
    Math.round(opt.timeWindow), lim.walk, lim.trip, opt.walkSpeed, Math.round(opt.maxRides),
    Math.round(opt.drawsPerMinute), seed,
  );
  return expandedResult(res, o, d, opt.breakdown, lim.trip);
}

/** Travel-time matrix for a time of arrival, as r5r's arrival_travel_time_matrix(). */
export function arrivalTravelTimeMatrix(network, origins, destinations, options) {
  const opt = withDefaults(options, { arrivalDatetime: null, breakdown: false }, COMMON);
  const warn = warner(opt);
  if (opt.arrivalDatetime === null || opt.arrivalDatetime === undefined) {
    throw new RangeError(
      "'arrivalDatetime' is required. Supply a datetime value specifying when trips must arrive. For trips that do not depend on time, use travelTimeMatrix().",
    );
  }
  checkThreads(opt.nThreads);
  assertNetwork(network);
  const o = pointsInput(origins, "origins", warn);
  const d = pointsInput(destinations, "destinations", warn);
  const modeList = assignMode(opt.mode, opt.modeEgress);
  checkExpandedMode(modeList);
  const lim = streetLimits(opt, modeList);
  // the departure is arrival - max_trip_duration, as r5r computes it
  const departure = assignDeparture(minusMinutes(opt.arrivalDatetime, lim.trip, "arrivalDatetime"), modeList, "arrivalDatetime");
  checkTransitAvailability(network, departure, warn);
  const seed = commonChecks(opt);
  checkOutputDir(opt.outputDir);
  assertFlag(opt.breakdown, "breakdown");
  checkScenarios(opt);
  // the engine's time window is max_trip_duration here
  const res = rawExpanded(
    network._raw(), o.lat, o.lon, d.lat, d.lon, true, departure.yyyymmdd, departure.seconds, lim.trip,
    lim.walk, lim.trip, opt.walkSpeed, Math.round(opt.maxRides), Math.round(opt.drawsPerMinute), seed,
  );
  return expandedResult(res, o, d, opt.breakdown, lim.trip);
}
