// Fares: a fare structure, a matrix and a Pareto frontier with it, and a
// fare structure file written and read back.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

const network = await peabiru.buildNetwork({
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
});
const points = parsePoints(await readFile("spo_points.csv", "utf8"));

// ANCHOR: structure
// every route, fare type and transfer costs 4.40; then two rides together 6.60
const fares = await peabiru.setupFareStructure(network, 4.4);
fares.fares_per_transfer.fare = fares.fares_per_transfer.fare.map(() => 6.6);
console.log("fare types:", fares.fares_per_type.type.join(", "));
console.log("routes:", fares.fares_per_route.route_id.length);
// ANCHOR_END: structure

// ANCHOR: routing
const options = {
  mode: ["WALK", "TRANSIT"],
  departureDatetime: "2019-05-13 14:00:00",
  seed: 42,
  fareStructure: fares,
};
const ttm = await peabiru.travelTimeMatrix(network, points, points, { ...options, drawsPerMinute: 1, maxFare: 6.6 });
console.log(`${ttm.from_id.length} pairs within 6.60`);
const frontier = await peabiru.paretoFrontier(network, points, points, { ...options, fareCutoffs: [0, 4.4, 6.6] });
console.log(`${frontier.from_id.length} rows; cutoffs as R5 reads them:`, [...new Set(frontier.monetary_cost)].join(", "));
// ANCHOR_END: routing

// ANCHOR: files
// the bytes of a .zip file, as the R and Python packages write it
const bytes = peabiru.writeFareStructure(fares);
const back = peabiru.readFareStructure(bytes);
console.log("read back the same:", JSON.stringify(back) === JSON.stringify(fares));
// ANCHOR_END: files

network.free();

// checks, not shown in the documentation
assert.ok(ttm.from_id.length > 0 && frontier.from_id.length > 0);
assert.equal(JSON.stringify(back), JSON.stringify(fares));
