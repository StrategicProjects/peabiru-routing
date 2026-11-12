// Trip options on r5r's sample of Porto Alegre with its fare structure:
// the trade-off between time and fare, every minute's frontier, the legs of
// an option with all four criteria.
import assert from "node:assert/strict";
import * as peabiru from "peabiru";
import { poaFile } from "./sample-poa.mjs";

/** Print rows (objects) as an aligned table of the given columns. */
function show(rows, columns) {
  const cells = [columns, ...rows.map((r) => columns.map((c) => String(r[c])))];
  const width = columns.map((_, k) => Math.max(...cells.map((r) => r[k].length)));
  for (const r of cells) console.log(r.map((v, k) => v.padEnd(width[k])).join("  ").trimEnd());
}

// ANCHOR: setup
// the feeds in the order macOS lists r5r's sample directory, which r5r uses there
const network = await peabiru.buildNetwork({
  pbf: await poaFile("poa_osm.pbf"),
  gtfs: [
    { name: "poa_trensurb.zip", data: await poaFile("poa_trensurb.zip") },
    { name: "poa_eptc.zip", data: await poaFile("poa_eptc.zip") },
  ],
});
const fares = peabiru.readFareStructure(await poaFile("fares/fares_poa.zip"));
const origin = { id: ["89a90128053ffff"], lon: [-51.1863771572777], lat: [-30.0722552482611] };
const destination = { id: ["89a90129c9bffff"], lon: [-51.1733521719011], lat: [-30.0511737750423] };
const departureDatetime = "2019-05-13 14:00:00";
// ANCHOR_END: setup

// ANCHOR: time-fare
const res = await peabiru.tripOptions(network, origin, destination, {
  departureDatetime, criteria: ["TIME", "FARE"], fareStructure: fares,
});
show(res.options, ["option", "total_time", "fare", "transfers", "walk_time", "departure_time", "n_departures", "description"]);
// ANCHOR_END: time-fare

// ANCHOR: by-departure
const minutes = await peabiru.tripOptions(network, origin, destination, {
  departureDatetime, timeWindow: 3, criteria: ["TIME", "FARE"], fareStructure: fares,
  byDeparture: true, itineraries: false,
});
show(minutes.departures, ["option", "departure_time", "arrival_time", "total_time"]);
// ANCHOR_END: by-departure

// ANCHOR: legs
const all = await peabiru.tripOptions(network, origin, destination, {
  departureDatetime, fareStructure: fares, dropGeometry: true,
});
console.log(`${all.options.length} options with the four criteria; the legs of the first:`);
show(all.itineraries.filter((l) => l.option === 1), ["segment", "mode", "cumulative_fare", "segment_duration", "wait", "distance", "route"]);
// ANCHOR_END: legs

network.free();

// checks, not shown in the documentation
assert.ok(res.options.length >= 3);
assert.ok(new Set(res.options.map((o) => o.fare)).size >= 3);
