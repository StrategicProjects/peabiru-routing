// The errors gtfs-lib finds in GTFS feeds, and a feed whose errors of
// priority HIGH stop the build.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";
import { storedZip } from "./sample-zip.mjs";

// ANCHOR: errors
const gtfs = [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }];
const errors = await peabiru.getGtfsErrors(gtfs); // the feeds, as buildNetwork() takes them
console.log(Object.keys(errors));
for (let i = 0; i < 3; i++) {
  console.log(errors.file[i], errors.line[i], errors.type[i], errors.field[i], errors.id[i], errors.priority[i]);
}
console.log(errors.priority.filter((p) => p === "HIGH").length, "errors of priority HIGH");
// ANCHOR_END: errors

// ANCHOR: refused
// a small feed made up here, with no agency.txt
const bus = storedZip({
  "stops.txt": "stop_id,stop_name,stop_lat,stop_lon\nA,A,-23.5563,-46.6290\nB,B,-23.5577,-46.6321\n",
  "routes.txt": "route_id,route_type\nR,3\n",
  "trips.txt": "route_id,service_id,trip_id\nR,S,T1\n",
  "stop_times.txt": "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nT1,14:00:00,14:00:00,A,1\nT1,14:05:00,14:05:00,B,2\n",
  "calendar.txt": "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nS,1,1,1,1,1,1,1,20190101,20191231\n",
});
const feeds = [...gtfs, { name: "bus.zip", data: bus }];
try {
  await peabiru.buildNetwork({ pbf: await readFile("spo_osm.pbf"), gtfs: feeds });
} catch (e) {
  console.log(`${e.name}: ${e.message}`);
}
const found = await peabiru.getGtfsErrors(feeds);
const high = found.priority.map((p, i) => (p === "HIGH" ? `${found.file[i] ?? "(feed)"}: ${found.type[i]}` : null)).filter(Boolean);
console.log(high);
// ANCHOR_END: refused

// checks, not shown in the documentation
assert.equal(high.length, 1);
