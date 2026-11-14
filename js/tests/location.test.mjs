// locateFacilities() and floatingCatchment() on the sample network: the
// values recorded here are the ones the Python and R packages give.

import assert from "node:assert/strict";
import { test } from "node:test";
import * as peabiru from "../lib/index.js";
import { DEPARTURE, sampleNetwork, samplePoints, sum } from "./helpers.mjs";

/** The rows of a table of columns where `keep(i)` holds. */
function rows(points, keep) {
  const idx = [...points.id.keys()].filter(keep);
  const out = {};
  for (const [name, col] of Object.entries(points)) out[name] = idx.map((i) => col[i]);
  return out;
}

function split() {
  const points = samplePoints();
  return {
    points,
    schools: rows(points, (i) => points.schools[i] > 0),
    others: rows(points, (i) => points.schools[i] === 0),
  };
}

test("max_cover returns its tables and the recorded solution", async () => {
  const net = await sampleNetwork();
  const { points, schools, others } = split();
  const found = await peabiru.locateFacilities(net, points, others, {
    existing: schools, model: "max_cover", nFacilities: 2, cutoff: 12, demandColname: "population", mode: "WALK",
  });
  assert.deepEqual(Object.keys(found), ["sites", "candidates", "demand", "summary"]);
  assert.deepEqual(Object.keys(found.sites), ["id", "lon", "lat", "order", "marginal", "cumulative"]);
  assert.deepEqual(Object.keys(found.candidates), ["id", "lon", "lat", "potential", "chosen"]);
  assert.deepEqual(Object.keys(found.demand), ["id", "lon", "lat", "demand", "facility_id", "travel_time", "covered"]);
  assert.equal(found.candidates.id.length, 21);
  assert.equal(found.demand.id.length, 27);
  // greedy covers 46040 residents, one swap takes it to 46217 (the optimum)
  assert.deepEqual(found.sites.id, ["89a8100c03bffff", "89a8100c383ffff"]);
  assert.deepEqual([...found.sites.order], [1, 2]);
  assert.deepEqual([...found.sites.marginal], [5658, 2683]);
  assert.deepEqual([...found.sites.cumulative], [43534, 46217]);
  assert.deepEqual(found.summary, {
    model: "max_cover", n_sites: 2, objective_before: 37876, objective_greedy: 46040, objective: 46217,
    covered_before: 37876, covered: 46217, total_demand: 46273, unservable_demand: 0, n_swaps: 1,
  });
  assert.deepEqual([...found.candidates.potential.slice(0, 4)], [3936, 4829, 5658, 5764]);
  assert.deepEqual(found.candidates.id.filter((_, i) => found.candidates.chosen[i]).sort(), [...found.sites.id].sort());
  assert.equal(sum(found.demand.demand.filter((_, i) => found.demand.covered[i])), 46217);

  // the same from a table of travel times, without a network or coordinates
  const sites = { id: [...schools.id, ...others.id], lon: [...schools.lon, ...others.lon], lat: [...schools.lat, ...others.lat] };
  const ttm = await peabiru.travelTimeMatrix(net, points, sites, { mode: "WALK", maxTripDuration: 12 });
  const again = await peabiru.locateFacilities(
    null, { id: points.id, population: points.population }, { id: others.id },
    { existing: { id: schools.id }, model: "max_cover", nFacilities: 2, cutoff: 12, demandColname: "population", travelTimes: ttm },
  );
  assert.deepEqual(Object.keys(again.sites), ["id", "order", "marginal", "cumulative"]);
  assert.deepEqual(again.sites.id, found.sites.id);
  assert.deepEqual(again.summary, found.summary);
});

test("the other models give the recorded solutions", async () => {
  const net = await sampleNetwork();
  const points = samplePoints();
  const sc = await peabiru.locateFacilities(net, points, points, { model: "set_cover", cutoff: 10, demandColname: "population" });
  assert.equal(sc.summary.objective, 7);
  assert.equal(sc.sites.id.length, 7);
  assert.ok(sc.demand.covered.every(Boolean));
  assert.equal(sc.sites.cumulative[6], 46273);

  const med = await peabiru.locateFacilities(net, points, points, {
    model: "p_median", nFacilities: 2, demandColname: "population", maxTripDuration: 60,
  });
  assert.deepEqual(med.sites.id, ["89a8100c027ffff", "89a8100c03bffff"]);
  assert.equal(med.summary.objective_greedy, 560703);
  assert.equal(med.summary.objective, 446711);
  assert.equal(med.summary.covered, null);
  assert.equal(sum(med.demand.demand.map((w, i) => w * med.demand.travel_time[i])), 446711);
  assert.deepEqual(med.demand.facility_id.slice(0, 2), ["89a8100c03bffff", "89a8100c03bffff"]);

  const cen = await peabiru.locateFacilities(net, points, points, {
    model: "p_center", nFacilities: 2, demandColname: "population", maxTripDuration: 60,
  });
  assert.deepEqual(cen.sites.id, ["89a8100c02fffff", "89a8100c067ffff"]);
  assert.equal(cen.summary.objective, 19);
  assert.equal(Math.max(...cen.demand.travel_time), 19);

  const transit = await peabiru.locateFacilities(net, points, points, {
    model: "p_median", nFacilities: 3, demandColname: "population", mode: ["WALK", "TRANSIT"],
    departureDatetime: DEPARTURE, seed: 1, maxTripDuration: 60,
  });
  assert.deepEqual(transit.sites.id, ["89a8100c03bffff", "89a8100c1dbffff", "89a8100c15bffff"]);
  assert.equal(transit.summary.objective, 351640);
});

test("floatingCatchment computes the two steps", async () => {
  const net = await sampleNetwork();
  const times = [
    { from_id: "a", to_id: "f", travel_time: 5 }, { from_id: "a", to_id: "g", travel_time: 20 },
    { from_id: "b", to_id: "f", travel_time: 12 }, { from_id: "b", to_id: "g", travel_time: 8 },
  ];
  const byHand = await peabiru.floatingCatchment(
    null, { id: ["a", "b"], residents: [100, 200] }, { id: ["f", "g"], places: [10, 30] },
    { cutoff: 15, demandColname: "residents", supplyColname: "places", travelTimes: times },
  );
  assert.deepEqual(Object.keys(byHand.demand), ["id", "demand", "accessibility"]);
  assert.deepEqual(Object.keys(byHand.supply), ["id", "supply", "catchment_demand", "ratio"]);
  assert.deepEqual([...byHand.supply.catchment_demand], [300, 200]);
  assert.deepEqual([...byHand.supply.ratio], [10 / 300, 30 / 200]);
  assert.deepEqual([...byHand.demand.accessibility], [10 / 300, 10 / 300 + 30 / 200]);

  const { points, schools } = split();
  const base = { demandColname: "population", supplyColname: "schools", mode: "WALK" };
  const step = await peabiru.floatingCatchment(net, points, schools, { ...base, cutoff: 20 });
  assert.deepEqual([...step.supply.catchment_demand], [22033, 17501, 10593, 21263, 22664, 11967]);
  assert.deepEqual(
    [...step.demand.accessibility.slice(0, 3)],
    [4.538646575591159e-05, 4.538646575591159e-05, 0.00013978842931675365],
  );
  const shared = (f) => sum(f.demand.demand.map((w, i) => w * f.demand.accessibility[i]));
  assert.ok(Math.abs(shared(step) - 7) < 1e-12);
  const logistic = await peabiru.floatingCatchment(net, points, schools, { ...base, cutoff: 20, decayFunction: "logistic", decayValue: 5 });
  assert.deepEqual([...logistic.demand.accessibility.slice(0, 2)], [6.917279110814355e-05, 7.752430155338007e-05]);
  assert.ok(Math.abs(shared(logistic) - 7) < 1e-12);
  const fixed = await peabiru.floatingCatchment(net, points, schools, { ...base, decayFunction: "fixed_exponential", decayValue: 0.001 });
  assert.ok(fixed.demand.accessibility.every((v) => v > 0));
});

test("bad arguments are refused", async () => {
  const net = await sampleNetwork();
  const { points, schools, others } = split();
  const refused = (promise, pattern) => assert.rejects(promise, (e) => pattern.test(e.message) || assert.fail(e.message));
  await refused(peabiru.locateFacilities(net, points, others, { model: "median", cutoff: 10 }), /'model'/);
  await refused(peabiru.locateFacilities(net, points, others, {}), /cutoff failed: must not be null/);
  await refused(peabiru.locateFacilities(net, points, others, { model: "p_median", cutoff: 10 }), /cutoff failed: must be null/);
  await refused(peabiru.locateFacilities(net, points, others, { cutoff: 10, nFacilities: 0 }), /nFacilities/);
  await refused(peabiru.locateFacilities(net, points, others, { cutoff: 10, nFacilities: 22 }), /nFacilities is 22, but there are only 21 candidate sites/);
  await refused(peabiru.locateFacilities(net, points, points, { cutoff: 10, existing: schools }), /must not share ids/);
  await refused(peabiru.locateFacilities(net, points, others, { cutoff: 10, demandColname: "residents" }), /no column 'residents'/);
  await refused(peabiru.locateFacilities(null, points, others, { cutoff: 10 }), /needs a network or a table/);
  await refused(peabiru.locateFacilities(null, points, others, { cutoff: 10, travelTimes: [], seed: 1 }), /routing options/);
  await refused(peabiru.locateFacilities(net, points, others, { cutoff: 10, percentiles: 25 }), /unknown option 'percentiles'/);
  await refused(peabiru.floatingCatchment(net, points, schools, {}), /cutoff failed: must not be null/);
  await refused(
    peabiru.floatingCatchment(net, points, schools, { cutoff: 10, decayFunction: "fixed_exponential", decayValue: 0.001 }),
    /cutoff failed: must be null/,
  );
});
