// The tripOptions() scenarios of bench/compare/trip_options_bindings.R, with
// the npm package, written as CSV.
//
//   FEEDS=a.zip,b.zip node bench/compare/trip_options_js.mjs <poa dir> <fares.zip> <poa points.csv> <spo dir> <spo points.csv> <out dir> [nThreads]
//
// The points files have id,lon,lat; Porto Alegre's first 30 rows are the
// origins and the next 30 the destinations, Sao Paulo's first 10 and next 10.
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import * as peabiru from "../../js/lib/index.js";

const [poa, faresZip, poaPts, spo, spoPts, out, threads = "1"] = process.argv.slice(2);
mkdirSync(out, { recursive: true });

async function network(dir, feeds) {
  const files = readdirSync(dir).sort();
  const pbf = readFileSync(join(dir, files.find((f) => f.endsWith(".pbf"))));
  // the feeds in the order R5 loads them (the order the file system lists
  // them, which Node's readdirSync does not keep)
  const order = feeds ? feeds.split(",") : files.filter((f) => f.endsWith(".zip"));
  const gtfs = order.map((name) => ({ name, data: readFileSync(join(dir, name)) }));
  return peabiru.buildNetwork({ pbf, gtfs, nThreads: Number(threads) });
}

function points(path) {
  const [head, ...lines] = readFileSync(path, "utf8").trim().split("\n");
  const cols = head.split(",");
  const pts = { id: [], lon: [], lat: [] };
  for (const l of lines) {
    const v = l.split(",");
    for (const c of Object.keys(pts)) pts[c].push(c === "id" ? v[cols.indexOf(c)] : Number(v[cols.indexOf(c)]));
  }
  return (a, b) => Object.fromEntries(Object.entries(pts).map(([k, v]) => [k, v.slice(a, b)]));
}

const field = (v) => {
  if (v === null || v === undefined) return "";
  const s = String(v);
  return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
};
function csv(path, rows) {
  if (!rows.length) return writeFileSync(path, "");
  const cols = Object.keys(rows[0]);
  writeFileSync(path, [cols.join(","), ...rows.map((r) => cols.map((c) => field(r[c])).join(","))].join("\n") + "\n");
}
function save(name, r) {
  csv(join(out, `${name}_options.csv`), r.options);
  if (r.itineraries) csv(join(out, `${name}_legs.csv`), r.itineraries);
  if (r.departures) csv(join(out, `${name}_departures.csv`), r.departures);
}

const t0 = "2019-05-13 14:00:00";
const M = ["WALK", "TRANSIT"];
let net = await network(poa, process.env.FEEDS);
const fs = peabiru.readFareStructure(readFileSync(faresZip));
let pts = points(poaPts);
let [o, d] = [pts(0, 30), pts(30, 60)];
save("poa_fares", await peabiru.tripOptions(net, o, d, {
  mode: M, departureDatetime: t0, timeWindow: 5, fareStructure: fs, byDeparture: true, dropGeometry: true,
}));
save("poa_bike", await peabiru.tripOptions(net, o, d, {
  mode: ["BICYCLE", "TRANSIT"], departureDatetime: t0, timeWindow: 3, criteria: ["TIME", "TRANSFERS"],
  maxRides: 2, dropGeometry: true,
}));
save("poa_cheap", await peabiru.tripOptions(net, o, d, {
  mode: M, departureDatetime: t0, timeWindow: 2, criteria: ["FARE", "WALK"], fareStructure: fs, maxFare: 7.2,
  itineraries: false, byDeparture: true,
}));
net.free();
net = await network(spo);
pts = points(spoPts);
save("spo", await peabiru.tripOptions(net, pts(0, 10), pts(10, 20), {
  mode: M, departureDatetime: t0, timeWindow: 5, byDeparture: true, dropGeometry: true,
}));
net.free();
