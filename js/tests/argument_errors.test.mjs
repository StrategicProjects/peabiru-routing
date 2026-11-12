// Argument errors worded as r5r's (and the R package's): every case of
// bench/compare/argument_errors.json, R's message turned into JavaScript's
// terms by the file's "js" rules and with the arguments in camelCase,
// unless the case is listed there as worded differently on purpose. The
// file is written by bench/compare/argument_errors.R from the R package,
// whose messages bench/compare/error_parity.R holds equal to r5r's.

import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import * as peabiru from "../lib/index.js";
import { sampleNetwork, samplePoints } from "./helpers.mjs";

const CASES = new URL("../../bench/compare/argument_errors.json", import.meta.url);

const VALUES = {
  NULL_: null, chr: "a", chr2: ["a", "b"], neg: -1, zero: 0, frac: 1.5, big: 1e10,
  inf: Infinity, nan: NaN, lgl: true, num2: [1, 2], empty: [],
};
const camel = (s) => s.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());

function first3(pts) {
  const out = {};
  for (const [k, v] of Object.entries(pts)) out[k] = v.slice(0, 3);
  return out;
}

/** The function's call: its positional arguments, then the options. */
function calls(network, fs) {
  const pts = first3(samplePoints());
  const one = { id: pts.id.slice(0, 1), lon: pts.lon.slice(0, 1), lat: pts.lat.slice(0, 1) };
  const d = new Date(2019, 4, 13, 14, 0);
  const transit = { mode: ["WALK", "TRANSIT"], departureDatetime: d, maxTripDuration: 30 };
  return {
    travel_time_matrix: { fn: "travelTimeMatrix", args: [pts, pts], opt: transit },
    expanded_travel_time_matrix: { fn: "expandedTravelTimeMatrix", args: [pts, pts], opt: transit },
    arrival_travel_time_matrix: {
      fn: "arrivalTravelTimeMatrix", args: [pts, pts],
      opt: { mode: ["WALK", "TRANSIT"], arrivalDatetime: d, maxTripDuration: 30 },
    },
    accessibility: {
      fn: "accessibility", args: [pts, pts],
      opt: { opportunitiesColnames: "jobs", mode: "WALK", departureDatetime: d, cutoffs: 20, maxTripDuration: 30 },
    },
    detailed_itineraries: {
      fn: "detailedItineraries", args: [pts, pts],
      opt: { mode: "WALK", departureDatetime: d, maxTripDuration: 30 },
    },
    isochrone: { fn: "isochrone", args: [one], opt: { mode: "WALK", cutoffs: 10, departureDatetime: d } },
    find_snap: { fn: "findSnap", args: [pts], opt: {} },
    check_transit_availability: { fn: "checkTransitAvailability", args: [], opt: { dates: "2019-05-13" } },
    setup_fare_structure: { fn: "setupFareStructure", args: [4.8], opt: {}, positional: { base_fare: 0 } },
    pareto_frontier: {
      fn: "paretoFrontier", args: [pts, pts],
      opt: { ...transit, fareStructure: fs, fareCutoffs: [0, 4.8, 9.6] },
    },
  };
}

function jsText(spec, msg, argNames) {
  for (const [pattern, repl] of spec.rules.js) msg = msg.replace(new RegExp(pattern, "g"), repl);
  // the arguments in camelCase
  return msg.replace(/[a-z]+(?:_[a-z0-9]+)+/g, (w) => (argNames.has(w) ? camel(w) : w));
}

test("argument errors are worded as r5r's", { skip: !existsSync(CASES) }, async () => {
  const spec = JSON.parse(readFileSync(CASES, "utf8"));
  const network = await sampleNetwork();
  const fs = await peabiru.setupFareStructure(network, 4.4);
  const call = calls(network, fs);
  const argNames = new Set(spec.cases.map((c) => c.arg));
  for (const n of ["max_trip_duration", "departure_datetime", "fare_structure", "max_fare",
    "setup_fare_structure", "travel_time_matrix", "decay_function", "decay_value", "start_date",
    "end_date", "mode_egress", "draws_per_minute", "time_window", "debug_path", "debug_info"]) argNames.add(n);
  const deliberate = Object.keys(spec.deliberate.js).map((k) => new RegExp(`^(?:${k})$`));
  const wrong = [];
  for (const c of spec.cases) {
    const key = `${c.fun}:${c.arg}:${c.value}`;
    if (deliberate.some((r) => r.test(key))) continue;
    const { fn, args, opt, positional } = call[c.fun];
    const a = [...args];
    const o = { ...opt };
    if (positional && c.arg in positional) a[positional[c.arg]] = VALUES[c.value];
    else o[camel(c.arg)] = VALUES[c.value];
    const expected = jsText(spec, c.message, argNames);
    let got = "(no error)";
    try {
      const quiet = fn === "setupFareStructure" || fn === "checkTransitAvailability" || fn === "findSnap" ? {} : { onWarning: () => {} };
      await peabiru[fn](network, ...a, { ...o, ...quiet });
    } catch (e) {
      got = e.message;
    }
    if (got !== expected) wrong.push(`${key}\n  expected: ${expected}\n  got:      ${got}`);
  }
  if (process.env.PEABIRU_ARGUMENT_ERRORS_OUT) writeFileSync(process.env.PEABIRU_ARGUMENT_ERRORS_OUT, wrong.join("\n"));
  assert.equal(wrong.length, 0, `${wrong.length} of ${spec.cases.length} differ:\n${wrong.slice(0, 40).join("\n")}`);
});

test("the deliberate cases still exist", { skip: !existsSync(CASES) }, () => {
  const spec = JSON.parse(readFileSync(CASES, "utf8"));
  const keys = spec.cases.map((c) => `${c.fun}:${c.arg}:${c.value}`);
  for (const k of Object.keys(spec.deliberate.js)) {
    const r = new RegExp(`^(?:${k})$`);
    assert.ok(keys.some((key) => r.test(key)), k);
  }
});

test("the checks of the R package", async () => {
  const network = await sampleNetwork();
  const pts = first3(samplePoints());
  const d = new Date(2019, 4, 13, 14, 0);
  const missing = { ...pts, id: [pts.id[0], null, pts.id[2]] };
  const one = {};
  for (const [k, v] of Object.entries(pts)) one[k] = v.slice(0, 1);
  for (const f of ["travelTimeMatrix", "expandedTravelTimeMatrix"]) {
    await assert.rejects(
      peabiru[f](network, one, missing, { mode: ["WALK", "TRANSIT"], departureDatetime: d }),
      /'destinations.id' cannot be missing/,
    );
  }
  const none = { id: [], lon: [], lat: [] };
  await assert.rejects(
    peabiru.isochrone(network, none, { mode: "WALK", cutoffs: 10, departureDatetime: d }),
    /'origins' has no rows/,
  );
  await assert.rejects(peabiru.detailedItineraries(network, pts, pts, { departureDatetime: d, seed: 1 }), /unknown option 'seed'/);
});
