// Isochrones by mode from one point, as GeoJSON: polygons, and the street
// segments reached.
import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

const network = await peabiru.buildNetwork({
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
});
const points = parsePoints(await readFile("spo_points.csv", "utf8"));
// ANCHOR: area
/** Area of a GeoJSON MultiPolygon in km², on a local flat approximation. */
function areaKm2(multiPolygon) {
  const ring = (r) => {
    const k = Math.cos((r[0][1] * Math.PI) / 180) * 111.32; // km per degree of longitude
    let a = 0;
    for (let i = 1; i < r.length; i++) {
      a += (r[i - 1][0] * k) * (r[i][1] * 110.57) - (r[i][0] * k) * (r[i - 1][1] * 110.57);
    }
    return Math.abs(a / 2);
  };
  // the first ring of each polygon is its shell, the others are holes
  return multiPolygon.coordinates
    .map(([shell, ...holes]) => ring(shell) - holes.reduce((s, h) => s + ring(h), 0))
    .reduce((s, a) => s + a, 0);
}
// ANCHOR_END: area

// ANCHOR: polygons
const origin = { id: [points.id[0]], lon: [points.lon[0]], lat: [points.lat[0]] };

const modes = { walk: "WALK", bicycle: "BICYCLE", car: "CAR", transit: ["WALK", "TRANSIT"] };
const features = [];
for (const [name, mode] of Object.entries(modes)) {
  const iso = await peabiru.isochrone(network, origin, {
    mode,
    cutoffs: [5, 10, 15],
    zoom: 12,
    departureDatetime: "2019-05-13 14:00:00",
    timeWindow: 30,
    seed: 42,
  });
  for (const f of iso.features) {
    f.properties.mode = name;
    features.push(f);
  }
  // one MultiPolygon per cutoff, largest first
  const areas = iso.features.map((f) => `${f.properties.isochrone} min ${areaKm2(f.geometry).toFixed(2)}`);
  console.log(name.padEnd(8), areas.reverse().join(" · "), "km²");
}
// one file for a web map, QGIS, geopandas or sf
await writeFile("isochrones.geojson", JSON.stringify({ type: "FeatureCollection", features }));
// ANCHOR_END: polygons

// ANCHOR: lines
const streets = await peabiru.isochrone(network, origin, {
  mode: "WALK",
  cutoffs: [5, 10, 15],
  polygonOutput: false,
});
// each segment is in the band of the smallest cutoff at or above its travel time
let metres = 0;
for (const cutoff of [5, 10, 15]) {
  for (const { properties: p } of streets.features) if (p.isochrone === cutoff) metres += p.length;
  console.log(`within ${cutoff} min on foot: ${(metres / 1000).toFixed(1)} km of street`);
}
// ANCHOR_END: lines

network.free();

// checks, not shown in the documentation
assert.equal(features.length, 4 * 3);
assert.ok(features.every((f) => f.geometry.type === "MultiPolygon"));
assert.ok(streets.features.every((f) => f.geometry.type === "LineString"));
