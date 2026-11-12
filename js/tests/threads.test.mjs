// The pool of workers (buildNetwork({ nThreads })) must give the results of
// the calling thread exactly: the same rows, in the same order, with the
// same values, whatever the number of workers a call uses.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test, after } from "node:test";
import { fileURLToPath } from "node:url";
import * as peabiru from "../lib/index.js";
import { DEPARTURE, read, sampleNetwork, samplePoints } from "./helpers.mjs";

const input = () => ({ pbf: read("spo_osm.pbf"), gtfs: [{ name: "spo_gtfs.zip", data: read("spo_gtfs.zip") }] });
const inline = await sampleNetwork();
const pool = await peabiru.buildNetwork({ ...input(), nThreads: 4 });
after(() => pool.free());

const pts = samplePoints();
const take = (idx) => {
  const out = {};
  for (const [k, v] of Object.entries(pts)) out[k] = idx.map((i) => v[i]);
  out.lon = Float64Array.from(out.lon);
  out.lat = Float64Array.from(out.lat);
  return out;
};
const one = take([5]);
const three = take([0, 7, 20]);
const transit = { mode: ["WALK", "TRANSIT"], departureDatetime: DEPARTURE, seed: 42 };

// [name, function, origins, destinations, options]
const CASES = [
  ["WALK ttm", peabiru.travelTimeMatrix, pts, pts, { mode: "WALK" }],
  ["WALK ttm, origins and destinations swapped", peabiru.travelTimeMatrix, pts, three, { mode: "WALK" }],
  ["BICYCLE ttm, two percentiles", peabiru.travelTimeMatrix, pts, pts, { mode: "BICYCLE", percentiles: [75, 10], maxLts: 3 }],
  ["CAR ttm, short trips", peabiru.travelTimeMatrix, pts, pts, { mode: "CAR", maxTripDuration: 4 }],
  ["transit ttm, five percentiles", peabiru.travelTimeMatrix, pts, pts, { ...transit, percentiles: [5, 25, 50, 75, 95] }],
  ["transit ttm, NA beyond the trip limit", peabiru.travelTimeMatrix, pts, pts, { ...transit, percentiles: [1, 99], maxTripDuration: 15, timeWindow: 30 }],
  ["transit ttm, one origin", peabiru.travelTimeMatrix, one, pts, transit],
  ["transit accessibility, step", peabiru.accessibility, pts, pts, { ...transit, opportunitiesColnames: ["jobs", "schools"], cutoffs: [15, 30], percentiles: [25, 50] }],
  ["transit accessibility, logistic", peabiru.accessibility, pts, pts, { ...transit, opportunitiesColnames: "jobs", decayFunction: "logistic", decayValue: 5, cutoffs: [20, 40] }],
  ["BICYCLE accessibility, linear", peabiru.accessibility, pts, pts, { mode: "BICYCLE", opportunitiesColnames: ["jobs", "healthcare"], decayFunction: "linear", decayValue: 10, cutoffs: [10, 20] }],
  ["WALK accessibility, fixed_exponential", peabiru.accessibility, pts, pts, { mode: "WALK", opportunitiesColnames: "jobs", decayFunction: "fixed_exponential", decayValue: 0.1 }],
  ["expanded, breakdown", peabiru.expandedTravelTimeMatrix, pts, pts, { ...transit, timeWindow: 3, breakdown: true }],
  ["expanded, short trips", peabiru.expandedTravelTimeMatrix, pts, pts, { mode: "TRANSIT", departureDatetime: "2019-05-13 07:30:30", timeWindow: 2, maxTripDuration: 12, drawsPerMinute: 3, seed: 7 }],
  ["arrival, breakdown", peabiru.arrivalTravelTimeMatrix, pts, pts, { mode: ["WALK", "TRANSIT"], arrivalDatetime: "2019-05-13 15:00:00", maxTripDuration: 30, breakdown: true, seed: 42 }],
  ["arrival across midnight", peabiru.arrivalTravelTimeMatrix, three, pts, { mode: "TRANSIT", arrivalDatetime: "2019-05-14 00:20:00", maxTripDuration: 40, seed: 3 }],
];

for (const [name, f, o, d, opts] of CASES) {
  test(`${name}: the same with 1, 2, 3, 4 and 100 workers`, async () => {
    const ref = await f(inline, o, d, opts);
    const rows = Object.values(ref)[0].length;
    assert.ok(rows > 0, "the case has rows");
    for (const nThreads of [1, 2, 3, 4, 100, null]) {
      assert.deepEqual(await f(pool, o, d, { ...opts, nThreads }), ref, `nThreads ${nThreads}`);
    }
  });
}

test("the network's workers and the clamp of nThreads", async () => {
  assert.equal(inline.workers, 0);
  assert.equal(pool.workers, 4);
  assert.deepEqual(pool.info(), inline.info());
  assert.deepEqual(pool.buildWarnings(), inline.buildWarnings());
  assert.deepEqual(await pool.transitAvailability("2019-05-13"), await inline.transitAvailability("2019-05-13"));
  // a network that routes inline takes any nThreads, as one
  const m = await peabiru.travelTimeMatrix(inline, pts, pts, { mode: "CAR", nThreads: 8 });
  assert.deepEqual(await peabiru.travelTimeMatrix(pool, pts, pts, { mode: "CAR", nThreads: Infinity }), m);
  await assert.rejects(peabiru.travelTimeMatrix(pool, pts, pts, { mode: "CAR", nThreads: 0 }), /'nThreads'/);
});

test("one worker, off the calling thread, and all the cores", async () => {
  const single = await peabiru.buildNetwork({ ...input(), useWorkers: true });
  assert.equal(single.workers, 1);
  const all = await peabiru.buildNetwork({ ...input(), nThreads: 0 });
  assert.ok(all.workers >= 1);
  const opts = { ...transit, percentiles: [10, 90] };
  const ref = await peabiru.travelTimeMatrix(inline, pts, pts, opts);
  assert.deepEqual(await peabiru.travelTimeMatrix(single, pts, pts, opts), ref);
  assert.deepEqual(await peabiru.travelTimeMatrix(all, pts, pts, opts), ref);
  single.free();
  all.free();
  await assert.rejects(peabiru.buildNetwork({ ...input(), nThreads: 2, useWorkers: false }), /'useWorkers'/);
  await assert.rejects(peabiru.buildNetwork({ ...input(), nThreads: 1.5 }), /'nThreads'/);
});

test("without a seed, one seed is drawn per call and every worker gets it", async () => {
  const random = Math.random;
  let k = 0;
  // a different value at every draw: a seed drawn per worker would differ
  Math.random = () => [0.125, 0.5, 0.875, 0.25][k++ % 4];
  try {
    const opts = { mode: "TRANSIT", departureDatetime: DEPARTURE, percentiles: [5, 95] };
    const pooled = await peabiru.travelTimeMatrix(pool, pts, pts, { ...opts, nThreads: 4 });
    assert.equal(k, 1, "one draw per call");
    const seed = (0.125 * 4294967296) | 0;
    assert.deepEqual(pooled, await peabiru.travelTimeMatrix(inline, pts, pts, { ...opts, seed }));
  } finally {
    Math.random = random;
  }
});

test("calls that run at the same time on one pool", async () => {
  const picks = [CASES[0], CASES[4], CASES[7], CASES[11], CASES[13]];
  const refs = [];
  for (const [, f, o, d, opts] of picks) refs.push(await f(inline, o, d, opts));
  const got = await Promise.all(picks.map(([, f, o, d, opts], i) => f(pool, o, d, { ...opts, nThreads: 1 + (i % 4) })));
  assert.deepEqual(got, refs);
});

// A GTFS feed of services that never run in 2019 (a stored zip; the reader
// does not check CRCs), to bring the services running below 20%.
function fillerFeed() {
  const files = {
    "agency.txt": "agency_id,agency_name,agency_url,agency_timezone\nA,A,http://a,America/Sao_Paulo\n",
    "stops.txt": "stop_id,stop_name,stop_lat,stop_lon\n",
    "routes.txt": "route_id,agency_id,route_short_name,route_type\n",
    "trips.txt": "route_id,service_id,trip_id\n",
    "stop_times.txt": "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n",
    "calendar.txt":
      "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\n" +
      Array.from({ length: 30 }, (_, i) => `S${i},1,1,1,1,1,1,1,20100101,20101231\n`).join(""),
  };
  const enc = new TextEncoder();
  const parts = [];
  const central = [];
  let offset = 0;
  const u16 = (v) => [v & 255, v >> 8];
  const u32 = (v) => [v & 255, (v >> 8) & 255, (v >> 16) & 255, v >>> 24];
  for (const [name, text] of Object.entries(files)) {
    const n = enc.encode(name);
    const data = enc.encode(text);
    const sizes = [...u32(data.length), ...u32(data.length)];
    const local = [...u32(0x04034b50), 20, 0, 0, 0, 0, 0, ...new Array(8).fill(0), ...sizes, ...u16(n.length), 0, 0];
    parts.push(Uint8Array.from(local), n, data);
    central.push(Uint8Array.from([...u32(0x02014b50), 20, 0, 20, 0, 0, 0, 0, 0, ...new Array(8).fill(0), ...sizes, ...u16(n.length), ...new Array(12).fill(0), ...u32(offset)]), n);
    offset += local.length + n.length + data.length;
  }
  const cdSize = central.reduce((s, a) => s + a.length, 0);
  const count = Object.keys(files).length;
  const end = Uint8Array.from([...u32(0x06054b50), 0, 0, 0, 0, ...u16(count), ...u16(count), ...u32(cdSize), ...u32(offset), 0, 0]);
  return Buffer.concat([...parts, ...central, end]);
}

test("the warning for few services running comes once, not once per worker", async () => {
  const gtfs = [...input().gtfs, { name: "filler.zip", data: fillerFeed() }];
  const warned = await peabiru.buildNetwork({ pbf: read("spo_osm.pbf"), gtfs, nThreads: 3 });
  try {
    const { services, active } = await warned.transitAvailability("2019-05-13");
    assert.ok(active > 0 && active / services < 0.2, `${active} of ${services}`);
    const warnings = [];
    const opts = { ...transit, nThreads: 3, onWarning: (m) => warnings.push(m) };
    await peabiru.travelTimeMatrix(warned, pts, pts, opts);
    await peabiru.accessibility(warned, pts, pts, { ...opts, opportunitiesColnames: "jobs", cutoffs: 30 });
    assert.deepEqual(warnings, Array(2).fill("Less than 20% of the transit services in the GTFS are running on the selected departure date."));
  } finally {
    warned.free();
  }
});

// A call long enough to be stopped half-way (seconds on one worker).
const SLOW = { mode: "TRANSIT", departureDatetime: DEPARTURE, timeWindow: 180, drawsPerMinute: 20, seed: 1 };

test("an aborted call rejects with an AbortError, and the network routes on", async () => {
  const ref = await peabiru.travelTimeMatrix(inline, pts, pts, { ...transit, percentiles: [25, 75] });
  for (const nThreads of [1, 4]) {
    const controller = new AbortController();
    const started = performance.now();
    const call = peabiru.travelTimeMatrix(pool, pts, pts, { ...SLOW, nThreads, signal: controller.signal });
    setTimeout(() => controller.abort(), 50);
    await assert.rejects(call, (e) => e.name === "AbortError");
    assert.ok(performance.now() - started < 1500, "the call stops at once");
    // the stopped workers are replaced; the next calls wait for them
    assert.deepEqual(await peabiru.travelTimeMatrix(pool, pts, pts, { ...transit, percentiles: [25, 75] }), ref);
  }
  // an abort reason is the rejection, as with fetch()
  const controller = new AbortController();
  const call = peabiru.expandedTravelTimeMatrix(pool, pts, pts, { ...SLOW, signal: controller.signal });
  setTimeout(() => controller.abort(new Error("enough")), 20);
  await assert.rejects(call, /^Error: enough$/);
  // a signal aborted before the call: nothing runs, inline or in workers
  for (const net of [inline, pool]) {
    await assert.rejects(peabiru.travelTimeMatrix(net, pts, pts, { mode: "CAR", signal: AbortSignal.abort() }), (e) => e.name === "AbortError");
  }
  await assert.rejects(peabiru.travelTimeMatrix(pool, pts, pts, { mode: "CAR", signal: "stop" }), /'signal'/);
  // other calls running on the pool are not affected by an abort
  const c2 = new AbortController();
  const [aborted, kept] = await Promise.allSettled([
    peabiru.travelTimeMatrix(pool, pts, pts, { ...SLOW, nThreads: 2, signal: c2.signal }),
    peabiru.accessibility(pool, pts, pts, { ...CASES[7][4], nThreads: 2 }),
    new Promise((r) => setTimeout(r, 30)).then(() => c2.abort()),
  ]);
  assert.equal(aborted.status, "rejected");
  assert.deepEqual(kept.value, await peabiru.accessibility(inline, pts, pts, CASES[7][4]));
});

test("errors from a worker keep their class and message", async () => {
  const bad = { mode: "TRANSIT", departureDatetime: DEPARTURE, timeWindow: 100000, drawsPerMinute: 1000 };
  const message = async (net) => {
    try {
      await peabiru.travelTimeMatrix(net, pts, pts, bad);
    } catch (e) {
      return [e.constructor, e.name, e.message];
    }
    assert.fail("no error");
  };
  const want = await message(inline);
  assert.match(want[2], /timeWindow x drawsPerMinute/);
  assert.deepEqual(await message(pool), want);
  // the checks on the calling side are the same with workers
  await assert.rejects(peabiru.travelTimeMatrix(pool, pts, pts, { mode: "WALK", outputDir: "out" }), peabiru.UnsupportedError);
  // building fails with the message of the calling thread
  for (const nThreads of [1, 3]) {
    await assert.rejects(peabiru.buildNetwork({ pbf: new Uint8Array([1, 2, 3]), nThreads }), /^Error: OpenStreetMap data: /);
    await assert.rejects(
      peabiru.buildNetwork({ pbf: read("spo_osm.pbf"), gtfs: [{ name: "feed.zip", data: new Uint8Array(10) }], nThreads }),
      /^Error: feed\.zip: /,
    );
  }
});

test("free() stops the workers and fails the calls still running", async () => {
  const net = await peabiru.buildNetwork({ ...input(), nThreads: 2 });
  const running = peabiru.travelTimeMatrix(net, pts, pts, SLOW);
  await new Promise((r) => setTimeout(r, 30));
  net.free();
  net.free();
  await assert.rejects(running, /freed/);
  await assert.rejects(peabiru.travelTimeMatrix(net, pts, pts), /freed/);
  assert.throws(() => net.info(), /freed/);
});

test("a Node program ends without freeing its networks", () => {
  const lib = fileURLToPath(new URL("../lib/index.js", import.meta.url));
  const helpers = fileURLToPath(new URL("./helpers.mjs", import.meta.url));
  const script = `
    const peabiru = await import(${JSON.stringify(lib)});
    const { read, samplePoints } = await import(${JSON.stringify(helpers)});
    const net = await peabiru.buildNetwork({ pbf: read("spo_osm.pbf"), nThreads: 3 });
    const pts = samplePoints();
    const m = await peabiru.travelTimeMatrix(net, pts, pts, { mode: "CAR" });
    // a call left running when the program has nothing else to do still ends
    peabiru.travelTimeMatrix(net, pts, pts, { mode: "BICYCLE" }).then((b) => console.log("done", m.from_id.length, b.from_id.length));
  `;
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;
  delete env.NODE_OPTIONS;
  const run = spawnSync(process.execPath, ["--input-type=module"], { input: script, env, encoding: "utf8", timeout: 30000 });
  assert.equal(run.signal, null, "the program did not hang");
  assert.equal(run.status, 0, run.stderr);
  assert.match(run.stdout, /^done 729 729\n$/);
});
