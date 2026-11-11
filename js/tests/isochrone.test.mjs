// isochrone(): r5r's rows and properties as GeoJSON, the same through the
// pool of workers as on the calling thread, and r5r's checks.

import assert from "node:assert/strict";
import { test, after } from "node:test";
import * as peabiru from "../lib/index.js";
import { DEPARTURE, read, sampleNetwork, samplePoints } from "./helpers.mjs";

const net = await sampleNetwork();
const pool = await peabiru.buildNetwork({
  pbf: read("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: read("spo_gtfs.zip") }],
  nThreads: 3,
});
after(() => pool.free());

const pts = samplePoints();
const take = (idx) => ({ id: idx.map((i) => pts.id[i]), lon: idx.map((i) => pts.lon[i]), lat: idx.map((i) => pts.lat[i]) });
const two = take([0, 1]);
const four = take([0, 5, 11, 20]);
const area = (ring) => {
  let a = 0;
  for (let i = 0; i + 1 < ring.length; i++) a += ring[i][0] * ring[i + 1][1] - ring[i + 1][0] * ring[i][1];
  return Math.abs(a / 2);
};
const polygonArea = (coords) => coords.reduce((s, poly) => s + area(poly[0]) - poly.slice(1).reduce((h, r) => h + area(r), 0), 0);

test("polygons: r5r's rows and properties, nested bands", async () => {
  const x = await peabiru.isochrone(net, two, { mode: "WALK", cutoffs: [20, 0, 10, 10], percentiles: [50, 25], departureDatetime: DEPARTURE });
  assert.equal(x.type, "FeatureCollection");
  assert.equal(x.features.length, 8);
  const props = x.features.map((f) => f.properties);
  assert.deepEqual(props.map((p) => p.id), [...Array(4).fill(two.id[0]), ...Array(4).fill(two.id[1])]);
  assert.deepEqual(props.map((p) => p.percentile), ["p25", "p25", "p50", "p50", "p25", "p25", "p50", "p50"]);
  assert.deepEqual(props.map((p) => p.isochrone), [20, 10, 20, 10, 20, 10, 20, 10]);
  for (const f of x.features) {
    assert.equal(f.geometry.type, "MultiPolygon");
    for (const poly of f.geometry.coordinates) for (const ring of poly) assert.deepEqual(ring[0], ring[ring.length - 1], "closed rings");
  }
  for (let k = 0; k < 8; k += 2) {
    assert.ok(polygonArea(x.features[k].geometry.coordinates) > polygonArea(x.features[k + 1].geometry.coordinates));
  }
});

test("lines: r5r's properties and order", async () => {
  const x = await peabiru.isochrone(net, two, { mode: "WALK", cutoffs: [0, 5, 10], polygonOutput: false });
  assert.ok(x.features.length > 0);
  assert.deepEqual(Object.keys(x.features[0].properties), [
    "id", "edge_index", "osm_id", "isochrone", "travel_time_p50", "from_vertex", "to_vertex", "street_class",
    "length", "walk", "car", "car_speed", "bicycle", "bicycle_lts",
  ]);
  const ids = [];
  let prev = null;
  for (const f of x.features) {
    const p = f.properties;
    assert.equal(f.geometry.type, "LineString");
    assert.equal(typeof p.edge_index, "string");
    assert.ok(p.travel_time_p50 > 0 && p.travel_time_p50 <= p.isochrone);
    assert.ok([5, 10].includes(p.isochrone));
    if (!ids.includes(p.id)) {
      ids.push(p.id);
      prev = null;
    }
    if (prev) assert.ok(prev.isochrone > p.isochrone || (prev.isochrone === p.isochrone && prev.travel_time_p50 >= p.travel_time_p50));
    prev = p;
  }
  assert.deepEqual(ids, two.id);
});

const CASES = [
  ["WALK polygons", { mode: "WALK", cutoffs: [0, 10, 20] }],
  ["CAR polygons at zoom 12", { mode: "CAR", cutoffs: [0, 3, 6], zoom: 12 }],
  ["transit polygons, two percentiles", { mode: "TRANSIT", cutoffs: [0, 15, 30], percentiles: [25, 75], departureDatetime: DEPARTURE, seed: 42 }],
  ["BICYCLE lines", { mode: "BICYCLE", cutoffs: [0, 5, 10], polygonOutput: false }],
  ["transit lines", { mode: "TRANSIT", cutoffs: [0, 15, 25], polygonOutput: false, departureDatetime: DEPARTURE, seed: 42 }],
];
for (const [name, opts] of CASES) {
  test(`${name}: the same with 1, 2, 3 and 100 workers`, async () => {
    const ref = await peabiru.isochrone(net, four, { departureDatetime: DEPARTURE, ...opts });
    assert.ok(ref.features.length > 0);
    for (const nThreads of [1, 2, 3, 100, null]) {
      assert.deepEqual(await peabiru.isochrone(pool, four, { departureDatetime: DEPARTURE, ...opts, nThreads }), ref, `nThreads ${nThreads}`);
    }
  });
}

test("polygons called again and again, in mixed order and at once: those of a fresh network", async () => {
  const one = take([5]);
  const calls = [];
  for (const zoom of [12, 9, 11, 10]) {
    for (const mode of ["TRANSIT", "WALK", "CAR", "BICYCLE"]) calls.push({ mode, zoom, cutoffs: [0, 10, 20], departureDatetime: DEPARTURE, seed: 42 });
  }
  // a network that never routed, for each call
  const want = [];
  for (const opts of calls) {
    const fresh = await peabiru.buildNetwork({ pbf: read("spo_osm.pbf"), gtfs: [{ name: "spo_gtfs.zip", data: read("spo_gtfs.zip") }] });
    want.push(await peabiru.isochrone(fresh, one, opts));
    fresh.free();
  }
  // more grids and modes than a network keeps, one origin on one worker,
  // then two calls at once
  for (let round = 0; round < 2; round++) {
    for (let k = 0; k < calls.length; k++) {
      const i = (k * 5 + round * 3) % calls.length;
      assert.deepEqual(await peabiru.isochrone(net, one, calls[i]), want[i], `inline, call ${i}`);
      assert.deepEqual(await peabiru.isochrone(pool, one, calls[i]), want[i], `pool, call ${i}`);
      const j = (i + 7) % calls.length;
      const both = await Promise.all([peabiru.isochrone(pool, one, calls[i]), peabiru.isochrone(pool, one, calls[j])]);
      assert.deepEqual(both, [want[i], want[j]], `pool, calls ${i} and ${j} at once`);
    }
  }
});

test("an origin far from the streets: empty polygons, no lines", async () => {
  const far = { id: ["far"], lon: [-46.0], lat: [-23.0] };
  const x = await peabiru.isochrone(net, far, { mode: "WALK", cutoffs: [0, 10] });
  assert.equal(x.features.length, 1);
  assert.deepEqual(x.features[0].geometry.coordinates, []);
  const l = await peabiru.isochrone(net, far, { mode: "WALK", cutoffs: [0, 10], polygonOutput: false });
  assert.equal(l.features.length, 0);
});

test("arguments are checked as r5r checks them", async () => {
  const one = take([0]);
  const iso = (opts) => peabiru.isochrone(net, one, { mode: "WALK", ...opts });
  const bad = [
    [{ cutoffs: 0 }, RangeError, /at least one value greater than 0/],
    [{ cutoffs: [-5, 10] }, RangeError, /cutoffs/],
    [{ cutoffs: [10, NaN] }, RangeError, /cutoffs/],
    [{ cutoffs: [10, Infinity] }, RangeError, /cutoffs/],
    [{ cutoffs: "10" }, TypeError, /cutoffs/],
    [{ zoom: 8 }, RangeError, /zoom/],
    [{ zoom: 13 }, RangeError, /zoom/],
    [{ zoom: 10.5 }, TypeError, /zoom/],
    [{ polygonOutput: null }, TypeError, /polygonOutput/],
    [{ polygonOutput: false, percentiles: 50 }, RangeError, /Percentiles not supported for line-based isochrones/],
    [{ percentiles: 100 }, RangeError, /percentiles/],
    [{ mode: ["BICYCLE", "TRANSIT"] }, peabiru.UnsupportedError, /not supported/],
    [{ mode: "TRANSIT" }, RangeError, /departureDatetime/],
    [{ newLts: {} }, TypeError, /unknown option 'newLts'/],
  ];
  for (const [opts, kind, msg] of bad) {
    await assert.rejects(() => iso(opts), (e) => e instanceof kind && msg.test(e.message), JSON.stringify(opts));
  }
  const warnings = [];
  await iso({ cutoffs: [0, 5], sampleSize: 0.5, onWarning: (w) => warnings.push(w) });
  assert.match(warnings[0], /sample_size/);
  // maxTripDuration is ignored
  assert.deepEqual(await iso({ cutoffs: [0, 5], maxTripDuration: 1 }), await iso({ cutoffs: [0, 5] }));
});
