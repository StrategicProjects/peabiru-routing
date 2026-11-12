// Inspecting a network: where points snap, the bounding box, the street
// and public transport networks as GeoJSON, and the dates with service.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

// ANCHOR: setup
const network = await peabiru.buildNetwork({
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
});
const points = parsePoints(await readFile("spo_points.csv", "utf8"));
// ANCHOR_END: setup

// ANCHOR: snap
const three = { id: points.id.slice(0, 3), lon: points.lon.slice(0, 3), lat: points.lat.slice(0, 3) };
const snap = await peabiru.findSnap(network, three, { mode: "CAR" }); // radius: 1600 m by default
for (let i = 0; i < snap.point_id.length; i++) {
  console.log(snap.point_id[i], snap.found[i], snap.snap_lon[i], snap.snap_lat[i], `${snap.distance[i].toFixed(1)} m`);
}
// ANCHOR_END: snap

// ANCHOR: bbox
console.log(await peabiru.streetNetworkBbox(network, "vector"));
console.log(await peabiru.streetNetworkBbox(network, "bbox")); // a GeoJSON bbox
const box = await peabiru.streetNetworkBbox(network); // a GeoJSON Polygon
console.log(box.type, box.coordinates[0].length, "points");
// ANCHOR_END: bbox

// ANCHOR: streets
const { vertices, edges } = await peabiru.streetNetworkToSf(network);
console.log(vertices.features.length, "vertices,", edges.features.length, "directed edges");
console.log(edges.features[0].properties);
// ANCHOR_END: streets

// ANCHOR: transit
const { stops, routes } = await peabiru.transitNetworkToSf(network);
console.log(stops.features.length, "stops,", routes.features.length, "routes (one per trip pattern)");
console.log(stops.features[0].properties);
console.log(routes.features[0].properties, routes.features[0].geometry.type);
// ANCHOR_END: transit

// ANCHOR: availability
const week = await peabiru.checkTransitAvailability(network, { startDate: "2019-05-12", endDate: "2019-05-18" });
for (let i = 0; i < week.date.length; i++) {
  console.log(week.date[i], `${week.active_services[i]} of ${week.total_services[i]} services`, week.pct_active[i]);
}
// with no dates: every date on which some trip runs
const all = await peabiru.checkTransitAvailability(network);
console.log(all.date.length, "dates, from", all.date[0], "to", all.date[all.date.length - 1]);
// ANCHOR_END: availability

network.free();

// checks, not shown in the documentation
assert.ok(snap.found.every(Boolean));
