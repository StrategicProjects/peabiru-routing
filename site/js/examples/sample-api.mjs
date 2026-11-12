// The setup of the examples of the API reference (the comments of
// js/lib/index.d.ts): run_examples.mjs runs each of them after these lines.
// ANCHOR: setup
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";

const pbf = await readFile("spo_osm.pbf");
const gtfs = [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }];
const network = await peabiru.buildNetwork({ pbf, gtfs });
// three points of the sample, with the number of jobs at each
const points = {
  id: ["a", "b", "c"],
  lon: [-46.6290, -46.6321, -46.6355],
  lat: [-23.5563, -23.5577, -23.5601],
  jobs: [153, 551, 120],
};
// ANCHOR_END: setup
