// Getting started in Node: build the sample network from its files and
// compute a walk + public transport travel-time matrix.
import assert from "node:assert/strict";

// ANCHOR: all
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

// ANCHOR: build
const network = await peabiru.buildNetwork({
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
});
const points = parsePoints(await readFile("spo_points.csv", "utf8"));
// ANCHOR_END: build

// ANCHOR: matrix
const ttm = await peabiru.travelTimeMatrix(network, points, points, {
  mode: ["WALK", "TRANSIT"],
  departureDatetime: "2019-05-13 14:00:00",
  timeWindow: 10,
  percentiles: [25, 50, 75],
  seed: 42,
});

console.log(Object.keys(ttm).join(" "));
console.log(`${ttm.from_id.length} rows`);
for (let i = 0; i < 4; i++) {
  console.log(ttm.from_id[i], ttm.to_id[i], ttm.travel_time_p25[i], ttm.travel_time_p50[i], ttm.travel_time_p75[i]);
}
// ANCHOR_END: matrix

network.free();
// ANCHOR_END: all

// checks, not shown in the documentation
assert.equal(points.id.length, 27);
assert.equal(ttm.from_id.length, 27 * 27);
assert.ok(ttm.travel_time_p50 instanceof Int32Array);
for (let i = 0; i < ttm.from_id.length; i++) {
  assert.ok(ttm.travel_time_p25[i] <= ttm.travel_time_p50[i] && ttm.travel_time_p50[i] <= ttm.travel_time_p75[i]);
}
