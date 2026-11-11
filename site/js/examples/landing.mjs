// The JavaScript example of the home page (site/index.html) and of the
// README at the root of the repository; run_examples.mjs checks that both
// show this code.
import assert from "node:assert/strict";

// ANCHOR: landing
// Not on npm yet. From a clone of the repository (needs Rust and wasm-pack):
//   cd js && sh build.sh, then npm install /path/to/peabiru-routing/js
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";

// the sample network of the Python package (python/peabiru/data/spo)
const network = await peabiru.buildNetwork({
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
});
const points = {
  id: ["a", "b", "c"],
  lon: [-46.6290, -46.6321, -46.6355],
  lat: [-23.5563, -23.5577, -23.5601],
};

const ttm = await peabiru.travelTimeMatrix(network, points, points, {
  mode: ["WALK", "TRANSIT"],
  departureDatetime: "2019-05-13 14:00:00",
  percentiles: [25, 50, 75],
  seed: 42,
});
// ANCHOR_END: landing

console.log(Object.keys(ttm).join(" "));
for (let i = 0; i < ttm.from_id.length; i++) {
  console.log(ttm.from_id[i], ttm.to_id[i], ttm.travel_time_p25[i], ttm.travel_time_p50[i], ttm.travel_time_p75[i]);
}
network.free();

// checks, not shown in the documentation
assert.equal(ttm.from_id.length, 9);
