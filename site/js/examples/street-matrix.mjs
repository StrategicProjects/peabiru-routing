// Street travel-time matrices: walking, cycling and driving, on a network
// built from the OpenStreetMap extract alone.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

const points = parsePoints(await readFile("spo_points.csv", "utf8"));

// ANCHOR: street
const streets = await peabiru.buildNetwork({ pbf: await readFile("spo_osm.pbf") });

const walk = await peabiru.travelTimeMatrix(streets, points, points, {
  mode: "WALK",
  maxTripDuration: 15,
});
const bike = await peabiru.travelTimeMatrix(streets, points, points, {
  mode: "BICYCLE",
  bikeSpeed: 15,
  maxLts: 3,
  maxTripDuration: 15,
});
const car = await peabiru.travelTimeMatrix(streets, points, points, {
  mode: "CAR",
  maxTripDuration: 15,
});

// pairs that are not reached within maxTripDuration are left out
console.log(`walk: ${walk.from_id.length} pairs, bicycle: ${bike.from_id.length}, car: ${car.from_id.length}`);
// ANCHOR_END: street

// ANCHOR: lookup
// the travel time from one point to another, by car
function travelTime(m, from, to) {
  for (let i = 0; i < m.from_id.length; i++) {
    if (m.from_id[i] === from && m.to_id[i] === to) return m.travel_time_p50[i];
  }
  return undefined; // not reached
}
console.log("car:", travelTime(car, "89a8100c00fffff", "89a8100c03bffff"), "min");
// ANCHOR_END: lookup

streets.free();

// checks, not shown in the documentation
assert.ok(walk.from_id.length > 27 && walk.from_id.length < 27 * 27);
assert.ok(bike.from_id.length >= walk.from_id.length);
for (const m of [walk, bike, car]) {
  assert.deepEqual(Object.keys(m), ["from_id", "to_id", "travel_time_p50"]);
  assert.ok(m.travel_time_p50.every((t) => t >= 0 && t <= 15));
}
