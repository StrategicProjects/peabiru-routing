import assert from "node:assert/strict";
import { test } from "node:test";
import * as peabiru from "../lib/index.js";
import { DEPARTURE, sampleNetwork, samplePoints, sum } from "./helpers.mjs";

const { NA_INT32, UnsupportedError } = peabiru;
const net = await sampleNetwork();
const pts = samplePoints();
const n = pts.id.length;

// Sums of the sample's matrices as the native command-line tool computes
// them on the same network, with the stops linked to the streets
// (bench/compare/js_parity.mjs compares the matrices cell by cell).
const NATIVE = {
  WALK: { rows: 729, sum: 14359 },
  BICYCLE: { rows: 729, sum: 8615 },
  CAR: { rows: 729, sum: 5440 },
  // ["WALK", "TRANSIT"], 2019-05-13 14:00, seed 42, percentile 50
  TRANSIT_P50: { rows: 729, sum: 13146 },
};

for (const mode of ["WALK", "BICYCLE", "CAR"]) {
  test(`${mode} matrix: r5r's columns, origin-major, as the native core`, () => {
    const m = peabiru.travelTimeMatrix(net, pts, pts, { mode });
    assert.deepEqual(Object.keys(m), ["from_id", "to_id", "travel_time_p50"]);
    assert.ok(m.travel_time_p50 instanceof Int32Array);
    assert.ok(Array.isArray(m.from_id) && typeof m.from_id[0] === "string");
    assert.equal(m.from_id.length, NATIVE[mode].rows);
    assert.equal(sum(m.travel_time_p50), NATIVE[mode].sum);
    assert.ok(m.travel_time_p50.every((t) => t >= 0 && t <= 120));
    const order = [...new Set(m.from_id)];
    assert.deepEqual(order, pts.id.filter((id) => order.includes(id)));
  });
}

test("a street mode reports its time for every percentile", () => {
  const m = peabiru.travelTimeMatrix(net, pts, pts, { mode: "BICYCLE", percentiles: [75, 10] });
  assert.deepEqual(Object.keys(m), ["from_id", "to_id", "travel_time_p10", "travel_time_p75"]);
  assert.deepEqual(m.travel_time_p10, m.travel_time_p75);
  assert.notEqual(m.travel_time_p10, m.travel_time_p75);
});

test("trip and mode limits drop pairs", () => {
  const full = peabiru.travelTimeMatrix(net, pts, pts);
  const short = peabiru.travelTimeMatrix(net, pts, pts, { maxTripDuration: 10 });
  const walk10 = peabiru.travelTimeMatrix(net, pts, pts, { maxWalkTime: 10 });
  assert.ok(short.travel_time_p50.every((t) => t <= 10));
  assert.deepEqual(short, walk10);
  assert.ok(short.from_id.length < full.from_id.length);
});

test("points can be an array of objects", () => {
  const rows = pts.id.map((id, i) => ({ id, lon: pts.lon[i], lat: pts.lat[i] }));
  assert.deepEqual(
    peabiru.travelTimeMatrix(net, rows, rows, { mode: "CAR" }),
    peabiru.travelTimeMatrix(net, pts, pts, { mode: "CAR" }),
  );
});

test("walking from more origins than destinations keeps the caller's columns", () => {
  const three = { id: pts.id.slice(0, 3), lon: pts.lon.slice(0, 3), lat: pts.lat.slice(0, 3) };
  const m = peabiru.travelTimeMatrix(net, pts, three);
  assert.ok(m.to_id.every((id) => three.id.includes(id)));
  assert.deepEqual(new Set(m.from_id), new Set(pts.id));
  // rows follow the routed origins, i.e. the destinations
  assert.deepEqual([...new Set(m.to_id)], three.id);
});

test("transit matrix: one column per percentile, reproducible with a seed", () => {
  const opts = { mode: ["WALK", "TRANSIT"], departureDatetime: DEPARTURE, percentiles: [95, 5, 50], seed: 1 };
  const m = peabiru.travelTimeMatrix(net, pts, pts, opts);
  assert.deepEqual(Object.keys(m), ["from_id", "to_id", "travel_time_p05", "travel_time_p50", "travel_time_p95"]);
  const [p5, p50, p95] = [m.travel_time_p05, m.travel_time_p50, m.travel_time_p95];
  for (let k = 0; k < p5.length; k++) {
    assert.notEqual(p5[k], NA_INT32);
    if (p50[k] !== NA_INT32) assert.ok(p5[k] <= p50[k]);
    if (p95[k] !== NA_INT32 && p50[k] !== NA_INT32) assert.ok(p50[k] <= p95[k]);
  }
  assert.deepEqual(peabiru.travelTimeMatrix(net, pts, pts, { ...opts, mode: "TRANSIT", nThreads: 1 }), m);
  const ref = peabiru.travelTimeMatrix(net, pts, pts, { mode: "TRANSIT", departureDatetime: DEPARTURE, seed: 42 });
  assert.equal(ref.from_id.length, NATIVE.TRANSIT_P50.rows);
  assert.equal(sum(ref.travel_time_p50), NATIVE.TRANSIT_P50.sum);
});

test("a Date is read as its local clock time", () => {
  const opts = { mode: "TRANSIT", seed: 3, timeWindow: 2 };
  assert.deepEqual(
    peabiru.travelTimeMatrix(net, pts, pts, { ...opts, departureDatetime: new Date(2019, 4, 13, 14, 0, 0) }),
    peabiru.travelTimeMatrix(net, pts, pts, { ...opts, departureDatetime: "2019-05-13T14:00" }),
  );
});

test("higher percentiles beyond the trip limit are NA", () => {
  const m = peabiru.travelTimeMatrix(net, pts, pts, {
    mode: "TRANSIT", departureDatetime: DEPARTURE, percentiles: [1, 99], maxTripDuration: 12, seed: 1,
  });
  assert.ok(m.travel_time_p01.every((t) => t <= 12 && t !== NA_INT32));
  assert.ok(m.travel_time_p99.some((t) => t === NA_INT32));
});

test("accessibility: one row per origin, opportunity, percentile and cutoff", () => {
  const a = peabiru.accessibility(net, pts, pts, {
    opportunitiesColnames: ["jobs", "schools"],
    mode: ["WALK", "TRANSIT"],
    departureDatetime: DEPARTURE,
    decayFunction: "logistic",
    cutoffs: [30, 15],
    decayValue: 5,
    percentiles: [50, 25],
    seed: 42,
  });
  assert.deepEqual(Object.keys(a), ["id", "opportunity", "percentile", "cutoff", "accessibility"]);
  assert.equal(a.accessibility.length, n * 2 * 2 * 2);
  assert.ok(a.accessibility instanceof Float64Array && a.percentile instanceof Int32Array);
  assert.deepEqual([...a.cutoff.slice(0, 4)], [15, 30, 15, 30]);
  assert.deepEqual([...a.percentile.slice(0, 4)], [25, 25, 50, 50]);
  assert.deepEqual(a.opportunity.slice(0, 5), ["jobs", "jobs", "jobs", "jobs", "schools"]);
  assert.equal(a.id[8], pts.id[1]);
  assert.ok(a.accessibility.every((v) => v >= 0));

  const step = peabiru.accessibility(net, pts, pts, { opportunitiesColnames: "jobs", mode: "CAR", cutoffs: 10 });
  const fixed = peabiru.accessibility(net, pts, pts, {
    opportunitiesColnames: "jobs", mode: "WALK", decayFunction: "fixed_exponential", decayValue: 0.1,
  });
  assert.equal(step.accessibility.length, n);
  assert.ok(!("cutoff" in fixed));
  assert.throws(
    () => peabiru.accessibility(net, pts, pts, { opportunitiesColnames: "nothing", cutoffs: 10 }),
    /missing elements \{'nothing'\}/,
  );
});

test("expanded matrix: one row per minute, with the breakdown on request", () => {
  const two = { id: pts.id.slice(0, 2), lon: pts.lon.slice(0, 2), lat: pts.lat.slice(0, 2) };
  const e = peabiru.expandedTravelTimeMatrix(net, two, pts, {
    mode: ["WALK", "TRANSIT"], departureDatetime: DEPARTURE, timeWindow: 2, breakdown: true, seed: 42,
  });
  assert.deepEqual(Object.keys(e), [
    "from_id", "to_id", "departure_time", "draw_number", "access_time", "wait_time", "ride_time",
    "transfer_time", "egress_time", "routes", "n_rides", "total_time",
  ]);
  assert.deepEqual([...new Set(e.departure_time)].sort(), ["14:00:00", "14:01:00"]);
  assert.ok(e.total_time instanceof Float64Array && e.n_rides instanceof Int32Array);
  assert.ok(e.routes.every((r) => r === null || typeof r === "string"));
  const plain = peabiru.expandedTravelTimeMatrix(net, two, pts, {
    mode: "TRANSIT", departureDatetime: DEPARTURE, timeWindow: 2, seed: 42,
  });
  assert.deepEqual(Object.keys(plain), ["from_id", "to_id", "departure_time", "draw_number", "routes", "total_time"]);
  assert.deepEqual(plain.total_time, e.total_time);
});

test("arrival matrix: every trip arrives in time", () => {
  const two = { id: pts.id.slice(0, 2), lon: pts.lon.slice(0, 2), lat: pts.lat.slice(0, 2) };
  const a = peabiru.arrivalTravelTimeMatrix(net, two, pts, {
    mode: ["WALK", "TRANSIT"], arrivalDatetime: "2019-05-13 15:00:00", maxTripDuration: 30, seed: 42,
  });
  assert.ok(a.from_id.length > 0);
  assert.ok(a.total_time.every((t) => Number.isNaN(t) || t <= 30));
  assert.throws(() => peabiru.arrivalTravelTimeMatrix(net, two, pts, { mode: "TRANSIT" }), /'arrivalDatetime' is required/);
});

test("what peabiru does not support is an UnsupportedError", () => {
  const transit = { mode: "TRANSIT", departureDatetime: DEPARTURE };
  for (const opts of [
    { mode: "BUS", departureDatetime: DEPARTURE },
    { mode: ["BICYCLE", "TRANSIT"], departureDatetime: DEPARTURE },
    { ...transit, modeEgress: "CAR" },
    { fareStructure: {} },
    { newLts: {} },
    { carspeedScale: 1.5 },
    { outputDir: "out" },
  ]) {
    assert.throws(() => peabiru.travelTimeMatrix(net, pts, pts, opts), UnsupportedError, JSON.stringify(opts));
  }
  assert.throws(() => peabiru.expandedTravelTimeMatrix(net, pts, pts, { mode: "WALK" }), UnsupportedError);
});

test("bad arguments are errors, never ignored", () => {
  const bad = [
    [{ walkSpeed: 0 }, RangeError, /'walkSpeed' failed/],
    [{ maxLts: 5 }, RangeError, /Level of Transit Stress/],
    [{ percentiles: [50, 50] }, RangeError, /duplicated/],
    [{ timeWindow: 1.5, mode: "TRANSIT", departureDatetime: DEPARTURE }, TypeError, /'timeWindow'/],
    [{ mode: "TRANSIT" }, RangeError, /must pass a 'departureDatetime'/],
    [{ mode: "TRANSIT", departureDatetime: "13/05/2019" }, TypeError, /departureDatetime/],
    [{ maxWalkTim: 3 }, TypeError, /unknown option 'maxWalkTim'/],
    [{ maxFare: 10 }, RangeError, /requires a 'fareStructure'/],
    [{ nThreads: 0 }, RangeError, /'nThreads'/],
    [{ seed: 1.5 }, TypeError, /'seed'/],
  ];
  for (const [opts, kind, msg] of bad) {
    assert.throws(() => peabiru.travelTimeMatrix(net, pts, pts, opts), (e) => e instanceof kind && msg.test(e.message), JSON.stringify(opts));
  }
  // the core's own checks come back with its message
  assert.throws(
    () => peabiru.travelTimeMatrix(net, pts, pts, { mode: "TRANSIT", departureDatetime: DEPARTURE, timeWindow: 100000, drawsPerMinute: 1000 }),
    /time_window x draws_per_minute/,
  );
});

test("ids and coordinates are checked as r5r checks them", () => {
  const warnings = [];
  const onWarning = (m) => warnings.push(m);
  const numeric = { id: pts.id.map((_, i) => i + 1), lon: pts.lon, lat: pts.lat };
  const m = peabiru.travelTimeMatrix(net, numeric, numeric, { mode: "CAR", onWarning });
  assert.equal(m.from_id[0], "1");
  assert.match(warnings.join("\n"), /'origins\.id' forcefully cast to character/);

  const lon = Float64Array.from(pts.lon);
  lon[0] = NaN;
  warnings.length = 0;
  const missing = { id: pts.id, lon, lat: pts.lat };
  const r = peabiru.travelTimeMatrix(net, missing, pts, { mode: "CAR", onWarning });
  assert.match(warnings[0], /1 point\(s\) in 'origins' have missing lon\/lat/);
  assert.ok(!r.from_id.includes(pts.id[0]));

  const dup = { id: pts.id.map((id, i) => (i === 1 ? pts.id[0] : id)), lon: pts.lon, lat: pts.lat };
  assert.throws(() => peabiru.travelTimeMatrix(net, dup, pts), /must not contain duplicated ids/);
  assert.throws(() => peabiru.travelTimeMatrix(net, { id: pts.id, lon: pts.lon }, pts), /missing elements \{'lat'\}/);
});
