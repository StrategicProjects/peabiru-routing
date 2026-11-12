// Each function of the npm package takes the options of r5r's function of
// the same name (its arguments after the positional ones, in camelCase),
// plus `onWarning` and `signal`, and refuses any other: nothing is silently
// ignored. The arguments come from the snapshot of r5r 2.5.0's formals
// (r5r-formals.csv, written by tools/r5r_formals.R).
//
//   node --test bench/compare/js_options.test.mjs
//
// Needs the WebAssembly package built (js/build.sh). Env: R5R_FORMALS, the
// snapshot (default: bench/compare/r5r-formals.csv, else the R package's
// test fixture).

import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import * as peabiru from "../../js/lib/index.js";
import { sampleNetwork, samplePoints } from "../../js/tests/helpers.mjs";

const FORMALS = [
  process.env.R5R_FORMALS,
  fileURLToPath(new URL("./r5r-formals.csv", import.meta.url)),
  fileURLToPath(new URL("./testthat/fixtures/r5r-formals.csv", import.meta.url)),
].find((p) => p && existsSync(p));
const camel = (s) => s.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());

// r5r function -> [JS function, the positional arguments]
const FUNCTIONS = {
  travel_time_matrix: ["travelTimeMatrix", ["r5r_network", "origins", "destinations"]],
  expanded_travel_time_matrix: ["expandedTravelTimeMatrix", ["r5r_network", "origins", "destinations"]],
  arrival_travel_time_matrix: ["arrivalTravelTimeMatrix", ["r5r_network", "origins", "destinations"]],
  accessibility: ["accessibility", ["r5r_network", "origins", "destinations"]],
  detailed_itineraries: ["detailedItineraries", ["r5r_network", "origins", "destinations"]],
  pareto_frontier: ["paretoFrontier", ["r5r_network", "origins", "destinations"]],
  isochrone: ["isochrone", ["r5r_network", "origins"]],
  find_snap: ["findSnap", ["r5r_network", "points"]],
  setup_fare_structure: ["setupFareStructure", ["r5r_network", "base_fare"]],
  check_transit_availability: ["checkTransitAvailability", ["r5r_network"]],
};
// not options anywhere: r5r's deprecated arguments
const NEVER = new Set(["r5r_core", "sample_size"]);

function formals() {
  const [, ...lines] = readFileSync(FORMALS, "utf8").trim().split(/\r?\n/);
  const out = {};
  for (const l of lines) {
    const [fun, , arg] = l.split(",").map((v) => v.replace(/^"|"$/g, ""));
    (out[fun] ??= []).push(arg);
  }
  return out;
}

const unknown = (e) => e instanceof TypeError && /^unknown option /.test(e.message);

test("the snapshot of r5r's formals is there", () => {
  assert.ok(FORMALS, "r5r-formals.csv not found: set R5R_FORMALS");
});

test("every function takes r5r's options and no others", async () => {
  const network = await sampleNetwork();
  const pts = samplePoints();
  const f = formals();
  const all = new Set();
  for (const args of Object.values(f)) for (const a of args) all.add(camel(a));
  const probe = Symbol("probe");
  const wrong = [];
  for (const [rfun, [fn, positional]] of Object.entries(FUNCTIONS)) {
    const options = f[rfun].filter((a) => !positional.includes(a) && !NEVER.has(a)).map(camel);
    const head = positional.slice(1).map((a) => (a === "base_fare" ? 4.4 : pts));
    const call = async (o) => {
      try {
        await peabiru[fn](network, ...head, o);
      } catch (e) {
        return e;
      }
      return null;
    };
    for (const o of options) {
      const e = await call({ [o]: probe });
      if (unknown(e)) wrong.push(`${fn}() refuses r5r's option '${o}'`);
    }
    for (const o of [...all].filter((a) => !options.includes(a) && !["onWarning", "signal"].includes(a))) {
      if (positional.map(camel).includes(o)) continue;
      const e = await call({ [o]: 1 });
      if (!unknown(e)) wrong.push(`${fn}() takes '${o}', which r5r's ${rfun}() does not have`);
    }
  }
  assert.deepEqual(wrong, []);
});
