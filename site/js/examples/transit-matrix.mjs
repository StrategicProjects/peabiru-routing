// Walk + public transport matrices: percentiles over the departure window,
// Monte Carlo draws with a seed, and missing values.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

const network = await peabiru.buildNetwork({
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
});
const points = parsePoints(await readFile("spo_points.csv", "utf8"));

// ANCHOR: transit
const options = {
  mode: ["WALK", "TRANSIT"],
  departureDatetime: "2019-05-13 14:00:00",
  timeWindow: 30,
  percentiles: [10, 50, 90],
  maxWalkTime: 20,
  maxTripDuration: 30,
  seed: 42,
};
const ttm = await peabiru.travelTimeMatrix(network, points, points, options);

// a percentile of the trips can be longer than maxTripDuration: NA there
let na = 0;
for (const t of ttm.travel_time_p90) if (t === peabiru.NA_INT32) na++;
console.log(`${ttm.from_id.length} pairs, ${na} of them NA at the 90th percentile`);
// ANCHOR_END: transit

// ANCHOR: seed
// the same seed gives the same results; without one, every call draws anew
const again = await peabiru.travelTimeMatrix(network, points, points, options);
const same = ttm.travel_time_p50.every((t, i) => t === again.travel_time_p50[i]);
console.log("same results with the same seed:", same);
// ANCHOR_END: seed

network.free();

// checks, not shown in the documentation
assert.ok(na > 0 && na < ttm.from_id.length);
assert.equal(same, true);
