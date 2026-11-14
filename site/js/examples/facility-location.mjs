// Facility location: where to open new facilities, and supply against
// demand around each place.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";

const network = await peabiru.buildNetwork({
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
});
// ANCHOR: cover
// id, lon, lat, population and the opportunity counts (schools among them)
const points = parsePoints(await readFile("spo_points.csv", "utf8"));
// the rows of a table of columns where `keep(i)` holds
const rows = (table, keep) => Object.fromEntries(
  Object.entries(table).map(([name, column]) => [name, column.filter((_, i) => keep(i))]),
);
const schools = rows(points, (i) => points.schools[i] > 0);
const others = rows(points, (i) => points.schools[i] === 0);

// two new schools, so that the most residents are within a 12-minute walk
const found = await peabiru.locateFacilities(network, points, others, {
  existing: schools,
  model: "max_cover",
  nFacilities: 2,
  cutoff: 12,
  demandColname: "population",
  mode: "WALK",
});
found.sites.id.forEach((id, k) => {
  console.log(id, found.sites.order[k], found.sites.marginal[k], found.sites.cumulative[k]);
});
const s = found.summary;
const pct = (x) => `${(100 * x / s.total_demand).toFixed(1)}%`;
console.log(`covered today: ${s.covered_before} of ${s.total_demand} residents (${pct(s.covered_before)})`);
console.log(`greedy: ${s.objective_greedy}; after ${s.n_swaps} swap: ${s.covered} (${pct(s.covered)})`);
// ANCHOR_END: cover

// ANCHOR: potential
// what a school at each candidate would cover alone: the four best
const order = [...found.candidates.id.keys()].sort((a, b) => found.candidates.potential[b] - found.candidates.potential[a]);
for (const j of order.slice(0, 4)) {
  console.log(found.candidates.id[j], found.candidates.potential[j], found.candidates.chosen[j]);
}
// ANCHOR_END: potential

// ANCHOR: models
const cover = await peabiru.locateFacilities(network, points, points, {
  model: "set_cover", cutoff: 10, demandColname: "population",
});
console.log(`set_cover: ${cover.sites.id.length} sites put everyone within 10 minutes`);

for (const model of ["p_median", "p_center"]) {
  const r = await peabiru.locateFacilities(network, points, points, {
    model, nFacilities: 2, demandColname: "population", maxTripDuration: 60,
  });
  let [sum, total, longest] = [0, 0, 0];
  r.demand.travel_time.forEach((t, i) => {
    sum += r.demand.demand[i] * t;
    total += r.demand.demand[i];
    longest = Math.max(longest, t);
  });
  console.log(`${model}: ${r.sites.id.join(", ")}; mean ${(sum / total).toFixed(1)} min, longest ${longest} min`);
}
// ANCHOR_END: models

// ANCHOR: catchment
// schools per resident within a 20-minute walk
const fca = await peabiru.floatingCatchment(network, points, schools, {
  cutoff: 20,
  demandColname: "population",
  supplyColname: "schools",
  mode: "WALK",
});
fca.supply.id.forEach((id, j) => console.log(id, fca.supply.supply[j], fca.supply.catchment_demand[j]));
const per10k = Array.from(fca.demand.accessibility, (a) => 10000 * a);
console.log(`schools per 10,000 residents: from ${Math.min(...per10k).toFixed(2)} to ${Math.max(...per10k).toFixed(2)}`);
let shared = 0;
fca.demand.accessibility.forEach((a, i) => { shared += a * fca.demand.demand[i]; });
console.log(`shared out: ${shared.toFixed(1)} schools`);
// ANCHOR_END: catchment

// ANCHOR: table
// the same from a table of travel times: no network, and only the ids
const sites = { id: [...schools.id, ...others.id], lon: [...schools.lon, ...others.lon], lat: [...schools.lat, ...others.lat] };
const times = await peabiru.travelTimeMatrix(network, points, sites, { mode: "WALK", maxTripDuration: 12 });
const again = await peabiru.locateFacilities(null, { id: points.id, population: points.population }, { id: others.id }, {
  existing: { id: schools.id },
  model: "max_cover",
  nFacilities: 2,
  cutoff: 12,
  demandColname: "population",
  travelTimes: times,
});
console.log(again.sites.id.join(" ") === found.sites.id.join(" "));
// ANCHOR_END: table

network.free();

// checks, not shown in the documentation
assert.equal(found.summary.covered, 46217);
assert.deepEqual(again.summary, found.summary);
assert.ok(Math.abs(shared - 7) < 1e-9);
