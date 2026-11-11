// A network with a pool of workers: same results as the calling thread,
// fewer of its workers for one call, and stopping a call with a signal.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

const input = {
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
};
const points = parsePoints(await readFile("spo_points.csv", "utf8"));
const options = { mode: ["WALK", "TRANSIT"], departureDatetime: "2019-05-13 14:00:00", seed: 42 };

// ANCHOR: pool
const network = await peabiru.buildNetwork({ ...input, nThreads: 4 });
console.log("workers:", network.workers);

const ttm = await peabiru.travelTimeMatrix(network, points, points, options);
// only two of the four workers for this call
const ttm2 = await peabiru.travelTimeMatrix(network, points, points, { ...options, nThreads: 2 });
// ANCHOR_END: pool

// ANCHOR: same
const inline = await peabiru.buildNetwork(input); // no workers
const reference = await peabiru.travelTimeMatrix(inline, points, points, options);
const equal = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
console.log("same as the calling thread:",
  equal(ttm.to_id, reference.to_id) && equal(ttm.travel_time_p50, reference.travel_time_p50) &&
  equal(ttm2.travel_time_p50, reference.travel_time_p50));
inline.free();
// ANCHOR_END: same

// ANCHOR: abort
const controller = new AbortController();
const call = peabiru.travelTimeMatrix(network, points, points, { ...options, signal: controller.signal });
controller.abort();
try {
  await call;
} catch (e) {
  console.log("stopped:", e.name);
}
// the network rebuilds the stopped workers and stays usable
const after = await peabiru.travelTimeMatrix(network, points, points, options);
console.log("rows after the abort:", after.from_id.length);
// ANCHOR_END: abort

// ANCHOR: one-worker
// one thread, but off the calling thread, so that a call can be stopped
const single = await peabiru.buildNetwork({ ...input, nThreads: 1, useWorkers: true });
console.log("workers:", single.workers);
single.free();
// ANCHOR_END: one-worker

network.free();

// checks, not shown in the documentation
assert.ok(equal(after.travel_time_p50, reference.travel_time_p50));
