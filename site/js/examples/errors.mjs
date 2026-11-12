// Warnings and errors: what r5r would warn about, unknown options, and
// options that peabiru does not support.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";

const network = await peabiru.buildNetwork({ pbf: await readFile("spo_osm.pbf") });

// ANCHOR: warnings
const warnings = [];
const points = { id: [1, 2], lon: [-46.6290, -46.6321], lat: [-23.5563, -23.5577] };
await peabiru.travelTimeMatrix(network, points, points, {
  mode: "WALK",
  onWarning: (message) => warnings.push(message), // default: console.warn
});
console.log(warnings);
// ANCHOR_END: warnings

const ids = { id: ["a", "b"], lon: points.lon, lat: points.lat };
// ANCHOR: errors
for (const options of [
  { mode: "WALK", maxTripDurations: 30 }, // a typo
  { mode: "WALK", outputDir: "results" }, // writing results to files
]) {
  try {
    await peabiru.travelTimeMatrix(network, ids, ids, options);
  } catch (e) {
    console.log(`${e.name}: ${e.message}`);
    if (e instanceof peabiru.UnsupportedError) console.log("  (an UnsupportedError)");
  }
}
// ANCHOR_END: errors

network.free();

// checks, not shown in the documentation
assert.equal(warnings.length > 0, true);
