// Expanded and arrival matrices: trip by trip, minute by minute.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

const network = await peabiru.buildNetwork({
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
});
const points = parsePoints(await readFile("spo_points.csv", "utf8"));

// ANCHOR: expanded
// points can also be an array of {id, lon, lat} objects
const origin = [{ id: points.id[0], lon: points.lon[0], lat: points.lat[0] }];

const expanded = await peabiru.expandedTravelTimeMatrix(network, origin, points, {
  mode: ["WALK", "TRANSIT"],
  departureDatetime: "2019-05-13 14:00:00",
  timeWindow: 5,
  breakdown: true,
  seed: 42,
});
console.log(Object.keys(expanded).join(" "));
// the first three trips that ride a bus
const rides = (m) => [...m.routes.keys()].filter((i) => m.routes[i] !== "[WALK]").slice(0, 3);
for (const i of rides(expanded)) {
  const e = expanded;
  console.log(e.to_id[i], e.departure_time[i], e.draw_number[i], e.routes[i],
    `access ${e.access_time[i]} wait ${e.wait_time[i]} ride ${e.ride_time[i]}`,
    `transfer ${e.transfer_time[i]} egress ${e.egress_time[i]} total ${e.total_time[i]}`);
}
// ANCHOR_END: expanded

// ANCHOR: arrival
// the latest trip from each origin that arrives by 15:00
const arrival = await peabiru.arrivalTravelTimeMatrix(network, origin, points, {
  mode: ["WALK", "TRANSIT"],
  arrivalDatetime: "2019-05-13 15:00:00",
  seed: 42,
});
for (const i of rides(arrival)) {
  console.log(arrival.to_id[i], arrival.departure_time[i], arrival.routes[i], arrival.total_time[i]);
}
// ANCHOR_END: arrival

network.free();

// checks, not shown in the documentation
assert.ok(expanded.access_time instanceof Float64Array && expanded.n_rides instanceof Int32Array);
assert.equal(arrival.access_time, undefined);
assert.ok(expanded.from_id.every((id) => id === points.id[0]));
