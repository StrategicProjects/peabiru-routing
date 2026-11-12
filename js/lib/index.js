// peabiru: multimodal routing on OpenStreetMap and GTFS networks, in
// WebAssembly.
//
// The functions mirror the Python and R packages of peabiru, which mirror
// r5r 2.5.0: the same arguments (in camelCase), the same defaults, the
// same checks and the same results. The argument checks, the trip limits
// and the assembly of the result columns live here; the routing runs in
// the Rust core, compiled to WebAssembly (pkg/peabiru_bg.wasm), on the
// calling thread or in a pool of workers (lib/worker.js), each with its
// own copy of the network, that share out the origins of a call.

import wasmInit, {
  initSync as initSyncRaw,
  Network as RawNetwork,
  accessibility as rawAccessibility,
  core_version as rawCoreVersion,
  expanded as rawExpanded,
  fare_check as rawFareCheck,
  fare_read as rawFareRead,
  fare_setup as rawFareSetup,
  fare_write as rawFareWrite,
  pareto as rawPareto,
  find_snap as rawFindSnap,
  gtfs_errors as rawGtfsErrors,
  isochrone_lines as rawIsochroneLines,
  itineraries as rawItineraries,
  isochrone_polygons as rawIsochronePolygons,
  street_bbox as rawStreetBbox,
  street_edges as rawStreetEdges,
  street_ttm as rawStreetTtm,
  street_vertices as rawStreetVertices,
  transit_routes as rawTransitRoutes,
  transit_service_dates as rawTransitServiceDates,
  transit_stops as rawTransitStops,
  transit_ttm as rawTransitTtm,
} from "../pkg/peabiru.js";

const RAW = {
  street_ttm: rawStreetTtm,
  transit_ttm: rawTransitTtm,
  accessibility: rawAccessibility,
  expanded: rawExpanded,
  itineraries: rawItineraries,
  isochrone_polygons: rawIsochronePolygons,
  street_edges: rawStreetEdges,
  availability: (raw, date) => raw.transit_availability(date),
  find_snap: rawFindSnap,
  street_bbox: rawStreetBbox,
  street_vertices: rawStreetVertices,
  transit_stops: rawTransitStops,
  transit_routes: rawTransitRoutes,
  transit_service_dates: rawTransitServiceDates,
  fare_setup: rawFareSetup,
  fare_check: rawFareCheck,
  pareto: rawPareto,
};

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
// the compiled module, which the workers of a pool instantiate
let wasmModule = null;

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

const isNode = typeof process !== "undefined" && process.versions != null && process.versions.node != null;

/** Compile the module from whatever `init()` was given. */
async function compile(input) {
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
  input = await input;
  if (input instanceof WebAssembly.Module) return input;
  if (typeof input === "string" || input instanceof URL || (typeof Request === "function" && input instanceof Request)) {
    input = await fetch(input);
  }
  if (typeof Response === "function" && input instanceof Response) {
    if (!input.ok) throw new Error(`failed to fetch Wasm: ${input.status} ${input.statusText} fetching '${input.url}'`);
    if (typeof WebAssembly.compileStreaming === "function") {
      try {
        return await WebAssembly.compileStreaming(input.clone());
      } catch {
        // served without the application/wasm type: compile the bytes
      }
    }
    input = await input.arrayBuffer();
  }
  return WebAssembly.compile(input);
}

async function load(input) {
  const module = await compile(input);
  await wasmInit({ module_or_path: module });
  wasmModule = module;
}

/**
 * Load the WebAssembly module synchronously from its bytes or a compiled
 * WebAssembly.Module.
 */
export function initSync(module) {
  if (ready) return;
  if (!(module instanceof WebAssembly.Module)) module = new WebAssembly.Module(module);
  initSyncRaw({ module });
  wasmModule = module;
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

function unsupported(what, instead = "") {
  throw new UnsupportedError(`${what} is not supported by peabiru yet${instead}.`);
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
function pointsInput(points, name, warn, uniqueIds = true) {
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
  if (uniqueIds && dups.length) {
    const d = uniq(dups).map((i) => `"${i}"`);
    const vals = d.length <= 2 ? d.join(" and ") : `${d.slice(0, -1).join(", ")}, and ${d[d.length - 1]}`;
    throw new RangeError(`\`${name}\` must not contain duplicated ids.\n\u2716 Duplicated id${d.length === 1 ? "" : "s"}: ${vals}.`);
  }
  return { ids, lat: coords.lat, lon: coords.lon, points, length: n };
}

const TRANSIT_MODES = ["TRANSIT", "TRAM", "SUBWAY", "RAIL", "BUS", "FERRY", "CABLE_CAR", "GONDOLA", "FUNICULAR"];
const DIRECT_MODES = ["WALK", "BICYCLE", "CAR", "BICYCLE_RENT", "CAR_PARK"];

/** r5r assign_mode(). */
function assignMode(mode, modeEgress) {
  const all = [...TRANSIT_MODES, ...DIRECT_MODES];
  const modes = uniq(asStrings(mode, "mode").map((m) => m.toUpperCase()));
  const setOf = (v) => `{${uniq(v).map((m) => `'${m}'`).join(",")}}`;
  const extra = modes.filter((m) => !all.includes(m));
  if (extra.length) fail("mode", `Names must be a subset of ${setOf(all)}, but has additional elements ${setOf(extra)}`);
  if (Array.isArray(modeEgress) && modeEgress.every((m) => typeof m === "string")) {
    const repeated = uniq(modeEgress.map((m) => m.toUpperCase()));
    if (repeated.length === 1) modeEgress = repeated[0];
  }
  assertString(modeEgress, "modeEgress");
  const egress = modeEgress.toUpperCase();
  if (!["WALK", "BICYCLE", "CAR"].includes(egress)) fail("modeEgress", `Names must be a subset of {'WALK','BICYCLE','CAR'}, but has additional elements {'${egress}'}`);

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
    transitMode: transitModes.join(";"),
    accessMode: access.join(";"),
    egressMode,
    transit: !(transitModes.length === 1 && transitModes[0] === ""),
  };
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


// r5r set_new_congestion() and set_new_lts(), with the checks of
// congestion_poly2geojson(), dt_to_speed_map(), lts_lines2shp() and
// dt_to_lts_map(). A table is an object of columns or an array of row
// objects (as the points); r5r's sf frames are GeoJSON FeatureCollections
// in WGS 84 with the attributes as properties. The object returned is what
// the engine reads (src/scenario.rs): the columns as text, as r5r hands
// them to R5 with as.character() (Java parses them), the geometries
// flattened; null when the scenario changes nothing.
function quoted(v) {
  return `{${v.map((x) => `'${x}'`).join(",")}}`;
}

function scenarioNames(table, must, name) {
  const missing = Array.isArray(table) ? [] : must.filter((c) => table[c] === undefined);
  if (missing.length) fail(name, `Names must include the elements ${quoted(must)}, but is missing elements ${quoted(missing)}`);
}

// checkmate's assert_numeric / assert_integer / assert_character with
// any.missing = FALSE
function scenarioColumn(values, kind, name, { allMissing = true, lower = -Infinity, upper = Infinity } = {}) {
  const v = Array.from(values ?? []);
  const isNa = (x) => x === null || x === undefined || (typeof x === "number" && Number.isNaN(x));
  const ok = v.every((x) => {
    if (isNa(x)) return true;
    if (kind === "character") return typeof x === "string";
    if (kind === "integer") return typeof x === "number" && Number.isInteger(x);
    return typeof x === "number";
  });
  if (!ok) {
    const t = v.find((x) => !isNa(x) && !(kind === "integer" && typeof x === "number" && Number.isInteger(x)));
    const got = typeof t === "string" ? "character" : typeof t === "boolean" ? "logical" : typeof t === "number" ? "double" : typeof t;
    fail(name, `Must be of type '${kind}', not '${got}'`, TypeError);
  }
  if (!allMissing && v.every(isNa)) fail(name, "Contains only missing values");
  const na = v.findIndex(isNa);
  if (na >= 0) fail(name, `Contains missing values (element ${na + 1})`);
  const low = v.findIndex((x) => x < lower);
  if (low >= 0) fail(name, `Element ${low + 1} is not >= ${lower}`);
  const high = v.findIndex((x) => x > upper);
  if (high >= 0) fail(name, `Element ${high + 1} is not <= ${upper}`);
  return v;
}

// R's as.character() of a number, as far as Java's parsers can tell: an
// integer as written, else 15 significant digits in fixed or scientific
// notation as R chooses (100000 as a double is "1e+05").
function rText(x, integer) {
  if (integer) return String(x);
  if (x === Infinity) return "Inf";
  if (x === -Infinity) return "-Inf";
  const [m, e] = x.toExponential(14).split("e");
  const mantissa = m.replace(/0+$/, "").replace(/\.$/, "");
  const exp = Number(e);
  const sci = `${mantissa}e${exp < 0 ? "-" : "+"}${String(Math.abs(exp)).padStart(2, "0")}`;
  const digits = mantissa.replace(/[-.]/g, "").length;
  const fixed = x.toFixed(Math.min(100, Math.max(0, digits - 1 - exp)));
  return fixed.length <= sci.length ? fixed : sci;
}

function geojsonFeatures(fc, name) {
  if (fc === null || typeof fc !== "object" || fc.type !== "FeatureCollection" || !Array.isArray(fc.features)) {
    fail(name, "Must be a GeoJSON FeatureCollection", TypeError);
  }
  // GeoJSON is WGS 84; a crs member (old GeoJSON) must say so
  const crs = fc.crs?.properties?.name;
  if (crs !== undefined && !/(EPSG::?4326|CRS84)$/.test(crs)) {
    throw new RangeError(`The CRS of parameter \`${name}\` must be WGS 84 (EPSG 4326).`);
  }
  return fc.features;
}

function isFeatureCollection(x) {
  return x !== null && typeof x === "object" && x.type === "FeatureCollection";
}

function featureProperties(features, must, name) {
  const props = features.length ? Object.keys(features[0]?.properties ?? {}) : [];
  const missing = must.filter((k) => !props.includes(k));
  if (missing.length) {
    fail(name, `Names must include the elements ${quoted([...must, "geometry"])}, but is missing elements ${quoted(missing)}`);
  }
  return (k) => features.map((f) => f?.properties?.[k]);
}

function geometryTypes(features, choices, name) {
  const types = [...new Set(features.map((f) => f?.geometry?.type))];
  const upper = (t) => String(t).replace(/([a-z])([A-Z])/g, "$1$2").toUpperCase();
  const want = quoted(choices.map(upper));
  if (!types.length) fail(name, `Must be a subset of ${want}, not empty`);
  const extra = types.filter((x) => !choices.includes(x));
  if (extra.length) fail(name, `Must be a subset of ${want}, but has additional elements ${quoted(extra.map(upper))}`);
}

// r5r's warning when a scenario is used: R5's scenario copy of the network
// has no slope costs (r5r issue #555). r5r gives it for every scenario; here
// only on a network with elevation, where it is true.
function elevationWarning(network, warn) {
  if (network && network.info().elevation !== "NONE") {
    warn("A scenario was used for this calculation. This may affect results for WALK or BICYCLE due to missing elevation. See issue #555.");
  }
}

function scenarioSpec(o, network, warn) {
  // r5r's check (checkmate::assert_numeric(lower = 0, finite = TRUE)), then
  // a clear error for NaN, which r5r lets through and then fails on
  if (!isNumber(o.carspeedScale)) {
    fail("carspeedScale", `Must be of type 'numeric', not '${o.carspeedScale === null ? "null" : typeof o.carspeedScale}'`, TypeError);
  }
  if (o.carspeedScale < 0) fail("carspeedScale", "Element 1 is not >= 0");
  if (o.carspeedScale === Infinity) fail("carspeedScale", "Must be finite");
  if (Number.isNaN(o.carspeedScale)) {
    throw new RangeError("'carspeedScale' must be a number that is not NaN (r5r 2.5.0 fails on this value with an internal R error)");
  }
  const spec = { car: "none", carspeed_scale: o.carspeedScale, lts: "none" };
  let changed = false;
  const given = (x) => x !== null && x !== undefined;
  if (given(o.newCarspeeds) || o.carspeedScale !== 1) {
    changed = true;
    elevationWarning(network, warn);
    const t = o.newCarspeeds;
    if (isFeatureCollection(t)) {
      const features = geojsonFeatures(t, "newCarspeeds");
      const prop = featureProperties(features, ["poly_id", "scale", "priority"], "names(new_speeds_poly)");
      scenarioColumn(prop("poly_id"), "character", "new_speeds_poly$poly_id");
      const scale = scenarioColumn(prop("scale"), "numeric", "new_speeds_poly$scale");
      const priority = scenarioColumn(prop("priority"), "integer", "new_speeds_poly$priority");
      geometryTypes(features, ["Polygon", "MultiPolygon"], "unique(as.character(sf::st_geometry_type(new_speeds_poly)))");
      const x = [], y = [], ringLen = [], ringPolygon = [], polygonFeature = [];
      features.forEach((f, k) => {
        const parts = f.geometry.type === "MultiPolygon" ? f.geometry.coordinates : [f.geometry.coordinates];
        for (const poly of parts) {
          for (const ring of poly) {
            // the engine writes them as GDAL does for R5
            for (const c of ring) {
              x.push(c[0]);
              y.push(c[1]);
            }
            ringLen.push(ring.length);
            ringPolygon.push(polygonFeature.length);
          }
          polygonFeature.push(k);
        }
      });
      Object.assign(spec, {
        car: "polygon", poly_x: Float64Array.from(x), poly_y: Float64Array.from(y),
        ring_len: Int32Array.from(ringLen), ring_polygon: Int32Array.from(ringPolygon),
        polygon_feature: Int32Array.from(polygonFeature),
        scale: Float64Array.from(scale), priority: Float64Array.from(priority),
      });
    } else if (given(t)) {
      if (typeof t !== "object") {
        fail("newCarspeeds", "Must be a table (an object of columns or an array of rows) or a GeoJSON FeatureCollection", TypeError);
      }
      scenarioNames(t, ["osm_id", "max_speed", "speed_type"], "names(dt)");
      const ids = scenarioColumn(column(t, "osm_id"), "numeric", "dt$osm_id", { allMissing: false });
      const speeds = scenarioColumn(column(t, "max_speed"), "numeric", "dt$max_speed", { allMissing: false });
      const types = [...new Set(Array.from(column(t, "speed_type") ?? []))];
      if (!(types.length === 1 && ["km/h", "scale"].includes(types[0]))) {
        fail('length(unique(dt$speed_type)) == 1 && dt$speed_type[1] %in% c("km/h", "scale")', "Must be TRUE");
      }
      Object.assign(spec, {
        car: types[0],
        car_ids: ids.map((v) => rText(v, Number.isSafeInteger(v))),
        car_values: speeds.map((v) => rText(v, false)),
      });
    }
  }
  if (given(o.newLts)) {
    changed = true;
    elevationWarning(network, warn);
    const t = o.newLts;
    if (isFeatureCollection(t)) {
      const features = geojsonFeatures(t, "newLts");
      const prop = featureProperties(features, ["line_id", "lts", "priority"], "names(new_lts_lines)");
      scenarioColumn(prop("line_id"), "character", "new_lts_lines$line_id");
      const lts = scenarioColumn(prop("lts"), "integer", "new_lts_lines$lts", { lower: 1, upper: 4 });
      scenarioColumn(prop("priority"), "integer", "new_lts_lines$priority");
      geometryTypes(features, ["LineString"], "unique(as.character(sf::st_geometry_type(new_lts_lines)))");
      const x = [], y = [], len = [];
      for (const f of features) {
        for (const c of f.geometry.coordinates) {
          x.push(c[0]);
          y.push(c[1]);
        }
        len.push(f.geometry.coordinates.length);
      }
      Object.assign(spec, {
        lts: "line", line_x: Float64Array.from(x), line_y: Float64Array.from(y),
        line_len: Int32Array.from(len), line_lts: Int32Array.from(lts),
      });
    } else {
      if (typeof t !== "object") {
        fail("newLts", "Must be a table (an object of columns or an array of rows) or a GeoJSON FeatureCollection", TypeError);
      }
      scenarioNames(t, ["osm_id", "lts"], "names(dt)");
      const ids = scenarioColumn(column(t, "osm_id"), "numeric", "dt$osm_id", { allMissing: false });
      const lts = scenarioColumn(column(t, "lts"), "integer", "dt$lts", { allMissing: false });
      Object.assign(spec, {
        lts: "way",
        lts_ids: ids.map((v) => rText(v, Number.isSafeInteger(v))),
        lts_values: lts.map((v) => String(v)),
      });
    }
  }
  return changed ? spec : null;
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

/**
 * The seed handed to the core: with a fare structure and no `seed`, none
 * (R5's McRAPTOR then samples its departure times from the origin alone, as
 * in r5r with `seed = NULL`); otherwise commonChecks()'s.
 */
function mcSeed(seed, opt, fares) {
  return fares !== null && (opt.seed === null || opt.seed === undefined) ? null : seed;
}

/** r5r check_transit_availability_on_date(). */
async function checkTransitAvailabilityOnDate(network, departure, warn, signal) {
  const [total, active] = await network._availability(departure.yyyymmdd, signal);
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
  const names = new Set([...known, ...Object.keys(extra), "onWarning", "signal"]);
  for (const k of Object.keys(o)) {
    if (!names.has(k)) throw new TypeError(`unknown option '${k}'`);
  }
  const out = { ...extra };
  for (const k of known) out[k] = DEFAULTS[k];
  for (const [k, v] of Object.entries(o)) if (v !== undefined) out[k] = v;
  const signal = out.signal;
  if (signal !== undefined && signal !== null && !(typeof signal === "object" && typeof signal.aborted === "boolean" && typeof signal.addEventListener === "function")) {
    fail("signal", "Must be an AbortSignal", TypeError);
  }
  return out;
}

const COMMON = [
  "mode", "modeEgress", "maxWalkTime", "maxBikeTime", "maxCarTime", "maxTripDuration", "walkSpeed",
  "bikeSpeed", "maxRides", "maxLts", "newCarspeeds", "carspeedScale", "newLts", "drawsPerMinute", "seed",
  "nThreads", "verbose", "progress", "outputDir",
];

// ---------------------------------------------------------------------------
// The pool of workers

const ERRORS = { Error, TypeError, RangeError, UnsupportedError, RuntimeError: WebAssembly.RuntimeError };

/** An error sent by a worker, rebuilt with the class it had there. */
function revive({ name, message }) {
  const Kind = Object.hasOwn(ERRORS, name) ? ERRORS[name] : Error;
  const e = new Kind(message);
  if (e.name !== name) e.name = name;
  return e;
}

function abortError(signal) {
  if (signal.reason !== undefined) return signal.reason;
  return typeof DOMException === "function" ? new DOMException("This operation was aborted", "AbortError") : Object.assign(new Error("This operation was aborted"), { name: "AbortError" });
}

const WORKER_FILE = "./worker.js";

/** Start a worker running lib/worker.js, with one interface in Node and browsers. */
async function spawnWorker() {
  let w;
  if (isNode) {
    const { Worker: NodeWorker } = await import(/* webpackIgnore: true */ /* @vite-ignore */ "node:worker_threads");
    // a worker inherits the process's flags, and --input-type (for code
    // given with -e or on stdin) would stop it from loading worker.js
    const inputType = (a) => a.startsWith("--input-type");
    const options = process.execArgv.some(inputType) ? { execArgv: process.execArgv.filter((a) => !inputType(a)) } : {};
    // not in the form bundlers look for: this branch only runs unbundled, in Node
    w = new NodeWorker(new URL(WORKER_FILE, import.meta.url), options);
    return {
      post: (msg, transfer) => w.postMessage(msg, transfer),
      listen(onMessage, onError) {
        w.on("message", onMessage);
        w.on("error", onError);
        w.on("exit", (code) => onError(new Error(`a peabiru worker stopped (exit code ${code})`)));
      },
      terminate: () => {
        w.removeAllListeners();
        w.on("error", () => {});
        w.terminate();
      },
      ref: () => w.ref(),
      unref: () => w.unref(),
    };
  }
  if (typeof Worker !== "function") throw new Error("Workers are not available here: build the network with nThreads: 1");
  // this exact form lets bundlers (Vite, webpack 5) find and emit the worker
  w = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
  return {
    post: (msg, transfer) => w.postMessage(msg, transfer),
    listen(onMessage, onError) {
      w.onmessage = (e) => onMessage(e.data);
      w.onerror = (e) => {
        e.preventDefault();
        onError(new Error(`a peabiru worker failed: ${e.message || "could not load lib/worker.js"}`));
      };
      w.onmessageerror = () => onError(new Error("a peabiru worker sent a message that could not be read"));
    },
    terminate: () => {
      w.onmessage = w.onerror = w.onmessageerror = null;
      w.terminate();
    },
    ref() {},
    unref() {},
  };
}

/**
 * Workers that each hold a copy of the network. A call is a list of tasks
 * (one per share of the origins); idle workers take them from one queue.
 * A worker that is stopped (an aborted call, a crash) is replaced by a new
 * one built from the kept input bytes.
 */
class Pool {
  constructor(size, module, input) {
    this.module = module;
    // { pbf, feeds, tif, elevation }: what every worker builds its network from
    this.input = input;
    this.queue = [];
    // key -> the slot that last ran a task with that key (see pump())
    this.affinity = new Map();
    this.closed = false;
    this.nextId = 0;
    this.slots = Array.from({ length: size }, () => this.start(true));
    this.ready = Promise.all(this.slots.map((s) => s.built));
  }

  get size() {
    return this.slots.length;
  }

  /** A new worker, building its network. */
  start(initial = false) {
    const slot = { state: "building", task: null, initial, worker: null, info: null, warnings: null };
    slot.built = new Promise((resolve, reject) => {
      slot.onBuilt = resolve;
      slot.onFailed = reject;
    });
    // an initial build that fails rejects buildNetwork(); a later one is
    // reported to the calls waiting for workers
    if (!initial) slot.built.catch(() => {});
    spawnWorker().then(
      (worker) => {
        if (this.closed || slot.state === "stopped") {
          worker.terminate();
          return;
        }
        slot.worker = worker;
        worker.listen(
          (msg) => this.message(slot, msg),
          (e) => this.crash(slot, e),
        );
        this.refs();
        const { pbf, feeds, tif, elevation } = this.input;
        worker.post({ type: "build", module: this.module, pbf, feeds, tif, elevation }, []);
      },
      (e) => this.failed(slot, e),
    );
    return slot;
  }

  failed(slot, error) {
    if (slot.state === "stopped") return;
    slot.state = "failed";
    slot.error = error;
    if (slot.worker) slot.worker.terminate();
    slot.worker = null;
    slot.onFailed(error);
    this.pump();
  }

  message(slot, msg) {
    if (msg.type === "ready") {
      slot.state = "idle";
      slot.initial = false;
      slot.info = msg.info;
      slot.warnings = msg.warnings;
      slot.onBuilt();
      this.pump();
    } else if (msg.type === "failed") {
      this.failed(slot, revive(msg.error));
    } else if (msg.type === "done") {
      const task = slot.task;
      slot.task = null;
      slot.state = "idle";
      if (msg.fatal) this.replace(slot);
      if (task && task.id === msg.id) {
        if (msg.error) task.reject(revive(msg.error));
        else task.resolve(msg.result);
      }
      this.pump();
    }
  }

  /** The worker died (an uncaught error, or it exited). */
  crash(slot, error) {
    if (slot.state === "stopped" || slot.state === "failed") return;
    if (slot.state === "building") {
      this.failed(slot, error);
      return;
    }
    const task = slot.task;
    this.replace(slot);
    if (task) task.reject(error);
    this.pump();
  }

  /** Stop a worker and put a new one, building, in its place. */
  replace(slot) {
    const k = this.slots.indexOf(slot);
    slot.state = "stopped";
    if (slot.worker) slot.worker.terminate();
    slot.worker = null;
    slot.task = null;
    if (k >= 0 && !this.closed) this.slots[k] = this.start();
  }

  /**
   * Hand queued tasks to idle workers, in order. A task with a `key` goes
   * to the worker that last ran a task with the same key when that one is
   * idle: it may have kept what the task needs (a surface's grid linkage).
   */
  pump() {
    while (this.queue.length) {
      const task = this.queue[0];
      const last = task.key === undefined ? undefined : this.affinity.get(task.key);
      const slot = last && last.state === "idle" ? last : this.slots.find((s) => s.state === "idle");
      if (!slot) break;
      this.queue.shift();
      if (task.key !== undefined) this.affinity.set(task.key, slot);
      slot.state = "busy";
      slot.task = task;
      task.slot = slot;
      slot.worker.post({ type: "call", id: task.id, fn: task.fn, args: task.args }, task.transfer);
      // the arguments now belong to the worker
      task.args = task.transfer = null;
    }
    if (this.queue.length && this.slots.every((s) => s.state === "failed")) {
      const error = this.slots[0].error;
      for (const task of this.queue.splice(0)) task.reject(error);
    }
    this.refs();
  }

  /**
   * In Node, a worker keeps the process alive only while it works for a
   * call (or builds a network a call waits for): a program can end without
   * freeing its networks.
   */
  refs() {
    const waiting = this.queue.length > 0;
    for (const s of this.slots) {
      if (!s.worker) continue;
      if (s.state === "busy" || (s.state === "building" && (waiting || s.initial))) s.worker.ref();
      else s.worker.unref();
    }
  }

  /**
   * Run `fn` once per entry of `calls` (`{args, transfer}`), each on a
   * worker; resolves to the results in the order of `calls`. Tasks with a
   * `key` prefer the worker that last ran that key (see pump()).
   */
  run(fn, calls, signal, key) {
    if (this.closed) return Promise.reject(new Error("this Network was freed"));
    return new Promise((resolve, reject) => {
      const tasks = [];
      const results = new Array(calls.length);
      let left = calls.length;
      let settled = false;
      const finish = (error) => {
        if (settled) return;
        settled = true;
        if (signal) signal.removeEventListener("abort", onAbort);
        if (error === undefined) {
          resolve(results);
          return;
        }
        // the call failed: drop its tasks that did not start; those that
        // run finish and are ignored (or are stopped, on abort)
        this.queue = this.queue.filter((t) => !tasks.includes(t));
        reject(error);
      };
      const onAbort = () => {
        if (settled) return;
        for (const t of tasks) if (t.slot && t.slot.task === t) this.replace(t.slot);
        finish(abortError(signal));
        this.pump();
      };
      calls.forEach((c, k) => {
        const task = {
          id: this.nextId++,
          fn,
          key,
          args: c.args,
          transfer: c.transfer || [],
          slot: null,
          resolve: (r) => {
            results[k] = r;
            if (--left === 0) finish();
          },
          reject: (e) => finish(e),
        };
        tasks.push(task);
        this.queue.push(task);
      });
      if (signal) signal.addEventListener("abort", onAbort, { once: true });
      this.pump();
    });
  }

  close() {
    if (this.closed) return;
    this.closed = true;
    const error = new Error("this Network was freed");
    for (const s of this.slots) {
      const task = s.task;
      s.state = "stopped";
      if (s.worker) s.worker.terminate();
      s.worker = null;
      s.task = null;
      if (task) task.reject(error);
    }
    for (const task of this.queue.splice(0)) task.reject(error);
    this.input = null;
  }
}

/** Number of cores: `navigator.hardwareConcurrency`, or Node's `os.availableParallelism()`. */
async function cores() {
  const n = globalThis.navigator && globalThis.navigator.hardwareConcurrency;
  if (Number.isInteger(n) && n > 0) return n;
  if (isNode) {
    const os = await import(/* webpackIgnore: true */ /* @vite-ignore */ "node:os");
    return typeof os.availableParallelism === "function" ? os.availableParallelism() : os.cpus().length || 1;
  }
  return 1;
}

/**
 * The input bytes as the workers receive them: in one SharedArrayBuffer
 * where there is one (Node, and pages that are cross-origin isolated), so
 * that every worker reads the same memory; otherwise in a copy of our own,
 * which each worker receives as a structured clone that it drops once its
 * network is built. Either way the pool keeps this one copy, to build a
 * new worker after an aborted call.
 */
function shareBytes(bytes) {
  const canShare = typeof SharedArrayBuffer === "function" && globalThis.crossOriginIsolated !== false;
  if (canShare) {
    const out = new Uint8Array(new SharedArrayBuffer(bytes.byteLength));
    out.set(bytes);
    return out;
  }
  return bytes.slice();
}

// ---------------------------------------------------------------------------
// The network

const PRIVATE = Symbol("peabiru.Network");

/**
 * A transport network built by `buildNetwork()`: the street network and,
 * when GTFS feeds were given, the public transport layer, in the memory of
 * the WebAssembly module, or in the memory of each of its workers. Call
 * `free()` when done with it.
 */
export class Network {
  #raw = null;
  #pool = null;
  #info;
  #warnings;
  #freed = false;

  /** @internal use buildNetwork() */
  constructor(token, raw, pool, hasTif = false) {
    if (token !== PRIVATE) throw new TypeError("use buildNetwork() to build a Network");
    this._hasTif = hasTif;
    if (raw) {
      this.#raw = raw;
      const [stops, patterns, services, frequencies] = raw.transit_sizes();
      this.#info = { vertices: raw.n_vertices(), edges: raw.n_edges(), elevation: raw.elevation(), feeds: Array.from(raw.feed_ids()), stops, patterns, services, frequencies: frequencies === 1 };
      this.#warnings = Array.from(raw.r5r_warnings());
    } else {
      this.#pool = pool;
      this.#info = pool.slots[0].info;
      this.#warnings = pool.slots[0].warnings;
    }
  }

  #alive() {
    if (this.#freed) throw new Error("this Network was freed");
  }

  /** Release the network's memory and stop its workers; the network cannot be used afterwards. */
  free() {
    if (this.#freed) return;
    this.#freed = true;
    if (this.#raw) this.#raw.free();
    if (this.#pool) this.#pool.close();
    this.#raw = this.#pool = null;
  }

  /** Workers of the network: 0 when it routes on the calling thread. */
  get workers() {
    this.#alive();
    return this.#pool ? this.#pool.size : 0;
  }

  /** Sizes of the network. */
  info() {
    this.#alive();
    return { ...this.#info, feeds: [...this.#info.feeds] };
  }

  /**
   * Services in the network and services running on a date (a Date, or a
   * string "YYYY-MM-DD"), as r5r counts them before routing.
   */
  async transitAvailability(date) {
    let p;
    if (typeof date === "string" && /^\d{4}-\d{2}-\d{2}$/.test(date.trim())) p = clockParts(`${date.trim()} 00:00:00`, "date");
    else p = clockParts(date, "date");
    const [services, active] = await this._availability(departureOf(p).yyyymmdd);
    return { services, active };
  }

  /**
   * Warnings about the build, which buildNetwork() also hands to
   * `onWarning`: those about the elevation raster (samples r5r stores
   * wrapped around, beyond +/-3276.7 m, such as SRTM's -32768 voids, or
   * equal to the raster's no-data value). GTFS feeds that r5r refuses to
   * build a network from are an error instead, as in r5r.
   */
  r5rWarnings() {
    this.#alive();
    return [...this.#warnings];
  }

  /** @internal `[services, active]` on a yyyymmdd date. */
  async _availability(yyyymmdd, signal) {
    this.#alive();
    if (this.#raw) return this.#raw.transit_availability(yyyymmdd);
    const [r] = await this.#pool.run("availability", [{ args: [yyyymmdd] }], signal);
    return r;
  }

  /** @internal `RAW[fn](network, ...args)` on one instance: inline, or on one worker. */
  async _one(fn, args, signal) {
    this.#alive();
    if (signal && signal.aborted) throw abortError(signal);
    if (this.#raw) return RAW[fn](this.#raw, ...args);
    const [r] = await this.#pool.run(fn, [{ args }], signal);
    return r;
  }

  /** @internal The street edges (the columns of the core's street_edges), from one instance. */
  async _edges(signal) {
    this.#alive();
    if (signal && signal.aborted) throw abortError(signal);
    if (this.#raw) return RAW.street_edges(this.#raw);
    const [r] = await this.#pool.run("street_edges", [{ args: [] }], signal);
    return r;
  }

  /**
   * @internal Run a routing function on the origins: inline, or on up to
   * `nThreads` workers, each with a contiguous share of the origins.
   * `args(olat, olon)` gives the arguments after the network. Resolves to
   * `[{offset, result}]` in origin order. Shares with the same `key` go to
   * the same workers when they are idle (see Pool.pump()).
   */
  async _route(fn, o, args, nThreads, signal, key) {
    this.#alive();
    if (signal && signal.aborted) throw abortError(signal);
    if (this.#raw) return [{ offset: 0, result: RAW[fn](this.#raw, ...args(o.lat, o.lon)) }];
    const pool = this.#pool;
    const wanted = nThreads === null || nThreads === undefined || !Number.isFinite(nThreads) ? pool.size : Math.round(nThreads);
    // one contiguous block per worker: smaller blocks were slower (every
    // block links the destinations and sets a call up again)
    const k = Math.max(1, Math.min(pool.size, wanted, o.length));
    const calls = [];
    const offsets = [];
    for (let c = 0, start = 0; c < k; c++) {
      const len = Math.floor(o.length / k) + (c < o.length % k ? 1 : 0);
      const lat = o.lat.slice(start, start + len);
      const lon = o.lon.slice(start, start + len);
      calls.push({ args: args(lat, lon), transfer: [lat.buffer, lon.buffer] });
      offsets.push(start);
      start += len;
    }
    const results = await pool.run(fn, calls, signal, key);
    return results.map((result, c) => ({ offset: offsets[c], result }));
  }
}

/** The input's `nThreads`: 1 by default; 0, null or Infinity for all cores. */
async function buildThreads(o) {
  if (!("nThreads" in o) || o.nThreads === undefined) return 1;
  const n = o.nThreads;
  if (n === null || n === 0 || n === Infinity) return cores();
  assertCount(n, "nThreads", true);
  return Math.round(n);
}

/**
 * Build a transport network from an OpenStreetMap extract (`pbf`), zero
 * or more GTFS feeds and an optional elevation raster (`tif`), as r5r's
 * build_network() does from a directory.
 */
export async function buildNetwork(input) {
  if (input === null || typeof input !== "object") fail("input", "Must be an object {pbf, gtfs}", TypeError);
  const o = optionsObject(input, "input");
  for (const k of Object.keys(o)) {
    if (!["pbf", "gtfs", "tif", "elevation", "onWarning", "nThreads", "useWorkers"].includes(k)) throw new TypeError(`unknown option '${k}'`);
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
  const tif = o.tif === undefined || o.tif === null ? undefined : bytesOf(o.tif, "tif");
  // r5r set_elevation(); without a raster, "NONE" as r5r uses
  let elevation = o.elevation === undefined ? "TOBLER" : o.elevation;
  assertString(elevation, "elevation");
  elevation = elevation.toUpperCase();
  const validElevation = ["TOBLER", "MINETTI", "NONE"];
  if (!validElevation.includes(elevation)) {
    throw new Error(`Invalid value for 'elevation': "${elevation}". Must be one of: ${validElevation.map((v) => `"${v}"`).join(", ")}.`);
  }
  if (tif === undefined) elevation = "NONE";
  const threads = await buildThreads(o);
  const useWorkers = o.useWorkers === undefined ? o.nThreads !== undefined && o.nThreads !== 1 : o.useWorkers;
  assertFlag(useWorkers, "useWorkers");
  if (!useWorkers && threads > 1) fail("useWorkers", "Must be true when nThreads is not 1");
  await init();
  let network;
  if (!useWorkers) {
    network = new Network(PRIVATE, RawNetwork.build(pbf, feeds, tif, elevation), null, tif !== undefined);
  } else {
    const pool = new Pool(threads, wasmModule, {
      pbf: shareBytes(pbf),
      feeds: feeds.map(([n, b]) => [n, shareBytes(b)]),
      tif: tif === undefined ? undefined : shareBytes(tif),
      elevation,
    });
    try {
      await pool.ready;
    } catch (e) {
      pool.close();
      throw e;
    }
    network = new Network(PRIVATE, null, pool, tif !== undefined);
  }
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

/** One typed array from the arrays of the shares of the origins, in order. */
function concat(arrays) {
  if (arrays.length === 1) return arrays[0];
  let n = 0;
  for (const a of arrays) n += a.length;
  const out = new arrays[0].constructor(n);
  let at = 0;
  for (const a of arrays) {
    out.set(a, at);
    at += a.length;
  }
  return out;
}

/**
 * The columns of the shares of the origins as one result: column 0 holds
 * origin indices, which each share counts from its own first origin.
 */
function merge(parts, ncols = parts[0].result.length) {
  for (const { offset, result } of parts) {
    if (offset === 0) continue;
    const from = result[0];
    for (let i = 0; i < from.length; i++) from[i] += offset;
  }
  const out = [];
  for (let c = 0; c < ncols; c++) out.push(concat(parts.map((p) => p.result[c])));
  return out;
}

/** merge() for expanded matrices, whose routes are codes into each share's own levels. */
function mergeExpanded(parts) {
  const CODES = 11;
  if (parts.length === 1) return parts[0].result;
  const levels = [];
  const index = new Map();
  for (const { result } of parts) {
    const local = result[CODES + 1].map((r) => {
      let k = index.get(r);
      if (k === undefined) {
        k = levels.length;
        levels.push(r);
        index.set(r, k);
      }
      return k;
    });
    const codes = result[CODES];
    for (let i = 0; i < codes.length; i++) if (codes[i] >= 0) codes[i] = local[codes[i]];
  }
  const out = merge(parts, CODES + 1);
  out.push(levels);
  return out;
}

/**
 * Travel-time matrix between origins and destinations, as r5r's
 * travel_time_matrix().
 */
export async function travelTimeMatrix(network, origins, destinations, options) {
  const opt = withDefaults(options, {}, [...COMMON, "departureDatetime", "timeWindow", "percentiles", "fareStructure", "maxFare"]);
  const warn = warner(opt);
  let o = pointsInput(origins, "origins", warn);
  let d = pointsInput(destinations, "destinations", warn);
  const modeList = assignMode(opt.mode, opt.modeEgress);
  const departure = assignDeparture(opt.departureDatetime, modeList);
  assertNetwork(network);
  if (modeList.transit) await checkTransitAvailabilityOnDate(network, departure, warn, opt.signal);
  // r5r reverse_if_direct_mode(): walking alone with more origins than
  // destinations routes from the destinations, unless the network was
  // built with an elevation raster (given, whatever the elevation model)
  const swapped = !modeList.transit && modeList.directModes === "WALK" && o.length > d.length && !network._hasTif;
  if (swapped) [o, d] = [d, o];
  const lim = streetLimits(opt, modeList);
  assertCount(opt.timeWindow, "timeWindow", true);
  const pcts = assignPercentiles(opt.percentiles);
  const seed = commonChecks(opt);
  checkMcDraws(opt.drawsPerMinute, opt.timeWindow, opt.fareStructure !== null && opt.fareStructure !== undefined);
  const fares = await fareArg(opt.fareStructure, opt.maxFare, network, opt.signal);
  checkOutputDir(opt.outputDir);
  const scenario = scenarioSpec(opt, network, warn);

  let cols;
  if (modeList.transit) {
    const parts = await network._route(
      "transit_ttm", o,
      (olat, olon) => [
        olat, olon, d.lat, d.lon, modeList.transitMode, modeList.accessMode, modeList.egressMode,
        opt.bikeSpeed, Math.round(opt.maxLts), lim.bike, lim.car, departure.yyyymmdd, departure.seconds, Math.round(opt.timeWindow),
        Int32Array.from(pcts), lim.walk, lim.trip, opt.walkSpeed, Math.round(opt.maxRides),
        Math.round(opt.drawsPerMinute), mcSeed(seed, opt, fares), scenario, fares, opt.maxFare,
      ],
      opt.nThreads, opt.signal,
    );
    cols = merge(parts);
  } else {
    const parts = await network._route(
      "street_ttm", o,
      (olat, olon) => [olat, olon, d.lat, d.lon, modeList.directModes, opt.walkSpeed, opt.bikeSpeed, Math.round(opt.maxLts), lim.trip, scenario],
      opt.nThreads, opt.signal,
    );
    const [from, to, tt] = merge(parts);
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

// ---------------------------------------------------------------------------
// Fare structures (r5r R/fare_structure.R)

const FARE_TYPE_COLS = ["type", "unlimited_transfers", "allow_same_route_transfer", "use_route_fare", "fare"];
const FARE_TRANSFER_COLS = ["first_leg", "second_leg", "fare"];
const FARE_ROUTE_COLS = ["agency_id", "agency_name", "route_id", "route_short_name", "route_long_name", "mode", "route_fare", "fare_type"];
const FARE_ELEMENTS = [
  "max_discounted_transfers", "transfer_time_allowance", "fare_cap", "fares_per_type", "fares_per_transfer", "fares_per_route", "debug_settings",
];

/** The core's flat object as r5r's fare structure (tables as objects of columns). */
function fareFromCore(c) {
  return {
    max_discounted_transfers: c.max_discounted_transfers,
    transfer_time_allowance: c.transfer_time_allowance,
    fare_cap: c.fare_cap,
    fares_per_type: Object.fromEntries(FARE_TYPE_COLS.map((k) => [k, c[k]])),
    fares_per_transfer: { first_leg: c.first_leg, second_leg: c.second_leg, fare: c.transfer_fare },
    fares_per_route: Object.fromEntries(FARE_ROUTE_COLS.map((k) => [k, c[k]])),
    debug_settings: { output_file: c.output_file, trip_info: c.trip_info },
  };
}

/** A table's column as an array (null for missing), or []. */
function fareColumn(table, key) {
  const v = table[key];
  if (v === undefined) return [];
  return Array.from(v, (x) => (x === undefined || (typeof x === "number" && Number.isNaN(x)) ? null : x));
}

/** r5r's fare structure as the core's flat object. */
function fareToCore(fs) {
  const t = fs.fares_per_type, tr = fs.fares_per_transfer, r = fs.fares_per_route;
  const num = (x) => (x === null || x === undefined ? NaN : Number(x));
  return {
    max_discounted_transfers: num(fs.max_discounted_transfers),
    transfer_time_allowance: num(fs.transfer_time_allowance),
    fare_cap: num(fs.fare_cap),
    ...Object.fromEntries(FARE_TYPE_COLS.map((k) => [k, fareColumn(t, k)])),
    first_leg: fareColumn(tr, "first_leg"),
    second_leg: fareColumn(tr, "second_leg"),
    transfer_fare: fareColumn(tr, "fare"),
    ...Object.fromEntries(FARE_ROUTE_COLS.map((k) => [k, fareColumn(r, k)])),
    output_file: String(fs.debug_settings.output_file),
    trip_info: String(fs.debug_settings.trip_info),
  };
}

/** r5r's assert_fare_structure(): the element names here, the values and tables in the core. */
function assertFareStructure(fs) {
  if (fs === null || typeof fs !== "object" || Array.isArray(fs)) fail("fareStructure", "Must be of type 'list'", TypeError);
  if ("type" in fs) unsupported("A fare calculator of R5 (a fare structure with a 'type')");
  const names = Object.keys(fs).sort();
  if (names.join() !== [...FARE_ELEMENTS].sort().join()) {
    fail("names(fareStructure)", `Must have exactly the names {'${FARE_ELEMENTS.join("','")}'}`);
  }
  for (const n of ["max_discounted_transfers", "transfer_time_allowance", "fare_cap"]) {
    assertNumber(fs[n], `fareStructure$${n}`, { lower: 0 });
  }
  const known = { fares_per_type: FARE_TYPE_COLS, fares_per_transfer: FARE_TRANSFER_COLS, fares_per_route: FARE_ROUTE_COLS };
  for (const [n, cols] of Object.entries(known)) {
    if (fs[n] === null || typeof fs[n] !== "object") fail(`fareStructure$${n}`, "Must be an object of columns", TypeError);
    // r5r hands the columns to Java, which fails on a column it does not know
    const extra = Object.keys(fs[n]).filter((k) => !cols.includes(k));
    if (extra.length) throw new RangeError(`fareStructure$${n} has column(s) r5r cannot read: ${extra.join(", ")} (r5r fails reading the structure in Java).`);
  }
  const ds = fs.debug_settings;
  if (ds === null || typeof ds !== "object" || Object.keys(ds).sort().join() !== "output_file,trip_info") {
    fail("fareStructure$debug_settings", "Must have exactly the names {'output_file','trip_info'}");
  }
  assertString(ds.output_file, "fareStructure$debug_settings$output_file");
  assertString(ds.trip_info, "fareStructure$debug_settings$trip_info");
}

/**
 * r5r's set_fare_structure() and set_max_fare(): the structure checked and,
 * as in r5r, read in for the network's routes whatever the modes; a finite
 * maxFare needs a structure. Resolves to the core's flat object, or null.
 */
async function fareArg(fareStructure, maxFare, network, signal) {
  let core = null;
  if (fareStructure !== null && fareStructure !== undefined) {
    assertFareStructure(fareStructure);
    core = fareToCore(fareStructure);
    await network._one("fare_check", [core], signal);
  }
  assertNumber(maxFare, "maxFare", { lower: 0 });
  if (Number.isFinite(maxFare) && core === null) {
    throw new RangeError(
      `'maxFare' requires a 'fareStructure': 'maxFare' is ${maxFare} but 'fareStructure' is null, so no fares are calculated and the limit would be ignored. Pass a fare structure (see setupFareStructure()) or leave 'maxFare' as Infinity.`,
    );
  }
  return core;
}

/**
 * r5r set_monte_carlo_draws(): with McRAPTOR (a fare structure, or the
 * alternatives of detailedItineraries()), the draws R5 cannot sample are
 * rejected as r5r rejects them.
 */
function checkMcDraws(drawsPerMinute, timeWindow, mcraptor) {
  if (!mcraptor) return;
  const dpm = Math.round(drawsPerMinute);
  const draws = Math.trunc(timeWindow * dpm);
  if (draws === 200) {
    throw new RangeError(
      `A 'timeWindow' of ${timeWindow} minutes with ${dpm} draw(s) per minute gives exactly 200 Monte Carlo draws, which R5's McRaptor algorithm does not accept. Use a slightly different 'timeWindow', e.g. ${timeWindow + 1}. See https://github.com/ipea/r5r/issues/602.`,
    );
  }
  const m = Math.floor(60 / dpm);
  const mu = (m - 1) / 2 + Math.floor(m / 2);
  const span = 60 * timeWindow;
  let p = 0;
  if (m >= 2) {
    const sd = Math.sqrt((span * (m * m - 1)) / 12 / mu ** 3);
    const z = ((span - (m - 1) / 2) / mu + 1 - draws) / sd;
    p = Math.exp((-z * z) / 2) / Math.sqrt(2 * Math.PI) / sd;
  }
  if (p < 5e-5) {
    throw new RangeError(
      `'drawsPerMinute' = ${dpm} with a 'timeWindow' of ${timeWindow} minutes is not supported with a 'fareStructure'. R5's McRaptor algorithm cannot sample this many departure times and could run indefinitely. Use a lower 'drawsPerMinute' or a shorter 'timeWindow'. See https://github.com/ipea/r5r/issues/602.`,
    );
  }
}

/**
 * A fare structure as r5r's setup_fare_structure(): every route, fare type
 * and transfer between two types costs `baseFare` (read as a Java float, as
 * in r5r), with one fare type per mode (`by: "MODE"`), per agency
 * (`"AGENCY_ID"`, `"AGENCY_NAME"`) or one for all routes (`"GENERIC"`).
 */
export async function setupFareStructure(network, baseFare, options = {}) {
  const o = optionsObject(options);
  for (const k of Object.keys(o)) {
    if (!["by", "debugPath", "debugInfo"].includes(k)) throw new TypeError(`unknown option '${k}'`);
  }
  assertNetwork(network);
  if (typeof baseFare !== "number") fail("baseFare", "Must be of type 'numeric'", TypeError);
  if (Number.isNaN(baseFare)) fail("baseFare", "Contains missing values (element 1)");
  if (baseFare < 0) fail("baseFare", "Element 1 is not >= 0");
  const options_ = ["MODE", "AGENCY_ID", "AGENCY_NAME", "GENERIC"];
  const by = o.by === undefined ? "MODE" : o.by;
  assertString(by, "by");
  const byU = by.toUpperCase();
  if (!options_.includes(byU)) fail("by", `Must be a subset of {'${options_.join("','")}'}, but has additional elements {'${byU}'}`);
  let debugPath = o.debugPath === undefined ? null : o.debugPath;
  let debugInfo = o.debugInfo === undefined ? null : o.debugInfo;
  if (debugPath !== null) {
    assertString(debugPath, "debugPath");
    if (!debugPath.endsWith(".csv")) fail("debugPath", "Must comply to pattern '\\.csv$'");
  }
  if (debugPath === null && debugInfo !== null) throw new RangeError("Please specify a file to write debug info to with 'debugPath'.");
  if (debugPath !== null && debugInfo === null) debugInfo = "ROUTE";
  if (debugInfo !== null) {
    assertString(debugInfo, "debugInfo");
    debugInfo = debugInfo.toUpperCase();
    if (!["MODE", "ROUTE", "MODE_ROUTE"].includes(debugInfo)) {
      fail("debugInfo", `Must be a subset of {'MODE','ROUTE','MODE_ROUTE'}, but has additional elements {'${debugInfo}'}`);
    }
  }
  const fs = fareFromCore(await network._one("fare_setup", [baseFare, byU]));
  fs.debug_settings = debugPath !== null ? { output_file: debugPath, trip_info: debugInfo } : { output_file: "", trip_info: "MODE" };
  return fs;
}

/**
 * A fare structure read from the bytes of a `.zip` file, as r5r's
 * read_fare_structure() (encoding "UTF-8", "unknown" or "Latin-1").
 */
export function readFareStructure(bytes, encoding = "UTF-8") {
  needInit();
  const b = bytesOf(bytes, "bytes");
  if (!["unknown", "UTF-8", "Latin-1"].includes(encoding)) {
    fail("encoding", `Must be a subset of {'unknown','UTF-8','Latin-1'}, but has additional elements {'${encoding}'}`);
  }
  return fareFromCore(rawFareRead(b, encoding));
}

/**
 * The bytes of a fare structure `.zip` file, as r5r's write_fare_structure()
 * writes it: the same five CSV files, stored uncompressed.
 */
export function writeFareStructure(fareStructure) {
  needInit();
  assertFareStructure(fareStructure);
  return rawFareWrite(fareToCore(fareStructure));
}

/**
 * The Pareto frontier of travel time and fare between origins and
 * destinations, as r5r's pareto_frontier().
 */
export async function paretoFrontier(network, origins, destinations, options) {
  const opt = withDefaults(
    options,
    // r5r's pareto_frontier() routes on foot and by public transport by default
    { mode: ["WALK", "TRANSIT"], fareStructure: null, fareCutoffs: null },
    [...COMMON.filter((k) => !["mode", "drawsPerMinute", "newCarspeeds", "carspeedScale", "newLts"].includes(k)), "departureDatetime", "timeWindow", "percentiles"],
  );
  if (opt.fareStructure === null || opt.fareStructure === undefined) {
    throw new RangeError("'fareStructure' is required. Create one with setupFareStructure() or readFareStructure().");
  }
  if (opt.fareCutoffs === null || opt.fareCutoffs === undefined) {
    throw new RangeError("'fareCutoffs' is required. Pass the monetary cutoffs of the frontier, e.g. the fares of your 'fareStructure'.");
  }
  const warn = warner(opt);
  const o = pointsInput(origins, "origins", warn);
  const d = pointsInput(destinations, "destinations", warn);
  const modeList = assignMode(opt.mode, opt.modeEgress);
  const departure = assignDeparture(opt.departureDatetime, modeList);
  assertNetwork(network);
  if (modeList.transit) await checkTransitAvailabilityOnDate(network, departure, warn, opt.signal);
  const lim = streetLimits(opt, modeList);
  assertCount(opt.timeWindow, "timeWindow", true);
  const pcts = assignPercentiles(opt.percentiles);
  checkMcDraws(1, opt.timeWindow, modeList.transit);
  const seed = commonChecks({ ...opt, drawsPerMinute: 1 });
  const core = await fareArg(opt.fareStructure, Infinity, network, opt.signal);
  checkOutputDir(opt.outputDir);
  const cutoffs = numberList(opt.fareCutoffs, "fareCutoffs", "numeric");
  if (!cutoffs.length) fail("fareCutoffs", "Must have length >= 1, but has length 0");
  if (cutoffs.some(Number.isNaN)) fail("fareCutoffs", "Contains missing values");
  if (cutoffs.some((v) => v < 0)) fail("fareCutoffs", "Element is not >= 0");
  if (new Set(cutoffs).size !== cutoffs.length) fail("fareCutoffs", "Contains duplicated values");
  const parts = await network._route(
    "pareto", o,
    (olat, olon) => [
      olat, olon, d.lat, d.lon, modeList.accessMode, modeList.transit ? modeList.transitMode : "",
      modeList.transit ? modeList.egressMode : modeList.accessMode, departure.yyyymmdd, departure.seconds,
      Math.round(opt.timeWindow), Int32Array.from(pcts), lim.walk, lim.bike, lim.car, lim.trip, opt.walkSpeed,
      opt.bikeSpeed, Math.round(opt.maxLts), Math.round(opt.maxRides), mcSeed(seed, opt, core), core, Float64Array.from(cutoffs),
    ],
    opt.nThreads, opt.signal,
  );
  const [from, to, percentile, tt, cost] = merge(parts);
  for (let i = 0; i < tt.length; i++) if (tt[i] > lim.trip) tt[i] = NA_INT32;
  return { from_id: ids(from, o.ids), to_id: ids(to, d.ids), percentile, travel_time: tt, monetary_cost: cost };
}

const DECAY_FUNCTIONS = ["step", "exponential", "fixed_exponential", "linear", "logistic"];

/** r5r assign_decay_function(). */
function assignDecay(fun, value) {
  assertString(fun, "decayFunction");
  if (!DECAY_FUNCTIONS.includes(fun)) fail("decayFunction", `Names must be a subset of {${DECAY_FUNCTIONS.map((f) => `'${f}'`).join(",")}}, but has additional elements {'${fun}'}`);
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
export async function accessibility(network, origins, destinations, options) {
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
  if (modeList.transit) await checkTransitAvailabilityOnDate(network, departure, warn, opt.signal);
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
  checkMcDraws(opt.drawsPerMinute, opt.timeWindow, opt.fareStructure !== null && opt.fareStructure !== undefined);
  const fares = await fareArg(opt.fareStructure, opt.maxFare, network, opt.signal);
  checkOutputDir(opt.outputDir);
  const scenario = scenarioSpec(opt, network, warn);

  const parts = await network._route(
    "accessibility", o,
    (olat, olon) => [
      olat, olon, d.lat, d.lon, opp.flat, opp.names.length,
      modeList.directModes, modeList.transitMode, modeList.egressMode, fun, value, Int32Array.from(cutoffs),
      Int32Array.from(pcts), departure.yyyymmdd, departure.seconds, Math.round(opt.timeWindow), lim.walk,
      lim.bike, lim.car, lim.trip, opt.walkSpeed, opt.bikeSpeed, Math.round(opt.maxLts), Math.round(opt.maxRides),
      Math.round(opt.drawsPerMinute), mcSeed(seed, opt, fares), scenario, fares, opt.maxFare,
    ],
    opt.nThreads, opt.signal,
  );
  // one block of rows per origin: the shares follow one another
  const values = concat(parts.map((p) => p.result));
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
export async function expandedTravelTimeMatrix(network, origins, destinations, options) {
  const opt = withDefaults(options, { breakdown: false }, [...COMMON, "departureDatetime", "timeWindow"]);
  const warn = warner(opt);
  const o = pointsInput(origins, "origins", warn);
  const d = pointsInput(destinations, "destinations", warn);
  const modeList = assignMode(opt.mode, opt.modeEgress);
  checkExpandedMode(modeList);
  const departure = assignDeparture(opt.departureDatetime, modeList);
  assertNetwork(network);
  await checkTransitAvailabilityOnDate(network, departure, warn, opt.signal);
  const lim = streetLimits(opt, modeList);
  assertCount(opt.timeWindow, "timeWindow", true);
  const seed = commonChecks(opt);
  checkOutputDir(opt.outputDir);
  assertFlag(opt.breakdown, "breakdown");
  const scenario = scenarioSpec(opt, network, warn);
  const parts = await network._route(
    "expanded", o,
    (olat, olon) => [
      olat, olon, d.lat, d.lon, modeList.transitMode, modeList.accessMode, modeList.egressMode,
      opt.bikeSpeed, Math.round(opt.maxLts), lim.bike, lim.car, false, departure.yyyymmdd, departure.seconds,
      Math.round(opt.timeWindow), lim.walk, lim.trip, opt.walkSpeed, Math.round(opt.maxRides),
      Math.round(opt.drawsPerMinute), seed, scenario,
    ],
    opt.nThreads, opt.signal,
  );
  const res = mergeExpanded(parts);
  return expandedResult(res, o, d, opt.breakdown, lim.trip);
}

/** Travel-time matrix for a time of arrival, as r5r's arrival_travel_time_matrix(). */
export async function arrivalTravelTimeMatrix(network, origins, destinations, options) {
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
  await checkTransitAvailabilityOnDate(network, departure, warn, opt.signal);
  const seed = commonChecks(opt);
  checkOutputDir(opt.outputDir);
  assertFlag(opt.breakdown, "breakdown");
  const scenario = scenarioSpec(opt, network, warn);
  // the engine's time window is max_trip_duration here
  const parts = await network._route(
    "expanded", o,
    (olat, olon) => [
      olat, olon, d.lat, d.lon, modeList.transitMode, modeList.accessMode, modeList.egressMode,
      opt.bikeSpeed, Math.round(opt.maxLts), lim.bike, lim.car, true, departure.yyyymmdd, departure.seconds, lim.trip,
      lim.walk, lim.trip, opt.walkSpeed, Math.round(opt.maxRides), Math.round(opt.drawsPerMinute), seed, scenario,
    ],
    opt.nThreads, opt.signal,
  );
  const res = mergeExpanded(parts);
  return expandedResult(res, o, d, opt.breakdown, lim.trip);
}

// ---------------------------------------------------------------------------
// Detailed itineraries

/** r5r expand_od_pairs(): the pairs routed, as indices into the origins and destinations. */
function expandOdPairs(no, nd, allToAll, warn) {
  assertFlag(allToAll, "allToAll");
  const oi = [];
  const di = [];
  if (allToAll || no === 1 || nd === 1) {
    for (let i = 0; i < no; i++) for (let j = 0; j < nd; j++) { oi.push(i); di.push(j); }
    if (!allToAll && (no > 1 || nd > 1)) {
      warn(no === 1
        ? "'origins' was expanded to match the number of destinations."
        : "'destinations' was expanded to match the number of origins.");
    }
  } else if (no !== nd) {
    throw new RangeError(
      "When 'allToAll' is false, 'origins' and 'destinations' must either have the same number of rows or one of them must have only one row.",
    );
  } else {
    for (let i = 0; i < no; i++) { oi.push(i); di.push(i); }
  }
  return { oi, di };
}

function pick(p, idx) {
  const lat = new Float64Array(idx.length);
  const lon = new Float64Array(idx.length);
  const id = new Array(idx.length);
  idx.forEach((k, n) => { lat[n] = p.lat[k]; lon[n] = p.lon[k]; id[n] = p.ids[k]; });
  return { ids: id, lat, lon, length: idx.length };
}

/**
 * Detailed itineraries between origin-destination pairs, as r5r's
 * detailed_itineraries() without fares. Rows (one per leg) as objects, or
 * GeoJSON features with `dropGeometry: false` (the default).
 */
export async function detailedItineraries(network, origins, destinations, options) {
  const opt = withDefaults(
    options,
    { suboptimalMinutes: 0, shortestPath: true, allToAll: false, fareStructure: null, maxFare: Infinity, dropGeometry: false, osmLinkIds: false },
    [...COMMON, "departureDatetime", "timeWindow"],
  );
  const warn = warner(opt);
  assertNetwork(network);
  const all = opt.allToAll === true;
  let o = pointsInput(origins, "origins", warn, all);
  let d = pointsInput(destinations, "destinations", warn, all);
  const { oi, di } = expandOdPairs(o.length, d.length, opt.allToAll, warn);
  o = pick(o, oi);
  d = pick(d, di);
  const modeList = assignMode(opt.mode, opt.modeEgress);
  const departure = assignDeparture(opt.departureDatetime, modeList);
  if (modeList.transit) {
    await checkTransitAvailabilityOnDate(network, departure, warn, opt.signal);
    if (network.info().frequencies) {
      throw new RangeError(
        "Assertion on 'r5r_network' failed: None of the GTFS feeds used to create the transit network can contain a 'frequencies' table. Try using gtfstools::frequencies_to_stop_times() to create a suitable feed.",
      );
    }
  }
  const lim = streetLimits(opt, modeList);
  assertFlag(opt.shortestPath, "shortestPath");
  assertFlag(opt.dropGeometry, "dropGeometry");
  assertFlag(opt.osmLinkIds, "osmLinkIds");
  if (opt.dropGeometry && opt.osmLinkIds) {
    throw new RangeError("To set osmLinkIds to `true`, the parameter 'dropGeometry' must also be `false`");
  }
  assertCount(opt.timeWindow, "timeWindow", true);
  checkSpeed(opt.walkSpeed, "walk");
  checkSpeed(opt.bikeSpeed, "bike");
  assertCount(opt.maxRides, "maxRides", true);
  checkMaxLts(opt.maxLts);
  checkMcDraws(1, opt.timeWindow, modeList.transit && !(opt.shortestPath && (opt.fareStructure === null || opt.fareStructure === undefined)));
  checkThreads(opt.nThreads);
  const fares = await fareArg(opt.fareStructure, opt.maxFare, network, opt.signal);
  checkOutputDir(opt.outputDir);
  assertCount(opt.suboptimalMinutes, "suboptimalMinutes");
  if (fares !== null && opt.suboptimalMinutes > 0) {
    throw new RangeError("Assertion on 'suboptimalMinutes' failed: Must be 0 when calculating fares with detailedItineraries().");
  }
  if (opt.shortestPath && opt.suboptimalMinutes > 0) {
    throw new RangeError("Assertion on 'suboptimalMinutes' failed: Must be 0 when 'shortestPath' is true.");
  }
  const scenario = scenarioSpec(opt, network, warn);
  let start = 0;
  const parts = await network._route(
    "itineraries", o,
    (olat, olon) => {
      const s = start;
      start += olat.length;
      return [
        olat, olon, d.lat.slice(s, s + olat.length), d.lon.slice(s, s + olat.length),
        modeList.directModes, modeList.transitMode, modeList.egressMode, departure.yyyymmdd, departure.seconds,
        Math.round(opt.timeWindow), lim.walk, lim.bike, lim.car, lim.trip, opt.walkSpeed, opt.bikeSpeed,
        Math.round(opt.maxRides), Math.round(opt.maxLts), opt.shortestPath, scenario, opt.osmLinkIds,
        Math.round(opt.suboptimalMinutes), fares, opt.maxFare,
      ];
    },
    opt.nThreads, opt.signal,
  );
  const rows = [];
  for (const { offset, result } of parts) {
    const [pair, option, totalDistance, segment, distance, nPoints, totalDuration, segmentDuration, wait, x, y, dep, mode, route,
      osmIds, edgeIds, boardStop, alightStop, totalFare, cumulativeFare] = result;
    let at = 0;
    for (let k = 0; k < pair.length; k++) {
      const i = pair[k] + offset;
      const row = {
        from_id: o.ids[i], from_lat: o.lat[i], from_lon: o.lon[i],
        to_id: d.ids[i], to_lat: d.lat[i], to_lon: d.lon[i],
        option: option[k], departure_time: dep[k], total_duration: totalDuration[k],
        total_distance: totalDistance[k],
        ...(fares !== null ? { total_fare: totalFare[k] } : {}),
        segment: segment[k], mode: mode[k],
        ...(fares !== null ? { cumulative_fare: cumulativeFare[k] } : {}),
        segment_duration: segmentDuration[k], wait: wait[k], distance: distance[k], route: route[k],
      };
      if (opt.osmLinkIds) {
        row.osm_id_list = osmIds[k];
        row.edge_id_list = edgeIds[k];
        row.board_stop_id = boardStop[k];
        row.alight_stop_id = alightStop[k];
      }
      const n = nPoints[k];
      if (opt.dropGeometry) {
        rows.push(row);
      } else {
        const coordinates = [];
        for (let p = at; p < at + n; p++) coordinates.push([x[p], y[p]]);
        rows.push({ type: "Feature", properties: row, geometry: { type: "LineString", coordinates } });
      }
      at += n;
    }
  }
  return opt.dropGeometry ? rows : { type: "FeatureCollection", features: rows };
}

// ---------------------------------------------------------------------------
// Isochrones

const STREET_CLASSES = ["MOTORWAY", "PRIMARY", "SECONDARY", "TERTIARY", "OTHER"];

const ISOCHRONE = [
  "maxWalkTime", "maxBikeTime", "maxCarTime", "maxTripDuration", "walkSpeed", "bikeSpeed", "maxRides", "maxLts",
  "drawsPerMinute", "seed", "nThreads", "verbose", "progress", "departureDatetime", "timeWindow",
];

/** r5r's checks of the cutoffs: the cutoffs with 0, sorted and distinct, and the trip limit. */
function isochroneCutoffs(cutoffs) {
  // checkmate::assert_numeric(cutoffs, lower = 0, finite = TRUE, any.missing = FALSE, min.len = 1)
  const values = numberList(cutoffs, "cutoffs", "numeric");
  if (!values.length) fail("cutoffs", "Must have length >= 1");
  if (values.some(Number.isNaN)) fail("cutoffs", "Contains missing values");
  if (values.some((v) => !Number.isFinite(v))) fail("cutoffs", "Must be finite");
  if (values.some((v) => v < 0)) fail("cutoffs", "All elements must be >= 0");
  // the largest cutoff is the trip limit (as.integer() truncates)
  const limit = Math.trunc(Math.max(...values));
  const cuts = [...new Set([0, ...values])].sort((a, b) => a - b);
  if (cuts.length < 2) throw new RangeError("`cutoffs` must contain at least one value greater than 0.");
  return { cuts, limit };
}

/** The MultiPolygon coordinates of each row, from the flat columns. */
function multipolygons(nPolygons, nRings, nPoints, x, y) {
  const out = new Array(nPolygons.length);
  let ring = 0;
  let poly = 0;
  let point = 0;
  for (let r = 0; r < nPolygons.length; r++) {
    const polys = [];
    for (let p = 0; p < nPolygons[r]; p++, poly++) {
      const rings = [];
      for (let k = 0; k < nRings[poly]; k++, ring++) {
        const coords = new Array(nPoints[ring]);
        for (let i = 0; i < nPoints[ring]; i++, point++) coords[i] = [x[point], y[point]];
        rings.push(coords);
      }
      polys.push(rings);
    }
    out[r] = polys;
  }
  return out;
}

/**
 * Isochrones, as r5r's isochrone(): the areas (`polygonOutput: true`) or
 * the street segments (`polygonOutput: false`) reached from each origin
 * within each cutoff, as a GeoJSON FeatureCollection.
 */
export async function isochrone(network, origins, options) {
  const given = optionsObject(options);
  const opt = withDefaults(
    options,
    { mode: "transit", modeEgress: "walk", cutoffs: [0, 15, 30], zoom: 10, polygonOutput: true, percentiles: null, sampleSize: undefined },
    ISOCHRONE,
  );
  const warn = warner(opt);
  // r5r's sample_size is deprecated: any value warns
  if ("sampleSize" in given) warn("The `sample_size` argument is no longer used and has no effect.");
  assertNetwork(network);
  const { cuts, limit } = isochroneCutoffs(opt.cutoffs);
  assertFlag(opt.polygonOutput, "polygonOutput");
  return opt.polygonOutput ? isochronePolygons(network, origins, opt, cuts, limit, warn) : isochroneLines(network, origins, opt, cuts, limit, warn);
}

async function isochronePolygons(network, origins, opt, cuts, limit, warn) {
  const percentiles = opt.percentiles === null || opt.percentiles === undefined ? 50 : opt.percentiles;
  assertNumber(opt.zoom, "zoom", { lower: 9, upper: 12 });
  assertInt(opt.zoom, "zoom");
  // r5r's travel_time_surface(), routed past max(cutoffs) so the outer band
  // is interpolated like the inner ones
  const o = pointsInput(origins, "origins", warn);
  const modeList = assignMode(opt.mode, opt.modeEgress);
  const departure = assignDeparture(opt.departureDatetime, modeList);
  const lim = streetLimits({ ...opt, maxTripDuration: limit + 10 }, modeList);
  assertCount(opt.timeWindow, "timeWindow", true);
  const pcts = assignPercentiles(percentiles);
  const seed = commonChecks(opt);
  const parts = await network._route(
    "isochrone_polygons", o,
    (olat, olon) => [
      olat, olon, modeList.directModes, modeList.transitMode, modeList.egressMode, Float64Array.from(cuts), Math.round(opt.zoom),
      Int32Array.from(pcts), departure.yyyymmdd, departure.seconds, Math.round(opt.timeWindow), lim.walk, lim.bike, lim.car,
      lim.trip,
      opt.walkSpeed, opt.bikeSpeed, Math.round(opt.maxLts), Math.round(opt.maxRides), Math.round(opt.drawsPerMinute), seed,
    ],
    opt.nThreads, opt.signal,
    // each worker keeps the links of the grid's cells by the modes (public
    // transport links them by the access and the egress mode): a single
    // origin goes back to a worker that has them
    `grid ${Math.round(opt.zoom)} ${modeList.directModes} ${modeList.egressMode}`,
  );
  const [origin, iso, pct, nPolygons, nRings, nPoints, x, y] = merge(parts);
  const coords = multipolygons(nPolygons, nRings, nPoints, x, y);
  const features = new Array(origin.length);
  for (let r = 0; r < origin.length; r++) {
    features[r] = {
      type: "Feature",
      properties: { id: o.ids[origin[r]], isochrone: iso[r], percentile: `p${pct[r]}` },
      geometry: { type: "MultiPolygon", coordinates: coords[r] },
    };
  }
  return { type: "FeatureCollection", features };
}

async function isochroneLines(network, origins, opt, cuts, limit, warn) {
  if (opt.percentiles !== null && opt.percentiles !== undefined) {
    throw new RangeError("Percentiles not supported for line-based isochrones");
  }
  const [edgeIndex, osmId, fromVertex, toVertex, classes, length, walk, car, carSpeed, bicycle, lts, lat, lon, nPoints, x, y] =
    await network._edges(opt.signal);
  const dest = { id: Array.from(edgeIndex, String), lon, lat };
  const ttm = await travelTimeMatrix(network, origins, dest, {
    mode: opt.mode, modeEgress: opt.modeEgress, departureDatetime: opt.departureDatetime, timeWindow: opt.timeWindow,
    maxWalkTime: opt.maxWalkTime, maxBikeTime: opt.maxBikeTime, maxCarTime: opt.maxCarTime, maxTripDuration: limit,
    walkSpeed: opt.walkSpeed, bikeSpeed: opt.bikeSpeed, maxRides: opt.maxRides, maxLts: opt.maxLts,
    drawsPerMinute: opt.drawsPerMinute, seed: opt.seed, nThreads: opt.nThreads, verbose: opt.verbose,
    progress: opt.progress, onWarning: warn, signal: opt.signal,
  });
  // travelTimeMatrix() has already warned about these origins
  const ids = pointsInput(origins, "origins", () => {}).ids;
  const nO = ids.length;
  const nE = edgeIndex.length;
  const originRow = new Map(ids.map((id, i) => [id, i]));
  const edgeRow = new Map(dest.id.map((id, i) => [id, i]));
  const minutes = new Int32Array(nO * nE).fill(NA_INT32);
  const tt = ttm.travel_time_p50;
  for (let k = 0; k < tt.length; k++) {
    minutes[originRow.get(ttm.from_id[k]) * nE + edgeRow.get(ttm.to_id[k])] = tt[k];
  }
  const [origin, edge, time, iso] = rawIsochroneLines(nO, nE, minutes, Float64Array.from(cuts));
  const ends = new Float64Array(nE);
  for (let i = 0, at = 0; i < nE; i++) ends[i] = at += nPoints[i];
  const features = new Array(origin.length);
  for (let r = 0; r < origin.length; r++) {
    const e = edge[r];
    const coords = new Array(nPoints[e]);
    for (let i = 0, p = ends[e] - nPoints[e]; i < nPoints[e]; i++, p++) coords[i] = [x[p], y[p]];
    features[r] = {
      type: "Feature",
      properties: {
        id: ids[origin[r]],
        edge_index: String(edgeIndex[e]),
        osm_id: osmId[e],
        isochrone: iso[r],
        travel_time_p50: time[r],
        from_vertex: fromVertex[e],
        to_vertex: toVertex[e],
        street_class: STREET_CLASSES[classes[e]],
        length: length[e],
        walk: walk[e] === 1,
        car: car[e] === 1,
        car_speed: carSpeed[e],
        bicycle: bicycle[e] === 1,
        bicycle_lts: lts[e],
      },
      geometry: { type: "LineString", coordinates: coords },
    };
  }
  return { type: "FeatureCollection", features };
}

// ---------------------------------------------------------------------------
// Inspecting a network: r5r's find_snap(), street_network_bbox(),
// street_network_to_sf(), transit_network_to_sf(),
// check_transit_availability() and get_gtfs_errors()

/**
 * Where points snap to the street network, as r5r's find_snap(): the
 * nearest point of a street the mode may use, first within 300 metres and
 * then within `radius`. Columns `point_id`, `lat`, `lon`, `snap_lat`,
 * `snap_lon`, `distance` (metres; NaN where the point does not snap) and
 * `found`.
 */
export async function findSnap(network, points, options) {
  const opt = optionsObject(options);
  for (const k of Object.keys(opt)) {
    if (!["radius", "mode", "onWarning", "signal"].includes(k)) throw new TypeError(`unknown option '${k}'`);
  }
  const warn = warner(opt);
  assertNetwork(network);
  let radius = opt.radius === undefined ? 1600 : opt.radius;
  const mode = opt.mode === undefined ? "WALK" : opt.mode;
  // checkmate::assert_numeric(radius, lower = 0, finite = TRUE, max.len = 1)
  if (!isNumber(radius)) fail("radius", "Must be of type 'numeric'", TypeError);
  if (radius === Infinity || radius === -Infinity) fail("radius", "Must be finite");
  if (radius < 0) fail("radius", "Element 1 is not >= 0");
  // checkmate lets NA through and R5 then snaps nothing
  if (Number.isNaN(radius)) radius = 0;
  assertString(mode, "mode");
  if (!["WALK", "BICYCLE", "CAR"].includes(mode)) {
    fail("mode", `Must be a subset of {'WALK','BICYCLE','CAR'}, but has additional elements {'${mode}'}`);
  }
  const p = pointsInput(points, "points", warn);
  const [snapLat, snapLon, distance, found] = await network._one("find_snap", [p.lat, p.lon, radius, mode], opt.signal);
  return {
    point_id: p.ids,
    lat: p.lat,
    lon: p.lon,
    snap_lat: snapLat,
    snap_lon: snapLon,
    distance,
    found: Array.from(found, (v) => v === 1),
  };
}

/**
 * The bounding box of the street network, as r5r's street_network_bbox():
 * R5's envelope of the street layer. `output` is "polygon" (a GeoJSON
 * Polygon, the default), "bbox" (`[xmin, ymin, xmax, ymax]`, a GeoJSON
 * bbox) or "vector" (`{xmin, ymin, xmax, ymax}`).
 */
export async function streetNetworkBbox(network, output = "polygon") {
  assertNetwork(network);
  const choices = ["polygon", "bbox", "vector"];
  if (!choices.includes(output)) throw new RangeError(`'arg' should be one of ${choices.map((c) => `"${c}"`).join(", ")}`);
  const [xmin, ymin, xmax, ymax] = await network._one("street_bbox", []);
  if (output === "vector") return { xmin, ymin, xmax, ymax };
  if (output === "bbox") return [xmin, ymin, xmax, ymax];
  // the ring of sf::st_as_sfc() of a bbox
  return { type: "Polygon", coordinates: [[[xmin, ymin], [xmax, ymin], [xmax, ymax], [xmin, ymax], [xmin, ymin]]] };
}

/** LineString geometries from `n_points` and flat `x`, `y`. */
function lineCoordinates(nPoints, x, y) {
  const out = new Array(nPoints.length);
  for (let i = 0, p = 0; i < nPoints.length; i++) {
    const c = new Array(nPoints[i]);
    for (let k = 0; k < nPoints[i]; k++, p++) c[k] = [x[p], y[p]];
    out[i] = c;
  }
  return out;
}

/**
 * The street network as GeoJSON, as r5r's street_network_to_sf():
 * `vertices` (Points with `index`, `park_and_ride`, `bike_sharing`) and
 * `edges` (LineStrings with `edge_index`, `osm_id`, `from_vertex`,
 * `to_vertex`, `street_class`, `length` in metres, `walk`, `car`,
 * `car_speed` in km/h, `bicycle`, `bicycle_lts`), in r5r's order.
 */
export async function streetNetworkToSf(network, options) {
  const opt = optionsObject(options);
  assertNetwork(network);
  const [index, lat, lon, pnr, bs] = await network._one("street_vertices", [], opt.signal);
  const vertices = new Array(index.length);
  for (let i = 0; i < index.length; i++) {
    vertices[i] = {
      type: "Feature",
      properties: { index: index[i], park_and_ride: pnr[i] === 1, bike_sharing: bs[i] === 1 },
      geometry: { type: "Point", coordinates: [lon[i], lat[i]] },
    };
  }
  const [edgeIndex, osmId, fromVertex, toVertex, classes, length, walk, car, carSpeed, bicycle, lts, , , nPoints, x, y] =
    await network._edges(opt.signal);
  const lines = lineCoordinates(nPoints, x, y);
  const edges = new Array(edgeIndex.length);
  for (let e = 0; e < edgeIndex.length; e++) {
    edges[e] = {
      type: "Feature",
      properties: {
        edge_index: edgeIndex[e],
        osm_id: osmId[e],
        from_vertex: fromVertex[e],
        to_vertex: toVertex[e],
        street_class: STREET_CLASSES[classes[e]],
        length: length[e],
        walk: walk[e] === 1,
        car: car[e] === 1,
        car_speed: carSpeed[e],
        bicycle: bicycle[e] === 1,
        bicycle_lts: lts[e],
      },
      geometry: { type: "LineString", coordinates: lines[e] },
    };
  }
  return {
    vertices: { type: "FeatureCollection", features: vertices },
    edges: { type: "FeatureCollection", features: edges },
  };
}

/**
 * The public transport network as GeoJSON, as r5r's
 * transit_network_to_sf(): `stops` (Points with `stop_index`, `stop_id`,
 * `stop_name`, `linked_to_street`; a stop not linked to the streets has a
 * null geometry) and `routes` (one LineString per trip pattern, with
 * `agency_id`, `agency_name`, `route_id`, `long_name`, `short_name` and
 * `mode`; patterns without a line are left out, and a line whose points
 * are all the same is the Point sf::st_make_valid() makes of it).
 */
export async function transitNetworkToSf(network, options) {
  const opt = optionsObject(options);
  assertNetwork(network);
  const [agencyId, agencyName, routeId, longName, shortName, mode, nPoints, x, y] = await network._one("transit_routes", [], opt.signal);
  const lines = lineCoordinates(nPoints, x, y);
  const routes = [];
  for (let i = 0; i < lines.length; i++) {
    const c = lines[i];
    if (c.length === 0) continue;
    const point = c.every((q) => q[0] === c[0][0] && q[1] === c[0][1]);
    routes.push({
      type: "Feature",
      properties: {
        agency_id: agencyId[i],
        agency_name: agencyName[i],
        route_id: routeId[i],
        long_name: longName[i],
        short_name: shortName[i],
        mode: mode[i],
      },
      geometry: point ? { type: "Point", coordinates: c[0] } : { type: "LineString", coordinates: c },
    });
  }
  const [stopIndex, stopId, stopName, lat, lon, linked] = await network._one("transit_stops", [], opt.signal);
  const stops = new Array(stopIndex.length);
  for (let i = 0; i < stopIndex.length; i++) {
    stops[i] = {
      type: "Feature",
      properties: { stop_index: stopIndex[i], stop_id: stopId[i], stop_name: stopName[i], linked_to_street: linked[i] === 1 },
      geometry: linked[i] === 1 ? { type: "Point", coordinates: [lon[i], lat[i]] } : null,
    };
  }
  return {
    stops: { type: "FeatureCollection", features: stops },
    routes: { type: "FeatureCollection", features: routes },
  };
}

/** r5r parse_date_input(): "YYYY-MM-DD" strings or Dates (their local date). */
function parseDates(x, arg) {
  const values = typeof x === "string" || x instanceof Date ? [x] : x !== null && typeof x === "object" && typeof x[Symbol.iterator] === "function" ? Array.from(x) : null;
  if (values === null) throw new TypeError(`\`${arg}\` must be a vector of character strings or Date objects.`);
  return values.map((v) => {
    if (v instanceof Date) {
      if (Number.isNaN(v.getTime())) throw new RangeError(`Could not parse all values in \`${arg}\`.`);
      return { y: v.getFullYear(), mo: v.getMonth() + 1, d: v.getDate() };
    }
    if (typeof v !== "string") throw new TypeError(`\`${arg}\` must be a vector of character strings or Date objects.`);
    const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(v);
    if (!m) throw new RangeError(`Invalid date format found in \`${arg}\`. Please use the strict 'YYYY-MM-DD' format for all date strings.`);
    const [y, mo, d] = m.slice(1).map(Number);
    const t = new Date(Date.UTC(y, mo - 1, d));
    if (t.getUTCFullYear() !== y || t.getUTCMonth() !== mo - 1 || t.getUTCDate() !== d) {
      throw new RangeError(`Could not parse all values in \`${arg}\`. One or more dates are logically invalid (e.g., '2025-02-29').`);
    }
    return { y, mo, d };
  });
}

/**
 * Services active on dates, as r5r's check_transit_availability(): with
 * no `dates`, `startDate` or `endDate`, the dates with an active service
 * used by some trip are discovered from the network. Columns `date`
 * ("YYYY-MM-DD"), `total_services`, `active_services` and `pct_active`.
 */
export async function checkTransitAvailability(network, options) {
  const opt = optionsObject(options);
  for (const k of Object.keys(opt)) {
    if (!["dates", "startDate", "endDate", "signal"].includes(k)) throw new TypeError(`unknown option '${k}'`);
  }
  assertNetwork(network);
  const has = (k) => opt[k] !== undefined && opt[k] !== null;
  const vector = has("dates") && !has("startDate") && !has("endDate");
  const range = !has("dates") && has("startDate") && has("endDate");
  const discover = !has("dates") && !has("startDate") && !has("endDate");
  if (!discover && !vector && !range) {
    throw new RangeError(
      "Incorrect date arguments provided. Omit all date arguments for discovery, or use one of these methods: use the `dates` option to provide a vector of specific dates, or use both `startDate` and `endDate` to provide a continuous date range. You cannot mix these methods or provide an incomplete date range.",
    );
  }
  let days;
  if (discover) {
    const found = await network._one("transit_service_dates", [], opt.signal);
    days = Array.from(found, (v) => ({ y: Math.floor(v / 10000), mo: Math.floor(v / 100) % 100, d: v % 100 }));
  } else if (vector) {
    days = parseDates(opt.dates, "dates");
  } else {
    const start = parseDates(opt.startDate, "startDate");
    const end = parseDates(opt.endDate, "endDate");
    if (start.length > 1 || end.length > 1) throw new RangeError("`startDate` and `endDate` must each be a single date.");
    const t0 = Date.UTC(start[0].y, start[0].mo - 1, start[0].d);
    const t1 = Date.UTC(end[0].y, end[0].mo - 1, end[0].d);
    if (t0 > t1) throw new RangeError("`startDate` must be before or the same as `endDate`.");
    days = [];
    for (let t = t0; t <= t1; t += 86400000) {
      const u = new Date(t);
      days.push({ y: u.getUTCFullYear(), mo: u.getUTCMonth() + 1, d: u.getUTCDate() });
    }
  }
  const n = days.length;
  const out = {
    date: days.map((p) => `${pad(p.y, 4)}-${pad(p.mo)}-${pad(p.d)}`),
    total_services: new Int32Array(n),
    active_services: new Int32Array(n),
    pct_active: new Float64Array(n),
  };
  for (let i = 0; i < n; i++) {
    const [total, active] = await network._availability(days[i].y * 10000 + days[i].mo * 100 + days[i].d, opt.signal);
    out.total_services[i] = total;
    out.active_services[i] = active;
    out.pct_active[i] = total === 0 ? 0 : active / total;
  }
  return out;
}

/**
 * The errors gtfs-lib (R5's GTFS library) reports in GTFS feeds, as r5r's
 * get_gtfs_errors() (which reads them from the gtfs_errors.csv its
 * build_network() writes). A Network does not keep its feeds, so pass
 * them as buildNetwork() takes them: an array of `{name, data}`, or an
 * object with a `gtfs` array. Columns `file`, `line`, `type`, `field`,
 * `id` and `priority`, feed by feed in the order gtfs-lib finds them.
 * Throws an Error naming the cause for a feed on which gtfs-lib fails
 * instead of listing errors (r5r's build_network() fails then too).
 */
export async function getGtfsErrors(feeds) {
  if (feeds instanceof Network) {
    throw new TypeError("a Network does not keep its GTFS feeds: pass them as buildNetwork() takes them ({gtfs: [{name, data}]})");
  }
  const list = Array.isArray(feeds) ? feeds : feeds !== null && typeof feeds === "object" && Array.isArray(feeds.gtfs) ? feeds.gtfs : null;
  if (list === null) fail("feeds", "Must be an array of {name, data} or an object {gtfs}", TypeError);
  await init();
  const cols = [[], [], [], [], [], []];
  list.forEach((f, i) => {
    if (f === null || typeof f !== "object") fail(`gtfs[${i}]`, "Must be an object {name, data}", TypeError);
    assertString(f.name, `gtfs[${i}].name`);
    const r = rawGtfsErrors(f.name, bytesOf(f.data, `gtfs[${i}].data`));
    // a loop, not push(...column): a feed can have more errors than a
    // call can take arguments
    for (let k = 0; k < 6; k++) for (const v of r[k]) cols[k].push(v);
  });
  return {
    file: cols[0],
    line: Int32Array.from(cols[1]),
    type: cols[2],
    field: cols[3],
    id: cols[4],
    priority: cols[5],
  };
}
