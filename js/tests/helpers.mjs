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

/** A stored (uncompressed) zip of `files`, {name: text}; the reader does not check CRCs. */
export function storedZip(files) {
  const enc = new TextEncoder();
  const parts = [];
  const central = [];
  let offset = 0;
  const u16 = (v) => [v & 255, v >> 8];
  const u32 = (v) => [v & 255, (v >> 8) & 255, (v >> 16) & 255, v >>> 24];
  for (const [name, text] of Object.entries(files)) {
    const n = enc.encode(name);
    const data = enc.encode(text);
    const sizes = [...u32(data.length), ...u32(data.length)];
    const local = [...u32(0x04034b50), 20, 0, 0, 0, 0, 0, ...new Array(8).fill(0), ...sizes, ...u16(n.length), 0, 0];
    parts.push(Uint8Array.from(local), n, data);
    central.push(Uint8Array.from([...u32(0x02014b50), 20, 0, 20, 0, 0, 0, 0, 0, ...new Array(8).fill(0), ...sizes, ...u16(n.length), ...new Array(12).fill(0), ...u32(offset)]), n);
    offset += local.length + n.length + data.length;
  }
  const cdSize = central.reduce((s, a) => s + a.length, 0);
  const count = Object.keys(files).length;
  const end = Uint8Array.from([...u32(0x06054b50), 0, 0, 0, 0, ...u16(count), ...u16(count), ...u32(cdSize), ...u32(offset), 0, 0]);
  return Buffer.concat([...parts, ...central, end]);
}
