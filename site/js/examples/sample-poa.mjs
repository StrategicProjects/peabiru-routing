// The sample data of Porto Alegre from r5r 2.5.0 (an OpenStreetMap extract,
// two GTFS feeds with timetables and 15 points of interest), downloaded
// once into the temporary directory (about 2 MB). Node only.
import { existsSync, mkdirSync, renameSync, writeFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

// ANCHOR: poaFile
const POA = "https://raw.githubusercontent.com/ipea/r5r/v2.5.0/r-package/inst/extdata/poa/";

/** The bytes of a file of r5r's Porto Alegre sample, downloaded the first time. */
export async function poaFile(name) {
  const dir = join(tmpdir(), "peabiru-poa");
  const path = join(dir, name);
  if (!existsSync(path)) {
    const response = await fetch(POA + name);
    if (!response.ok) throw new Error(`${POA + name}: HTTP ${response.status}`);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(`${path}.part`, new Uint8Array(await response.arrayBuffer()));
    renameSync(`${path}.part`, path);
  }
  return readFile(path);
}
// ANCHOR_END: poaFile
