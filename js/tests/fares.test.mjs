import assert from "node:assert/strict";
import { test } from "node:test";
import * as peabiru from "../lib/index.js";
import { DEPARTURE, read, sampleNetwork, samplePoints } from "./helpers.mjs";

const { UnsupportedError } = peabiru;
const net = await sampleNetwork();
const pts = samplePoints();
const head = (k) => ({ id: pts.id.slice(0, k), lon: pts.lon.slice(0, k), lat: pts.lat.slice(0, k) });

test("setupFareStructure() builds r5r's object", async () => {
  const fs = await peabiru.setupFareStructure(net, 4.4);
  assert.deepEqual(Object.keys(fs), [
    "max_discounted_transfers", "transfer_time_allowance", "fare_cap", "fares_per_type",
    "fares_per_transfer", "fares_per_route", "debug_settings",
  ]);
  assert.equal(fs.max_discounted_transfers, 1);
  assert.equal(fs.transfer_time_allowance, 120);
  assert.equal(fs.fare_cap, Infinity);
  const types = fs.fares_per_type.type;
  assert.deepEqual([...types].sort(), [...new Set(fs.fares_per_route.fare_type)].sort());
  assert.ok(fs.fares_per_type.fare.every((f) => f === 4.4));
  assert.equal(fs.fares_per_transfer.first_leg.length, types.length ** 2);
  assert.deepEqual(fs.debug_settings, { output_file: "", trip_info: "MODE" });
  const g = await peabiru.setupFareStructure(net, 2, { by: "generic", debugPath: "x.csv" });
  assert.deepEqual(g.fares_per_type.type, ["GENERIC"]);
  assert.deepEqual(g.debug_settings, { output_file: "x.csv", trip_info: "ROUTE" });
  await assert.rejects(() => peabiru.setupFareStructure(net, -1), />= 0/);
  await assert.rejects(() => peabiru.setupFareStructure(net, 1, { by: "ROUTE" }), /subset/);
});

test("writeFareStructure() and readFareStructure() round-trip", async () => {
  const fs = await peabiru.setupFareStructure(net, 4.4, { by: "AGENCY_NAME" });
  fs.fares_per_route.route_long_name[0] = 'a, b "c"';
  fs.fare_cap = 7.5;
  const bytes = peabiru.writeFareStructure(fs);
  assert.ok(bytes instanceof Uint8Array);
  // deflated entries (method 8), as r5r's zip::zip() writes them
  assert.deepEqual([...bytes.slice(0, 4)], [0x50, 0x4b, 0x03, 0x04]);
  assert.equal(bytes[8] | (bytes[9] << 8), 8);
  assert.deepEqual(peabiru.readFareStructure(bytes), fs);
});

test("fare structures are checked as r5r checks them", async () => {
  const fs = await peabiru.setupFareStructure(net, 4.4);
  const ttm = (f, extra = {}) => peabiru.travelTimeMatrix(net, pts, pts, {
    mode: ["WALK", "TRANSIT"], departureDatetime: DEPARTURE, fareStructure: f, seed: 1, ...extra,
  });
  const missing = structuredClone(fs);
  for (const k of Object.keys(missing.fares_per_route)) missing.fares_per_route[k] = missing.fares_per_route[k].slice(1);
  await assert.rejects(() => ttm(missing), /no row for agency_id/);
  await assert.rejects(() => ttm({ type: "boston" }), UnsupportedError);
  const extra = structuredClone(fs);
  extra.fares_per_route.note = extra.fares_per_route.route_id.map(() => "x");
  await assert.rejects(() => ttm(extra), /cannot read/);
  await assert.rejects(() => ttm(fs, { timeWindow: 40 }), /200 Monte Carlo draws/);
});

test("fares limit trips; the Pareto frontier lists the times at each fare", async () => {
  const fs = await peabiru.setupFareStructure(net, 4.4);
  const run = (extra) => peabiru.travelTimeMatrix(net, pts, pts, {
    mode: ["WALK", "TRANSIT"], departureDatetime: DEPARTURE, seed: 42, drawsPerMinute: 1, fareStructure: fs, ...extra,
  });
  const free = await run({});
  const none = await run({ maxFare: 0 });
  const t0 = new Map(none.from_id.map((f, i) => [`${f} ${none.to_id[i]}`, none.travel_time_p50[i]]));
  free.from_id.forEach((f, i) => {
    const t = t0.get(`${f} ${free.to_id[i]}`);
    if (t !== undefined) assert.ok(free.travel_time_p50[i] <= t);
  });
  const pf = await peabiru.paretoFrontier(net, head(3), pts, {
    departureDatetime: DEPARTURE, fareStructure: fs, fareCutoffs: [0, 4.4, 8.8], seed: 42,
  });
  assert.deepEqual(Object.keys(pf), ["from_id", "to_id", "percentile", "travel_time", "monetary_cost"]);
  assert.ok(pf.from_id.length > 0);
  await assert.rejects(() => peabiru.paretoFrontier(net, pts, pts, { departureDatetime: DEPARTURE, fareCutoffs: [1] }), /required/);
  await assert.rejects(() => peabiru.paretoFrontier(net, pts, pts, { departureDatetime: DEPARTURE, fareStructure: fs, fareCutoffs: [1, 1] }), /duplicated/);
  await assert.rejects(() => peabiru.paretoFrontier(net, pts, pts, { departureDatetime: DEPARTURE, fareStructure: fs, fareCutoffs: [1], drawsPerMinute: 2 }), /unknown option/);
  // r5r's default mode is c("WALK", "TRANSIT"): public transport needs a departure time
  await assert.rejects(() => peabiru.paretoFrontier(net, head(1), pts, { fareStructure: fs, fareCutoffs: [1] }), /departureDatetime/);
  const walk = await peabiru.paretoFrontier(net, head(1), pts, { mode: "WALK", fareStructure: fs, fareCutoffs: [1], departureDatetime: DEPARTURE });
  const both = await peabiru.paretoFrontier(net, head(1), pts, { mode: ["WALK", "TRANSIT"], fareStructure: fs, fareCutoffs: [1], departureDatetime: DEPARTURE, seed: 1 });
  assert.deepEqual(await peabiru.paretoFrontier(net, head(1), pts, { fareStructure: fs, fareCutoffs: [1], departureDatetime: DEPARTURE, seed: 1 }), both);
  assert.ok(walk.from_id.length > 0);
});

test("the pool of workers gives the frontier of one instance", async () => {
  const fs = await peabiru.setupFareStructure(net, 4.4);
  const pool = await peabiru.buildNetwork({
    pbf: read("spo_osm.pbf"), gtfs: [{ name: "spo_gtfs.zip", data: read("spo_gtfs.zip") }], nThreads: 3,
  });
  const opts = { departureDatetime: DEPARTURE, fareStructure: fs, fareCutoffs: [0, 4.4, 8.8], seed: 42 };
  const a = await peabiru.paretoFrontier(net, head(7), pts, opts);
  const b = await peabiru.paretoFrontier(pool, head(7), pts, opts);
  assert.deepEqual(b, a);
  const ma = await peabiru.travelTimeMatrix(net, head(7), pts, { mode: ["WALK", "TRANSIT"], departureDatetime: DEPARTURE, seed: 3, fareStructure: fs, maxFare: 4.4 });
  const mb = await peabiru.travelTimeMatrix(pool, head(7), pts, { mode: ["WALK", "TRANSIT"], departureDatetime: DEPARTURE, seed: 3, fareStructure: fs, maxFare: 4.4 });
  assert.deepEqual(mb, ma);
  pool.free();
});

test("detailed itineraries with fares carry fare columns", async () => {
  const fs = await peabiru.setupFareStructure(net, 4.4);
  const di = await peabiru.detailedItineraries(net, head(3), { id: pts.id.slice(3, 6), lon: pts.lon.slice(3, 6), lat: pts.lat.slice(3, 6) }, {
    mode: "WALK", departureDatetime: DEPARTURE, fareStructure: fs, dropGeometry: true,
  });
  assert.ok(di.length > 0);
  assert.ok(di.every((r) => r.total_fare === 0 && r.cumulative_fare === 0));
  assert.deepEqual(Object.keys(di[0]).slice(9, 13), ["total_distance", "total_fare", "segment", "mode"]);
});
