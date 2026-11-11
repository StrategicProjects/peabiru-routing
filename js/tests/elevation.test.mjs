// Elevation: networks built with a GeoTIFF raster. data/spo_hills.tif is
// synthetic (smooth hills between 743 and 834 m over the Sao Paulo sample,
// made with terra for these tests), not measured data.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import * as peabiru from "../lib/index.js";
import { read, sampleNetwork, samplePoints } from "./helpers.mjs";

const hills = readFileSync(new URL("./data/spo_hills.tif", import.meta.url));
const build = (extra) =>
  peabiru.buildNetwork({
    pbf: read("spo_osm.pbf"),
    gtfs: [{ name: "spo_gtfs.zip", data: read("spo_gtfs.zip") }],
    ...extra,
  });
const times = async (net, mode, opts = {}) => {
  const pts = samplePoints();
  return Array.from((await peabiru.travelTimeMatrix(net, pts, pts, { mode, ...opts })).travel_time_p50);
};

test("a raster gives walking and cycling slope costs", async () => {
  const flat = await sampleNetwork();
  assert.equal(flat.info().elevation, "NONE");
  const tobler = await build({ tif: hills });
  assert.equal(tobler.info().elevation, "TOBLER");
  const minetti = await build({ tif: hills, elevation: "minetti" });
  assert.equal(minetti.info().elevation, "MINETTI");
  const none = await build({ tif: hills, elevation: "NONE" });
  assert.equal(none.info().elevation, "NONE");
  for (const mode of ["WALK", "BICYCLE"]) {
    const t0 = await times(none, mode);
    assert.deepEqual(t0, await times(flat, mode));
    const t1 = await times(tobler, mode);
    assert.notDeepEqual(t0, t1);
    assert.notDeepEqual(t1, await times(minetti, mode));
  }
  assert.deepEqual(await times(tobler, "CAR"), await times(flat, "CAR"));
  // a scenario routes without the slope costs, as in r5r
  const warnings = [];
  const onWarning = (w) => warnings.push(w);
  assert.deepEqual(await times(tobler, "WALK", { carspeedScale: 0.9, onWarning }), await times(flat, "WALK"));
  assert.equal(warnings.length, 1);
  assert.match(warnings[0], /due to missing elevation\. See issue #555\./);
  await times(flat, "WALK", { carspeedScale: 0.9, onWarning });
  assert.equal(warnings.length, 1);
  for (const n of [tobler, minetti, none]) n.free();
});

test("workers build the same network with elevation", async () => {
  const one = await build({ tif: hills });
  const pool = await build({ tif: hills, nThreads: 2 });
  assert.equal(pool.info().elevation, "TOBLER");
  assert.deepEqual(await times(pool, "BICYCLE"), await times(one, "BICYCLE"));
  one.free();
  pool.free();
});

test("bad elevation input is an error", async () => {
  await assert.rejects(() => build({ tif: hills, elevation: "hills" }), /Invalid value for 'elevation'/);
  await assert.rejects(() => build({ tif: new Uint8Array(10) }), /elevation raster: not a TIFF file/);
  await assert.rejects(() => build({ tif: "x.tif" }), TypeError);
  // without a raster the model is "NONE"
  const net = await build({ elevation: "MINETTI" });
  assert.equal(net.info().elevation, "NONE");
  net.free();
});

test("rasters in other coordinate reference systems, and voids with a warning", async () => {
  const flat = await sampleNetwork();
  const utm = await build({ tif: readFileSync(new URL("./data/spo_hills_31983.tif", import.meta.url)) });
  assert.equal(utm.info().elevation, "TOBLER");
  assert.notDeepEqual(await times(utm, "WALK"), await times(flat, "WALK"));
  const warnings = [];
  const voids = await build({
    tif: readFileSync(new URL("./data/spo_voids.tif", import.meta.url)),
    onWarning: (w) => warnings.push(w),
  });
  assert.equal(warnings.length, 1);
  assert.match(warnings[0], /[0-9]+ with a value beyond \+\/-3276\.7 m/);
  assert.deepEqual(voids.r5rWarnings(), warnings);
  for (const n of [utm, voids]) n.free();
});
