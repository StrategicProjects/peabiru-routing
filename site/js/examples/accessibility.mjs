// Accessibility: opportunities reachable from each origin, cumulative or
// weighted by a decay function.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

const network = await peabiru.buildNetwork({
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
});
// ANCHOR: access
// id, lon, lat and the opportunity counts: population, schools, jobs, healthcare
const points = parsePoints(await readFile("spo_points.csv", "utf8"));

const access = await peabiru.accessibility(network, points, points, {
  opportunitiesColnames: ["jobs", "schools"],
  mode: ["WALK", "TRANSIT"],
  departureDatetime: "2019-05-13 14:00:00",
  decayFunction: "step",
  cutoffs: [15, 30],
  seed: 42,
});

// one row per origin, opportunity, percentile and cutoff
console.log(Object.keys(access).join(" "));
for (let i = 0; i < 4; i++) {
  console.log(access.id[i], access.opportunity[i], access.percentile[i], access.cutoff[i], access.accessibility[i]);
}
// ANCHOR_END: access

// ANCHOR: decay
const decayed = await peabiru.accessibility(network, points, points, {
  opportunitiesColnames: "jobs",
  mode: "WALK",
  decayFunction: "logistic",
  cutoffs: 20,
  decayValue: 5,
});
console.log("jobs within about 20 min on foot, first origin:", decayed.accessibility[0].toFixed(1));
// ANCHOR_END: decay

network.free();

// checks, not shown in the documentation
assert.equal(access.id.length, 27 * 2 * 2);
assert.ok(access.accessibility instanceof Float64Array);
assert.ok(access.percentile instanceof Int32Array && access.cutoff instanceof Int32Array);
assert.equal(decayed.id.length, 27);
