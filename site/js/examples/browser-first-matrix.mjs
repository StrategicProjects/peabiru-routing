// Getting started in a browser: fetch the files, build the network on a
// pool of workers, and compute a matrix without blocking the page.

// ANCHOR: all
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

async function bytes(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  return response.arrayBuffer();
}

const network = await peabiru.buildNetwork({
  pbf: await bytes("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await bytes("spo_gtfs.zip") }],
  nThreads: 2, // two Web Workers; the page stays responsive
});
const points = parsePoints(await (await fetch("spo_points.csv")).text());

const ttm = await peabiru.travelTimeMatrix(network, points, points, {
  mode: ["WALK", "TRANSIT"],
  departureDatetime: "2019-05-13 14:00:00",
  percentiles: [25, 50, 75],
  seed: 42,
});
console.log(`${ttm.from_id.length} rows on ${network.workers} workers`);
for (let i = 0; i < 4; i++) {
  console.log(ttm.from_id[i], ttm.to_id[i], ttm.travel_time_p50[i]);
}
network.free();
// ANCHOR_END: all
