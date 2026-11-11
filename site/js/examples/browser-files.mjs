// A network from files the user picks in an <input type="file" multiple>:
// one .pbf and any GTFS .zip feeds. The function is tested with File
// objects made from the sample (a headless browser cannot pick files).
import * as peabiru from "peabiru";

// ANCHOR: files
/** Build a network from the files of an <input type="file" multiple>. */
async function networkFromFiles(files) {
  const all = [...files];
  const pbf = all.filter((f) => f.name.endsWith(".pbf"));
  if (pbf.length !== 1) throw new Error("pick exactly one .pbf file");
  const gtfs = all
    .filter((f) => f.name.endsWith(".zip"))
    .sort((a, b) => (a.name < b.name ? -1 : 1)) // the feeds load in this order
    .map(async (f) => ({ name: f.name, data: await f.arrayBuffer() }));
  return peabiru.buildNetwork({ pbf: await pbf[0].arrayBuffer(), gtfs: await Promise.all(gtfs) });
}

// <input id="files" type="file" multiple accept=".pbf,.zip">
// document.querySelector("#files").addEventListener("change", async (event) => {
//   const network = await networkFromFiles(event.target.files);
//   ...
// });
// ANCHOR_END: files

const file = async (name) => new File([await (await fetch(name)).arrayBuffer()], name);
const network = await networkFromFiles([await file("spo_gtfs.zip"), await file("spo_osm.pbf")]);
console.log(network.info().feeds, network.info().stops, "stops");
network.free();
