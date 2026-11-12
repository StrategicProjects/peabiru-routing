// The fare scenarios of bench/compare/fares_bindings.R, with the npm
// package, written as CSV.
//
//   FEEDS=a.zip,b.zip node bench/compare/fares_js.mjs <network dir> <fares.zip> <points.csv> <out dir> [nThreads]
//
// <points.csv> has id,lon,lat,jobs; the first 15 rows are the origins.
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import * as peabiru from "../../js/lib/index.js";

const [dir, faresZip, pointsCsv, out, threads = "1"] = process.argv.slice(2);
mkdirSync(out, { recursive: true });
const files = readdirSync(dir).sort();
const pbf = readFileSync(join(dir, files.find((f) => f.endsWith(".pbf"))));
// the feeds in the order R5 loads them (the order the file system lists
// them, which Node's readdirSync does not keep): FEEDS="a.zip,b.zip"
const order = process.env.FEEDS ? process.env.FEEDS.split(",") : files.filter((f) => f.endsWith(".zip"));
const gtfs = order.map((name) => ({ name, data: readFileSync(join(dir, name)) }));
const net = await peabiru.buildNetwork({ pbf, gtfs, nThreads: Number(threads) });
const fs = peabiru.readFareStructure(readFileSync(faresZip));
const [head, ...lines] = readFileSync(pointsCsv, "utf8").trim().split("\n");
const cols = head.split(",");
const pts = { id: [], lon: [], lat: [], jobs: [] };
for (const l of lines) {
  const v = l.split(",");
  for (const c of Object.keys(pts)) pts[c].push(c === "id" ? v[cols.indexOf(c)] : Number(v[cols.indexOf(c)]));
}
const slice = (a, b) => Object.fromEntries(Object.entries(pts).map(([k, v]) => [k, v.slice(a, b)]));
const o = slice(0, 15);
const t0 = "2019-05-13 14:00:00";
const M = ["WALK", "TRANSIT"];

// CSV as pandas writes it: NA empty, numbers in their shortest form
const cell = (v) => (v === null || v === undefined || (typeof v === "number" && Number.isNaN(v)) || v === peabiru.NA_INT32 ? "" : String(v));
function writeColumns(name, table) {
  const keys = Object.keys(table);
  const n = table[keys[0]].length;
  const rows = [keys.join(",")];
  for (let i = 0; i < n; i++) rows.push(keys.map((k) => cell(table[k][i])).join(","));
  writeFileSync(join(out, name), rows.join("\n") + "\n");
}
function writeRows(name, rows) {
  const keys = Object.keys(rows[0]);
  writeFileSync(join(out, name), [keys.join(","), ...rows.map((r) => keys.map((k) => cell(r[k])).join(","))].join("\n") + "\n");
}

writeColumns("ttm.csv", await peabiru.travelTimeMatrix(net, o, pts, {
  mode: M, departureDatetime: t0, fareStructure: fs, maxFare: 4.8, drawsPerMinute: 2, percentiles: [10, 50, 90],
}));
writeColumns("access.csv", await peabiru.accessibility(net, o, pts, {
  opportunitiesColnames: "jobs", mode: M, departureDatetime: t0, fareStructure: fs, maxFare: 7.2, cutoffs: [20, 45], drawsPerMinute: 1,
}));
writeColumns("pareto.csv", await peabiru.paretoFrontier(net, o, pts, {
  mode: M, departureDatetime: t0, fareStructure: fs, fareCutoffs: [4.5, 4.8, 9, 15],
}));
writeRows("itineraries.csv", await peabiru.detailedItineraries(net, o, slice(15, 30), {
  mode: M, departureDatetime: t0, fareStructure: fs, shortestPath: false, dropGeometry: true,
}));
writeFileSync(join(out, "setup.zip"), peabiru.writeFareStructure(await peabiru.setupFareStructure(net, 4.8, { by: "AGENCY_NAME" })));
net.free();
