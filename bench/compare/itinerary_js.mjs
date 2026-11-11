// detailedItineraries() of the npm package, written as CSV for
// bench/compare/itinerary_csv.py to compare with r5r's (or another
// binding's) output.
//
//   node bench/compare/itinerary_js.mjs <dir with .pbf and .zip> <origins.csv> <destinations.csv> \
//     <out.csv> <mode,...> [YYYY-MM-DDTHH:MM:SS] [nThreads]
//
// The points files have columns id,lat,lon (paired row by row); the
// output has one row per leg and the line as WKT.
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildNetwork, detailedItineraries } from "../../js/lib/index.js";

const [dir, ofile, dfile, out, modes, when = "2019-05-13T14:00:00", threads = "1"] = process.argv.slice(2);
const read = (f) => {
  const [head, ...lines] = readFileSync(f, "utf8").trim().split("\n");
  const cols = head.split(",");
  const pts = { id: [], lat: [], lon: [] };
  for (const l of lines) {
    const v = l.split(",");
    pts.id.push(v[cols.indexOf("id")]);
    pts.lat.push(Number(v[cols.indexOf("lat")]));
    pts.lon.push(Number(v[cols.indexOf("lon")]));
  }
  return pts;
};
const files = readdirSync(dir).sort();
const pbf = readFileSync(join(dir, files.find((f) => f.endsWith(".pbf"))));
const gtfs = files.filter((f) => f.endsWith(".zip")).map((name) => ({ name, data: readFileSync(join(dir, name)) }));
const net = await buildNetwork({ pbf, gtfs, nThreads: Number(threads) });
const fc = await detailedItineraries(net, read(ofile), read(dfile), {
  mode: modes.split(","),
  departureDatetime: new Date(when),
  onWarning: () => {},
});
const q = (s) => (/[",\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s);
const rows = ["from_id,to_id,option,departure_time,total_duration,total_distance,segment,mode,segment_duration,wait,distance,route,geometry"];
for (const f of fc.features) {
  const p = f.properties;
  const wkt = `LINESTRING (${f.geometry.coordinates.map(([x, y]) => `${x} ${y}`).join(", ")})`;
  rows.push([q(p.from_id), q(p.to_id), p.option, p.departure_time, p.total_duration, p.total_distance, p.segment,
    p.mode, p.segment_duration, p.wait, p.distance, q(p.route), q(wkt)].join(","));
}
writeFileSync(out, rows.join("\n") + "\n");
net.free();
