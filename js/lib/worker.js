// A worker of a peabiru network's pool (see `nThreads` in buildNetwork()).
//
// Each worker holds its own WebAssembly instance and its own copy of the
// network, built from the same bytes in the same feed order as every other
// worker of the pool. lib/index.js sends it a share of the origins of a
// call and gets back the result columns as typed arrays, which it moves
// (transfers) instead of copying. It runs as a module Web Worker in
// browsers and as a worker_threads Worker in Node; it is not meant to be
// loaded by hand.

import {
  initSync,
  Network,
  accessibility,
  expanded,
  fare_check,
  fare_setup,
  find_snap,
  isochrone_polygons,
  itineraries,
  pareto,
  street_bbox,
  street_edges,
  street_ttm,
  street_vertices,
  transit_routes,
  transit_service_dates,
  transit_stops,
  transit_ttm,
  trip_options_js,
} from "../pkg/peabiru.js";

const FUNCTIONS = {
  street_ttm,
  transit_ttm,
  accessibility,
  expanded,
  itineraries,
  trip_options: trip_options_js,
  isochrone_polygons,
  street_edges,
  availability: (net, date) => net.transit_availability(date),
  find_snap,
  street_bbox,
  street_vertices,
  transit_stops,
  transit_routes,
  transit_service_dates,
  fare_setup,
  fare_check,
  pareto,
};

let network = null;

/** The buffers of the typed arrays in a result, to move them. */
function buffers(x, out = []) {
  if (ArrayBuffer.isView(x)) {
    if (!out.includes(x.buffer)) out.push(x.buffer);
  } else if (Array.isArray(x)) {
    for (const v of x) buffers(v, out);
  }
  return out;
}

const errorOf = (e) => ({ name: e && e.name ? String(e.name) : "Error", message: e && e.message !== undefined ? String(e.message) : String(e) });

function handle(data, post) {
  if (data.type === "build") {
    try {
      initSync({ module: data.module });
      network = Network.build(data.pbf, data.feeds, data.tif, data.elevation);
      // the sizes and warnings, which the calling side keeps
      const [stops, patterns, services, frequencies] = network.transit_sizes();
      post({
        type: "ready",
        info: {
          vertices: network.n_vertices(),
          edges: network.n_edges(),
          elevation: network.elevation(),
          feeds: Array.from(network.feed_ids()),
          stops,
          patterns,
          services,
          frequencies: frequencies === 1,
        },
        warnings: Array.from(network.r5r_warnings()),
      });
    } catch (e) {
      post({ type: "failed", error: errorOf(e) });
    }
    return;
  }
  if (data.type === "call") {
    let result;
    try {
      result = FUNCTIONS[data.fn](network, ...data.args);
    } catch (e) {
      // a trap (WebAssembly.RuntimeError) may leave the instance unusable:
      // the calling side then replaces this worker
      post({ type: "done", id: data.id, error: errorOf(e), fatal: e instanceof WebAssembly.RuntimeError });
      return;
    }
    post({ type: "done", id: data.id, result }, buffers(result));
  }
}

if (typeof self !== "undefined" && typeof self.postMessage === "function") {
  // a Web Worker
  self.onmessage = (e) => handle(e.data, (msg, transfer) => self.postMessage(msg, transfer || []));
} else {
  // a Node worker_threads Worker (no top-level await, which bundlers that
  // emit workers as classic scripts reject; the port keeps the messages
  // that arrive before the listener)
  import(/* webpackIgnore: true */ /* @vite-ignore */ "node:worker_threads").then(({ parentPort }) => {
    parentPort.on("message", (data) => handle(data, (msg, transfer) => parentPort.postMessage(msg, transfer || [])));
  });
}
