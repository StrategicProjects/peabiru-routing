// Elevation: slope costs for walking and cycling from a GeoTIFF raster, in
// longitude and latitude or in another coordinate reference system, and the
// warning about voids. The rasters are synthetic hills over the sample
// (made for the tests of the package), not measured elevations.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

const pbf = await readFile("spo_osm.pbf");
const points = parsePoints(await readFile("spo_points.csv", "utf8"));
const two = { id: points.id.slice(0, 2), lon: points.lon.slice(0, 2), lat: points.lat.slice(0, 2) };
const far = { id: points.id.slice(-2), lon: points.lon.slice(-2), lat: points.lat.slice(-2) };

// ANCHOR: build
const tif = await readFile("spo_hills.tif"); // EPSG:4326, one band, metres
const flat = await peabiru.buildNetwork({ pbf });
const tobler = await peabiru.buildNetwork({ pbf, tif }); // elevation: "TOBLER" by default
const minetti = await peabiru.buildNetwork({ pbf, tif, elevation: "MINETTI" });
console.log(flat.info().elevation, tobler.info().elevation, minetti.info().elevation);
// ANCHOR_END: build

// ANCHOR: times
for (const mode of ["WALK", "BICYCLE"]) {
  const row = [];
  for (const network of [flat, tobler, minetti]) {
    const ttm = await peabiru.travelTimeMatrix(network, two, far, { mode, departureDatetime: "2019-05-13 14:00:00" });
    row.push(Array.from(ttm.travel_time_p50).join(" "));
  }
  console.log(`${mode.padEnd(7)} flat: ${row[0]} | TOBLER: ${row[1]} | MINETTI: ${row[2]}`);
}
// ANCHOR_END: times

// ANCHOR: crs
// the same hills in SIRGAS 2000 / UTM zone 23S (EPSG:31983)
const utm = await peabiru.buildNetwork({ pbf, tif: await readFile("spo_hills_31983.tif") });
const ttm = await peabiru.travelTimeMatrix(utm, two, far, { mode: "BICYCLE", departureDatetime: "2019-05-13 14:00:00" });
console.log("BICYCLE on the UTM raster:", Array.from(ttm.travel_time_p50).join(" "));
// ANCHOR_END: crs

// ANCHOR: voids
const warnings = [];
const voids = await peabiru.buildNetwork({
  pbf,
  tif: await readFile("spo_voids.tif"), // with SRTM-style voids of -32768
  onWarning: (message) => warnings.push(message),
});
console.log(warnings);
console.log("kept with the network:", voids.buildWarnings().length);
// ANCHOR_END: voids

for (const n of [flat, tobler, minetti, utm, voids]) n.free();

// checks, not shown in the documentation
assert.equal(warnings.length, 1);
assert.match(warnings[0], /3276\.7 m/);
