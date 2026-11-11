// detailedItineraries(): r5r's rows as GeoJSON features or plain rows, the
// same through the pool of workers as on the calling thread, and r5r's
// checks.

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
const o = take([0, 1, 2, 3, 4, 5, 6]);
const d = take([7, 8, 9, 10, 11, 12, 13]);
const COLS = [
  "from_id", "from_lat", "from_lon", "to_id", "to_lat", "to_lon", "option", "departure_time",
  "total_duration", "total_distance", "segment", "mode", "segment_duration", "wait", "distance", "route",
];

test("features with r5r's properties, the same with workers", async () => {
  const fc = await peabiru.detailedItineraries(net, o, d, { mode: "WALK", departureDatetime: DEPARTURE });
  assert.equal(fc.type, "FeatureCollection");
  assert.ok(fc.features.length > 0);
  for (const f of fc.features) {
    assert.deepEqual(Object.keys(f.properties), COLS);
    assert.equal(f.geometry.type, "LineString");
    assert.ok(f.geometry.coordinates.length >= 2);
    assert.equal(f.properties.departure_time, "14:00:00");
  }
  const again = await peabiru.detailedItineraries(pool, o, d, { mode: "WALK", departureDatetime: DEPARTURE });
  assert.deepEqual(again, fc);
  const rows = await peabiru.detailedItineraries(pool, o, d, { mode: "WALK", departureDatetime: DEPARTURE, dropGeometry: true });
  assert.deepEqual(rows, fc.features.map((f) => f.properties));
});

test("pairs, allToAll and the checks", async () => {
  const a = await peabiru.detailedItineraries(net, take([0, 1]), take([2, 3, 4]), {
    mode: "CAR", departureDatetime: DEPARTURE, allToAll: true, dropGeometry: true,
  });
  const pairs = [...new Set(a.map((r) => `${r.from_id}>${r.to_id}`))];
  assert.equal(pairs.length, 6);
  const warnings = [];
  await peabiru.detailedItineraries(net, take([0]), take([2, 3]), {
    mode: "CAR", departureDatetime: DEPARTURE, dropGeometry: true, onWarning: (w) => warnings.push(w),
  });
  assert.ok(warnings.some((w) => w.includes("'origins' was expanded")));
  await assert.rejects(peabiru.detailedItineraries(net, take([0, 1]), take([2, 3, 4]), { mode: "CAR", departureDatetime: DEPARTURE }), /same number of rows/);
  await assert.rejects(peabiru.detailedItineraries(net, o, d, { departureDatetime: DEPARTURE, osmLinkIds: true }), peabiru.UnsupportedError);
  await assert.rejects(peabiru.detailedItineraries(net, o, d, { departureDatetime: DEPARTURE, suboptimalMinutes: 3 }), /Must be 0/);
  // the sample's GTFS is frequency-based, which r5r refuses here
  await assert.rejects(peabiru.detailedItineraries(net, o, d, { mode: ["WALK", "TRANSIT"], departureDatetime: DEPARTURE }), /frequencies/);
});

test("scenarios slow the car, the same with workers", async () => {
  const opts = { mode: "CAR", departureDatetime: DEPARTURE, dropGeometry: true };
  const base = await peabiru.detailedItineraries(net, o, d, opts);
  assert.deepEqual(await peabiru.detailedItineraries(net, o, d, { ...opts, carspeedScale: 1 }), base);
  const slow = await peabiru.detailedItineraries(net, o, d, { ...opts, carspeedScale: 0.5 });
  assert.equal(slow.length, base.length);
  assert.ok(slow.every((r, i) => r.total_duration >= base[i].total_duration));
  assert.ok(slow.some((r, i) => r.total_duration > base[i].total_duration));
  assert.deepEqual(await peabiru.detailedItineraries(pool, o, d, { ...opts, carspeedScale: 0.5 }), slow);
});
