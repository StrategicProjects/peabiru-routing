// tripOptions(): the multi-criteria planner.
// The core checks the search against an exhaustive enumeration; these check
// the JavaScript side: the rows, the merge over the window, the legs, the
// same result through the pool of workers, and the checks.

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
const o = take([0, 1, 2, 3]);
const d = take([4, 5, 6, 7]);
const fares = await peabiru.setupFareStructure(net, 4.4);
fares.transfer_time_allowance = 60;
fares.fares_per_transfer.fare = fares.fares_per_transfer.fare.map(() => 6);

const OPTION_COLS = [
  "from_id", "to_id", "option", "total_time", "fare", "transfers", "walk_time", "departure_time",
  "arrival_time", "n_departures", "first_departure", "last_departure", "description",
];

test("options, every minute and the legs; the same with workers", async () => {
  const opts = { departureDatetime: DEPARTURE, timeWindow: 3, fareStructure: fares, byDeparture: true };
  const r = await peabiru.tripOptions(net, o, d, opts);
  assert.ok(r.options.length >= 4);
  for (const row of r.options) assert.deepEqual(Object.keys(row), OPTION_COLS);
  const n = new Map();
  for (const x of r.departures) {
    const k = `${x.from_id}>${x.to_id}>${x.option}`;
    n.set(k, (n.get(k) ?? 0) + 1);
  }
  for (const x of r.options) {
    assert.equal(n.get(`${x.from_id}>${x.to_id}>${x.option}`), x.n_departures);
    assert.ok(x.n_departures >= 1 && x.n_departures <= 3);
  }
  assert.equal(r.itineraries.type, "FeatureCollection");
  const legs = r.itineraries.features.map((f) => f.properties);
  // the last cumulative fare of each option is its fare
  const last = new Map();
  for (const l of legs) last.set(`${l.from_id}>${l.to_id}>${l.option}`, l.cumulative_fare);
  for (const x of r.options) assert.equal(last.get(`${x.from_id}>${x.to_id}>${x.option}`), x.fare);
  const again = await peabiru.tripOptions(pool, o, d, opts);
  assert.deepEqual(again, r);
  const rows = await peabiru.tripOptions(pool, o, d, { ...opts, dropGeometry: true });
  assert.deepEqual(rows.itineraries, legs);
});

test("criteria and limits", async () => {
  const call = (extra) => peabiru.tripOptions(net, o, d, { departureDatetime: DEPARTURE, timeWindow: 2, itineraries: false, ...extra });
  const a = await call({});
  assert.ok(!("fare" in a.options[0]));
  assert.equal(a.itineraries, null);
  assert.equal(a.departures, null);
  const fastest = (rows) => {
    const m = new Map();
    for (const x of rows) m.set(`${x.from_id}>${x.to_id}`, Math.min(m.get(`${x.from_id}>${x.to_id}`) ?? Infinity, x.total_time));
    return m;
  };
  assert.deepEqual(fastest((await call({ criteria: ["TIME"] })).options), fastest(a.options));
  assert.ok((await call({ maxTripDuration: 40 })).options.every((x) => x.total_time <= 40));
  assert.ok((await call({ fareStructure: fares, maxFare: 4.4 })).options.every((x) => x.fare <= 4.4));
  await assert.rejects(call({ criteria: ["FARE"] }), /needs a fareStructure/);
  await assert.rejects(call({ criteria: ["TIME", "SPEED"] }), /criteria/);
  await assert.rejects(call({ maxOptions: 0 }), /maxOptions/);
  assert.ok((await call({ walkResolution: 1 })).options.length >= a.options.length);
  await assert.rejects(call({ walkResolution: 0 }), /walkResolution/);
  await assert.rejects(call({ seed: 1 }), /unknown option 'seed'/);
});
