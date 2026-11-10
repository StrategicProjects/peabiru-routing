import assert from "node:assert/strict";
import { test } from "node:test";
import * as peabiru from "../lib/index.js";
import { read, sampleNetwork, samplePoints } from "./helpers.mjs";

test("the module loads and gives its versions", async () => {
  await peabiru.init();
  assert.equal(peabiru.coreVersion(), peabiru.version);
  assert.equal(peabiru.NA_INT32, -(2 ** 31));
});

test("the sample network has its streets and its feed", async () => {
  const net = await sampleNetwork();
  const info = net.info();
  assert.deepEqual(info.feeds, ["spo_gtfs"]);
  assert.ok(info.vertices > 1000 && info.edges > info.vertices);
  assert.ok(info.stops > 0 && info.patterns > 0 && info.services > 0);
  assert.equal(info.frequencies, true);
  assert.deepEqual(net.r5rWarnings(), []);
  const monday = await net.transitAvailability("2019-05-13");
  assert.equal(monday.services, info.services);
  assert.ok(monday.active > 0 && monday.active <= monday.services);
  assert.deepEqual(await net.transitAvailability(new Date(2019, 4, 13, 9, 30)), monday);
  await assert.rejects(() => net.transitAvailability("2019-02-30"), TypeError);
});

test("feeds load in the order given, named by their file names", async () => {
  const gtfs = read("spo_gtfs.zip");
  const net = await peabiru.buildNetwork({
    pbf: read("spo_osm.pbf"),
    gtfs: [
      { name: "b.zip", data: gtfs },
      { name: "a.zip", data: gtfs.buffer.slice(gtfs.byteOffset, gtfs.byteOffset + gtfs.byteLength) },
    ],
  });
  assert.deepEqual(net.info().feeds, ["b", "a"]);
  net.free();
});

test("a network without feeds routes on the streets only", async () => {
  const net = await peabiru.buildNetwork({ pbf: read("spo_osm.pbf") });
  assert.deepEqual(net.info().feeds, []);
  assert.deepEqual(await net.transitAvailability("2019-05-13"), { services: 0, active: 0 });
  const pts = samplePoints();
  const m = await peabiru.travelTimeMatrix(net, pts, pts, { mode: "CAR" });
  assert.ok(m.from_id.length > 0);
  await assert.rejects(() => peabiru.travelTimeMatrix(net, pts, pts, { mode: "TRANSIT", departureDatetime: "2019-05-13 14:00" }),
    /no transit services available/,
  );
  net.free();
});

test("bad input data is an error that names it", async () => {
  await assert.rejects(peabiru.buildNetwork({ pbf: new Uint8Array([1, 2, 3]) }), /^Error: OpenStreetMap data: /);
  await assert.rejects(
    peabiru.buildNetwork({ pbf: read("spo_osm.pbf"), gtfs: [{ name: "feed.zip", data: new Uint8Array(10) }] }),
    /^Error: feed\.zip: /,
  );
  await assert.rejects(peabiru.buildNetwork({ pbf: "spo_osm.pbf" }), TypeError);
  await assert.rejects(peabiru.buildNetwork({ pbf: read("spo_osm.pbf"), elevation: "NONE" }), /unknown option 'elevation'/);
  const z = read("spo_gtfs.zip");
  await assert.rejects(
    peabiru.buildNetwork({ pbf: read("spo_osm.pbf"), gtfs: [{ name: "x.zip", data: z }, { name: "x.zip", data: z }] }),
    /Feed names must be distinct/,
  );
});

test("a freed network cannot be used", async () => {
  const net = await peabiru.buildNetwork({ pbf: read("spo_osm.pbf") });
  net.free();
  net.free();
  assert.throws(() => net.info(), /freed/);
  const pts = samplePoints();
  await assert.rejects(() => peabiru.travelTimeMatrix(net, pts, pts), /freed/);
});
