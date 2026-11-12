// Scenarios: car speeds changed by way or by area, and levels of traffic
// stress for cyclists changed by way or along lines.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

const network = await peabiru.buildNetwork({ pbf: await readFile("spo_osm.pbf") });
const points = parsePoints(await readFile("spo_points.csv", "utf8"));
const departureDatetime = "2019-05-13 14:00:00";
/** The mean travel time between the sample's points, and the pairs reached. */
async function summary(options) {
  const ttm = await peabiru.travelTimeMatrix(network, points, points, { departureDatetime, ...options });
  const t = ttm.travel_time_p50;
  return `mean ${(t.reduce((s, v) => s + v, 0) / t.length).toFixed(1)} min, ${t.length} pairs reached`;
}

// ANCHOR: scale
console.log("car:              ", await summary({ mode: "CAR" }));
console.log("car, half speed:  ", await summary({ mode: "CAR", carspeedScale: 0.5 }));
// ANCHOR_END: scale

// ANCHOR: ways
// the OpenStreetMap ways of the primary streets, from the network itself
const { edges } = await peabiru.streetNetworkToSf(network);
const primary = [...new Set(edges.features.filter((f) => f.properties.street_class === "PRIMARY").map((f) => f.properties.osm_id))];
console.log(primary.length, "primary ways");
// close them to cars: 0 km/h
const closed = { osm_id: primary, max_speed: primary.map(() => 0), speed_type: primary.map(() => "km/h") };
console.log("car, primary closed:", await summary({ mode: "CAR", newCarspeeds: closed }));
// or slow them to 40% of their speed, and every other street to 90%
const slower = { osm_id: primary, max_speed: primary.map(() => 0.4), speed_type: primary.map(() => "scale") };
console.log("car, primary at 40%:", await summary({ mode: "CAR", newCarspeeds: slower, carspeedScale: 0.9 }));
// ANCHOR_END: ways

// ANCHOR: polygon
// a polygon in which cars go at 20% of their speed (GeoJSON, WGS 84)
const zone = {
  type: "FeatureCollection",
  features: [{
    type: "Feature",
    properties: { poly_id: "centre", scale: 0.2, priority: 1 },
    geometry: {
      type: "Polygon",
      coordinates: [[[-46.640, -23.560], [-46.625, -23.560], [-46.625, -23.548], [-46.640, -23.548], [-46.640, -23.560]]],
    },
  }],
};
console.log("car, slow zone:   ", await summary({ mode: "CAR", newCarspeeds: zone }));
// ANCHOR_END: polygon

// ANCHOR: lts
console.log("bicycle, maxLts 2:", await summary({ mode: "BICYCLE" }));
// the same primary ways made low-stress (level 1) for cyclists
const calm = { osm_id: primary, lts: primary.map(() => 1) };
console.log("bicycle, calm primary:", await summary({ mode: "BICYCLE", newLts: calm }));
// or along a line: the edges closest to it take its level
const lane = {
  type: "FeatureCollection",
  features: [{
    type: "Feature",
    properties: { line_id: "lane", lts: 1, priority: 1 },
    geometry: { type: "LineString", coordinates: [[-46.6560, -23.5630], [-46.6330, -23.5480]] },
  }],
};
console.log("bicycle, new lane:", await summary({ mode: "BICYCLE", newLts: lane }));
// ANCHOR_END: lts

network.free();

// checks, not shown in the documentation
assert.ok(primary.length > 0);
