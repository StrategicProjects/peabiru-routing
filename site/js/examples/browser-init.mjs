// Loading the WebAssembly module from a path of your own, before anything
// else. The runner serves a copy of pkg/peabiru_bg.wasm at static/.
import * as peabiru from "peabiru";

// ANCHOR: init
await peabiru.init("static/peabiru_bg.wasm"); // a URL, a Response, bytes or a WebAssembly.Module
// ANCHOR_END: init

const network = await peabiru.buildNetwork({ pbf: await (await fetch("spo_osm.pbf")).arrayBuffer() });
console.log("edges:", network.info().edges);
network.free();
