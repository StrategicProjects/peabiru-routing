// The sample network shipped with the Python package (python/peabiru/data/spo):
// the streets around Avenida Paulista, the SPTrans frequency-based GTFS feed
// and 27 points.

import { readFileSync } from "node:fs";
import * as peabiru from "../lib/index.js";

const SPO = new URL("../../python/peabiru/data/spo/", import.meta.url);

export const read = (name) => readFileSync(new URL(name, SPO));

/** The sample's points as columns: id, lon, lat and the opportunity counts. */
export function samplePoints() {
  const [header, ...lines] = read("spo_points.csv").toString().trim().split(/\r?\n/);
  const names = header.split(",");
  const rows = lines.map((l) => l.split(","));
  const pts = {};
  names.forEach((n, k) => {
    pts[n] = n === "id" ? rows.map((r) => r[k]) : Float64Array.from(rows, (r) => Number(r[k]));
  });
  return pts;
}

let network;
/** The sample network, built once. */
export async function sampleNetwork() {
  network ??= await peabiru.buildNetwork({
    pbf: read("spo_osm.pbf"),
    gtfs: [{ name: "spo_gtfs.zip", data: read("spo_gtfs.zip") }],
  });
  return network;
}

export const DEPARTURE = "2019-05-13 14:00:00";

export function sum(a) {
  let s = 0;
  for (const v of a) s += v;
  return s;
}
