// Scenarios: newCarspeeds, carspeedScale and newLts (parity with r5r is in
// bench/compare/js_parity.mjs).

import assert from "node:assert/strict";
import { test } from "node:test";
import * as peabiru from "../lib/index.js";
import { sampleNetwork, samplePoints } from "./helpers.mjs";

const ttm = (net, pts, opts) => peabiru.travelTimeMatrix(net, pts, pts, { maxTripDuration: 60, ...opts });

function pairs(m) {
  const out = new Map();
  m.from_id.forEach((f, i) => out.set(`${f} ${m.to_id[i]}`, m.travel_time_p50[i]));
  return out;
}

test("car speed and LTS scenarios change the times as asked", async () => {
  const net = await sampleNetwork();
  const pts = samplePoints();
  const ways = [...new Set((await peabiru.streetNetworkToSf(net)).edges.features.map((f) => f.properties.osm_id))];
  const base = await ttm(net, pts, { mode: "CAR" });
  assert.deepEqual(await ttm(net, pts, { mode: "CAR", carspeedScale: 1 }), base);
  const slow = await ttm(net, pts, { mode: "CAR", carspeedScale: 0.5 });
  const b = pairs(base);
  let slower = 0;
  for (const [k, v] of pairs(slow)) {
    assert.ok(v >= b.get(k), k);
    if (v > b.get(k)) slower++;
  }
  assert.ok(slower > 0);
  const every = { osm_id: ways, max_speed: ways.map(() => 0.5), speed_type: ways.map(() => "scale") };
  assert.deepEqual(await ttm(net, pts, { mode: "CAR", newCarspeeds: every, carspeedScale: 0.5 }), slow);
  // as rows, too
  const rows = ways.map((w) => ({ osm_id: w, max_speed: 0.5, speed_type: "scale" }));
  assert.deepEqual(await ttm(net, pts, { mode: "CAR", newCarspeeds: rows, carspeedScale: 0.5 }), slow);
  const stressful = { osm_id: ways, lts: ways.map(() => 4) };
  assert.deepEqual(
    await ttm(net, pts, { mode: "BICYCLE", newLts: stressful, maxLts: 4 }),
    await ttm(net, pts, { mode: "BICYCLE", maxLts: 4 }),
  );
  assert.notDeepEqual(await ttm(net, pts, { mode: "BICYCLE", newLts: stressful }), await ttm(net, pts, { mode: "BICYCLE" }));
});

test("GeoJSON scenarios, and r5r's checks", async () => {
  const net = await sampleNetwork();
  const pts = samplePoints();
  const [x0, x1] = [Math.min(...pts.lon), Math.max(...pts.lon)];
  const [y0, y1] = [Math.min(...pts.lat), Math.max(...pts.lat)];
  const poly = {
    type: "FeatureCollection",
    features: [{
      type: "Feature",
      properties: { poly_id: "a", scale: 0.5, priority: 1 },
      geometry: { type: "Polygon", coordinates: [[[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]]] },
    }],
  };
  assert.deepEqual(
    await ttm(net, pts, { mode: "CAR", newCarspeeds: poly, carspeedScale: 0.5 }),
    await ttm(net, pts, { mode: "CAR", carspeedScale: 0.5 }),
  );
  const line = {
    type: "FeatureCollection",
    features: [{ type: "Feature", properties: { line_id: "a", lts: 9, priority: 1 }, geometry: { type: "LineString", coordinates: [[x0, y0], [x1, y1]] } }],
  };
  await assert.rejects(() => ttm(net, pts, { mode: "BICYCLE", newLts: line }), /Element 1 is not <= 4/);
  await assert.rejects(() => ttm(net, pts, { mode: "CAR", carspeedScale: -1 }), /Element 1 is not >= 0/);
  // r5r's check lets NaN through, then fails with an internal R error
  await assert.rejects(() => ttm(net, pts, { mode: "CAR", carspeedScale: NaN }), /must be a number that is not NaN/);
  await assert.rejects(() => ttm(net, pts, { mode: "CAR", carspeedScale: "1" }), /Must be of type 'numeric', not 'string'/);
  await assert.rejects(() => ttm(net, pts, { mode: "CAR", carspeedScale: 1e300 }), /32-bit float/);
  await assert.rejects(() => ttm(net, pts, { mode: "CAR", newCarspeeds: { osm_id: [1], max_speed: [1] } }), /missing elements \{'speed_type'\}/);
  await assert.rejects(() => ttm(net, pts, { mode: "CAR", newLts: { osm_id: [1], lts: [1.5] } }), /Must be of type 'integer'/);
  await assert.rejects(() => ttm(net, pts, { mode: "CAR", carspeedScale: 1e9 }), /speed of 0 or less/);
});
