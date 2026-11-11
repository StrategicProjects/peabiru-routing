// findSnap(), streetNetworkBbox(), streetNetworkToSf(), transitNetworkToSf(),
// checkTransitAvailability() and getGtfsErrors(). Their values are compared
// with r5r's by the R package's parity tests (the core is the same); these
// check the JavaScript layer, inline and on workers.

import assert from "node:assert/strict";
import { test } from "node:test";
import * as peabiru from "../lib/index.js";
import { read, sampleNetwork, samplePoints, storedZip } from "./helpers.mjs";

test("findSnap has r5r's columns, and NaN where nothing snaps", async () => {
  const net = await sampleNetwork();
  const pts = samplePoints();
  const some = { id: [...pts.id.slice(0, 3), "far"], lon: [...pts.lon.slice(0, 3), -46.0], lat: [...pts.lat.slice(0, 3), -23.0] };
  const x = await peabiru.findSnap(net, some);
  assert.deepEqual(Object.keys(x), ["point_id", "lat", "lon", "snap_lat", "snap_lon", "distance", "found"]);
  assert.deepEqual(x.point_id, some.id);
  assert.deepEqual(x.found, [true, true, true, false]);
  assert.ok(Number.isNaN(x.snap_lat[3]) && Number.isNaN(x.distance[3]));
  assert.ok(Array.from(x.distance.slice(0, 3)).every((d) => d >= 0 && d < 300));
  const none = await peabiru.findSnap(net, pts, { radius: 0 });
  assert.ok(none.found.every((f) => !f));
  assert.ok((await peabiru.findSnap(net, pts, { radius: NaN })).found.every((f) => !f));
  await assert.rejects(() => peabiru.findSnap(net, pts, { radius: -1 }), RangeError);
  await assert.rejects(() => peabiru.findSnap(net, pts, { radius: Infinity }), /finite/);
  await assert.rejects(() => peabiru.findSnap(net, pts, { mode: "walk" }), /subset/);
  await assert.rejects(() => peabiru.findSnap(net, pts, { radios: 10 }), /unknown option/);
});

test("streetNetworkBbox gives a polygon, a bbox or a vector", async () => {
  const net = await sampleNetwork();
  const v = await peabiru.streetNetworkBbox(net, "vector");
  assert.deepEqual(Object.keys(v), ["xmin", "ymin", "xmax", "ymax"]);
  assert.deepEqual(await peabiru.streetNetworkBbox(net, "bbox"), [v.xmin, v.ymin, v.xmax, v.ymax]);
  const p = await peabiru.streetNetworkBbox(net);
  assert.equal(p.type, "Polygon");
  assert.deepEqual(p.coordinates[0][0], [v.xmin, v.ymin]);
  assert.deepEqual(p.coordinates[0][2], [v.xmax, v.ymax]);
  await assert.rejects(() => peabiru.streetNetworkBbox(net, "box"), RangeError);
});

test("streetNetworkToSf: every edge end is a vertex", async () => {
  const net = await sampleNetwork();
  const { vertices, edges } = await peabiru.streetNetworkToSf(net);
  const index = vertices.features.map((f) => f.properties.index);
  const ends = new Set(edges.features.flatMap((f) => [f.properties.from_vertex, f.properties.to_vertex]));
  assert.equal(new Set(index).size, index.length);
  assert.deepEqual([...index].sort((a, b) => a - b), [...ends].sort((a, b) => a - b));
  const at = new Map(vertices.features.map((f) => [f.properties.index, f.geometry.coordinates]));
  for (const e of edges.features.slice(0, 20)) {
    assert.deepEqual(e.geometry.coordinates[0], at.get(e.properties.from_vertex));
  }
  assert.ok(edges.features.every((f) => ["MOTORWAY", "PRIMARY", "SECONDARY", "TERTIARY", "OTHER"].includes(f.properties.street_class)));
});

test("transitNetworkToSf: stops and pattern routes", async () => {
  const net = await sampleNetwork();
  const { stops, routes } = await peabiru.transitNetworkToSf(net);
  assert.equal(stops.features.length, net.info().stops);
  for (const [i, f] of stops.features.entries()) {
    assert.equal(f.properties.stop_index, i);
    assert.ok(f.properties.stop_id.startsWith("spo_gtfs:"));
    assert.equal(f.geometry === null, !f.properties.linked_to_street);
  }
  assert.ok(routes.features.length > 0);
  assert.ok(routes.features.every((f) => f.properties.agency_name === "SPTRANS"));
});

test("checkTransitAvailability discovers dates and counts services", async () => {
  const net = await sampleNetwork();
  const d = await peabiru.checkTransitAvailability(net);
  assert.deepEqual(Object.keys(d), ["date", "total_services", "active_services", "pct_active"]);
  assert.deepEqual(d.date, [...d.date].sort());
  assert.ok(Array.from(d.active_services).every((a) => a > 0));
  const x = await peabiru.checkTransitAvailability(net, { dates: ["2019-05-13", new Date(2019, 4, 19)] });
  assert.deepEqual(x.date, ["2019-05-13", "2019-05-19"]);
  const monday = await net.transitAvailability("2019-05-13");
  assert.equal(x.total_services[0], monday.services);
  assert.equal(x.active_services[0], monday.active);
  const r = await peabiru.checkTransitAvailability(net, { startDate: "2019-05-01", endDate: "2019-05-31" });
  assert.equal(r.date.length, 31);
  assert.equal(r.active_services[12], x.active_services[0]);
  await assert.rejects(() => peabiru.checkTransitAvailability(net, { dates: "2019-05-13", startDate: "2019-05-01" }), /Incorrect date/);
  await assert.rejects(() => peabiru.checkTransitAvailability(net, { dates: "13/05/2019" }), /Invalid date format/);
  await assert.rejects(() => peabiru.checkTransitAvailability(net, { dates: "2019-02-30" }), /logically invalid/);
  await assert.rejects(() => peabiru.checkTransitAvailability(net, { startDate: "2019-05-31", endDate: "2019-05-01" }), /before or the same/);
});

test("getGtfsErrors lists gtfs-lib's errors of the feeds", async () => {
  const gtfs = [{ name: "spo_gtfs.zip", data: read("spo_gtfs.zip") }];
  const e = await peabiru.getGtfsErrors(gtfs);
  assert.deepEqual(Object.keys(e), ["file", "line", "type", "field", "id", "priority"]);
  assert.ok(e.type.length > 0 && e.type.every((t) => t === "DuplicateKeyError"));
  assert.deepEqual(await peabiru.getGtfsErrors({ gtfs }), e);
  assert.ok(e.line instanceof Int32Array);
  const net = await sampleNetwork();
  await assert.rejects(() => peabiru.getGtfsErrors(net), /does not keep/);
  await assert.rejects(() => peabiru.getGtfsErrors(42), TypeError);
});

test("buildNetwork refuses feeds with errors of priority HIGH, as r5r", async () => {
  const feed = {
    "stops.txt": "stop_id,stop_name,stop_lat,stop_lon\nA,A,-23.5563,-46.6290\nB,B,-23.5577,-46.6321\n",
    "routes.txt": "route_id,route_type\nR,3\n",
    "trips.txt": "route_id,service_id,trip_id\nR,S,T1\n",
    "stop_times.txt": "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nT1,14:00:00,14:00:00,A,1\nT1,14:05:00,14:05:00,B,2\n",
    "calendar.txt": "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nS,1,1,1,1,1,1,1,20190101,20191231\n",
  };
  // no agency.txt: a NoAgencyInFeedError
  const gtfs = [{ name: "spo_gtfs.zip", data: read("spo_gtfs.zip") }, { name: "bus.zip", data: storedZip(feed) }];
  const message = "High priority GTFS errors found; network build failed (bus.zip: 1 error). Use getGtfsErrors() on the same feeds to see them.";
  for (const nThreads of [1, 2]) {
    await assert.rejects(() => peabiru.buildNetwork({ pbf: read("spo_osm.pbf"), gtfs, nThreads }), { message });
  }
  const e = await peabiru.getGtfsErrors(gtfs);
  assert.deepEqual(e.type.filter((_, i) => e.priority[i] === "HIGH"), ["NoAgencyInFeedError"]);
  // errors of lower priority only: built
  feed["agency.txt"] = "agency_id,agency_name,agency_url,agency_timezone\nA,A,www.a,America/Sao_Paulo\n";
  const net = await peabiru.buildNetwork({ pbf: read("spo_osm.pbf"), gtfs: [{ name: "bus.zip", data: storedZip(feed) }] });
  assert.deepEqual(net.info().feeds, ["bus"]);
  net.free();
});

test("the same results on workers", async () => {
  const one = await sampleNetwork();
  const net = await peabiru.buildNetwork({
    pbf: read("spo_osm.pbf"),
    gtfs: [{ name: "spo_gtfs.zip", data: read("spo_gtfs.zip") }],
    nThreads: 2,
  });
  try {
    const pts = samplePoints();
    assert.deepEqual(await peabiru.findSnap(net, pts), await peabiru.findSnap(one, pts));
    assert.deepEqual(await peabiru.streetNetworkToSf(net), await peabiru.streetNetworkToSf(one));
    assert.deepEqual(await peabiru.transitNetworkToSf(net), await peabiru.transitNetworkToSf(one));
    assert.deepEqual(await peabiru.checkTransitAvailability(net), await peabiru.checkTransitAvailability(one));
    assert.deepEqual(await peabiru.streetNetworkBbox(net, "vector"), await peabiru.streetNetworkBbox(one, "vector"));
  } finally {
    net.free();
  }
});
