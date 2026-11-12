// What a network holds, and the versions of the package and of the core.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";

// ANCHOR: init
// buildNetwork() loads the WebAssembly module itself; coreVersion() needs it loaded
await peabiru.init();
const versions = `package ${peabiru.version}, core ${peabiru.coreVersion()}`;
// ANCHOR_END: init

const network = await peabiru.buildNetwork({
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
});

// ANCHOR: info
console.log(network.info());
console.log(await network.transitAvailability("2019-05-13"));
console.log("warnings of the build:", network.r5rWarnings().length);
console.log("workers:", network.workers);
// ANCHOR_END: info

network.free();

// checks, not shown in the documentation (the versions change with every release)
assert.match(versions, /^package \d+\.\d+\.\d+\S*, core \d+\.\d+\.\d+\S*$/);
