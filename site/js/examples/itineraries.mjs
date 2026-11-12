// Detailed itineraries on the sample of Porto Alegre (timetabled GTFS):
// the fastest trip, the alternatives, all-to-all pairs, the OpenStreetMap
// ways of a leg, and fares.
import assert from "node:assert/strict";
import * as peabiru from "peabiru";
import { parsePoints } from "./sample.mjs";
import { poaFile } from "./sample-poa.mjs";

/** Print rows (objects) as an aligned table of the given columns. */
function show(rows, columns) {
  const cells = [columns, ...rows.map((r) => columns.map((c) => String(r[c])))];
  const width = columns.map((_, k) => Math.max(...cells.map((r) => r[k].length)));
  for (const r of cells) console.log(r.map((v, k) => v.padEnd(width[k])).join("  ").trimEnd());
}

/** The rows of each option, in order. */
function byOption(rows) {
  const groups = new Map();
  for (const r of rows) groups.set(r.option, [...(groups.get(r.option) ?? []), r]);
  return [...groups.values()];
}

/** An option's legs, such as "WALK > BUS 255 > WALK". */
const via = (rows) => rows.map((r) => (r.route ? `${r.mode} ${r.route}` : r.mode)).join(" > ");

// ANCHOR: network
// the feeds in the order macOS lists the sample directory
const network = await peabiru.buildNetwork({
  pbf: await poaFile("poa_osm.pbf"),
  gtfs: [
    { name: "poa_trensurb.zip", data: await poaFile("poa_trensurb.zip") },
    { name: "poa_eptc.zip", data: await poaFile("poa_eptc.zip") },
  ],
});
const points = parsePoints((await poaFile("poa_points_of_interest.csv")).toString());
const pick = (...k) => ({ id: k.map((i) => points.id[i]), lon: k.map((i) => points.lon[i]), lat: k.map((i) => points.lat[i]) });
// ANCHOR_END: network

// ANCHOR: fastest
// paired row by row: the first origin with the first destination, and so on
const origins = pick(0, 1, 2);
const destinations = pick(3, 4, 5);
const trips = await peabiru.detailedItineraries(network, origins, destinations, {
  mode: ["WALK", "TRANSIT"],
  departureDatetime: "2019-05-13 14:00:00",
});
console.log(trips.type, trips.features.length, "legs");
const legs = trips.features.map((f) => f.properties);
show(legs, ["from_id", "to_id", "option", "departure_time", "total_duration", "segment", "mode", "segment_duration", "wait", "distance", "route"]);
const line = trips.features[2].geometry;
console.log(line.type, line.coordinates.length, "points, from", line.coordinates[0]);
// ANCHOR_END: fastest

// ANCHOR: alternatives
const alternatives = (suboptimalMinutes) =>
  peabiru.detailedItineraries(network, pick(1), pick(4), {
    mode: ["WALK", "TRANSIT"],
    departureDatetime: "2019-05-13 14:00:00",
    shortestPath: false, // McRAPTOR's alternatives, one per sequence of routes
    suboptimalMinutes,
    dropGeometry: true, // plain rows, no GeoJSON
  });
const options = await alternatives(0);
for (const rows of byOption(options)) {
  console.log(`option ${rows[0].option}: ${rows[0].departure_time}, ${rows[0].total_duration} min, ${via(rows)}`);
}
const wider = await alternatives(3);
console.log(`with suboptimalMinutes: 3, ${new Set(wider.map((r) => r.option)).size} options`);
// ANCHOR_END: alternatives

// ANCHOR: allToAll
const all = await peabiru.detailedItineraries(network, pick(0, 1), pick(3, 4, 5), {
  mode: "BICYCLE",
  departureDatetime: "2019-05-13 14:00:00",
  allToAll: true, // 2 origins x 3 destinations
  dropGeometry: true,
});
show(all, ["from_id", "to_id", "mode", "total_duration", "total_distance"]);
// ANCHOR_END: allToAll

// ANCHOR: osmLinkIds
const walk = await peabiru.detailedItineraries(network, pick(0), pick(3), {
  mode: "WALK",
  departureDatetime: "2019-05-13 14:00:00",
  osmLinkIds: true, // needs the geometry
});
const leg = walk.features[0].properties;
console.log("OpenStreetMap ways:", leg.osm_id_list);
console.log("street edges:", leg.edge_id_list);
// ANCHOR_END: osmLinkIds

// ANCHOR: fares
const fares = await peabiru.setupFareStructure(network, 4.8); // every ride 4.80, a transfer too
const priced = await peabiru.detailedItineraries(network, pick(1), pick(4), {
  mode: ["WALK", "TRANSIT"],
  departureDatetime: "2019-05-13 14:00:00",
  fareStructure: fares,
  shortestPath: false,
  dropGeometry: true,
});
for (const rows of byOption(priced)) {
  const paid = rows.map((r) => r.cumulative_fare).join(", ");
  console.log(`option ${rows[0].option}: ${rows[0].total_duration} min, fare ${rows[0].total_fare} (${via(rows)}; paid after each leg: ${paid})`);
}
// ANCHOR_END: fares

network.free();

// checks, not shown in the documentation
assert.ok(legs.some((l) => l.mode !== "WALK"));
assert.ok(new Set(options.map((r) => r.option)).size > 1);
assert.equal(new Set(all.map((r) => `${r.from_id} ${r.to_id}`)).size, 6);
assert.match(leg.osm_id_list, /^\[\d+(, \d+)*\]$/);
assert.ok(priced.every((r) => typeof r.total_fare === "number"));
