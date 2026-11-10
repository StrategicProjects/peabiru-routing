# peabiru (JavaScript)

Multimodal routing on [OpenStreetMap](https://www.openstreetmap.org) street
networks and [GTFS](https://gtfs.org) public transport, in WebAssembly:
travel-time matrices, accessibility, and minute-by-minute (expanded) and
arrival-time matrices, by walking, cycling, driving and public transport.

The routing engine is the Rust core of the peabiru packages for R and
Python, compiled to WebAssembly. It runs in browsers, in bundlers and in
Node (18 or later), with no server and no Java, and gives the same numbers
as the native packages, which give the numbers of
[r5r](https://github.com/ipea/r5r) 2.5.0.

**Status**: version 0.0.1, early development; not published on npm yet.

```js
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";

const network = await peabiru.buildNetwork({
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
});

const points = {
  id: ["a", "b", "c"],
  lon: [-46.6290, -46.6321, -46.6355],
  lat: [-23.5563, -23.5577, -23.5601],
};

const ttm = peabiru.travelTimeMatrix(network, points, points, {
  mode: ["WALK", "TRANSIT"],
  departureDatetime: "2019-05-13 14:00:00",
  percentiles: [25, 50, 75],
  seed: 42,
});
// ttm.from_id, ttm.to_id: string arrays; ttm.travel_time_p25, ...: Int32Array

network.free();
```

In a browser, read the files with `fetch(url).then((r) => r.arrayBuffer())`
or from an `<input type="file">` (`file.arrayBuffer()`).

| Function | What it computes |
|---|---|
| `buildNetwork({ pbf, gtfs })` | a network from an OpenStreetMap extract and any GTFS feeds |
| `travelTimeMatrix(network, origins, destinations, options)` | travel times between every origin and destination |
| `accessibility(network, origins, destinations, options)` | cumulative or decay-weighted access to opportunities |
| `expandedTravelTimeMatrix(network, origins, destinations, options)` | travel times minute by minute, with a breakdown of each trip |
| `arrivalTravelTimeMatrix(network, origins, destinations, options)` | the latest trip that arrives by a given time |
| `coreVersion()`, `version` | versions of the Rust core and of this package |

`network.info()` gives the sizes of a network, `network.transitAvailability(date)`
the services running on a date, `network.r5rWarnings()` the feeds that r5r
would refuse, and `network.free()` releases its memory (a network lives in
the WebAssembly module's memory until then).

The full types are in `lib/index.d.ts`.

## Arguments

The functions have the arguments and defaults of r5r and of the Python and
R packages, in camelCase and in an options object: `departure_datetime`
is `departureDatetime`, `max_trip_duration` is `maxTripDuration`, and so on
(`timeWindow`, `percentiles`, `maxWalkTime`, `maxBikeTime`, `maxCarTime`,
`walkSpeed`, `bikeSpeed`, `maxRides`, `maxLts`, `drawsPerMinute`, `seed`,
`opportunitiesColnames`, `decayFunction`, `cutoffs`, `decayValue`,
`breakdown`, `arrivalDatetime`, ...). An option name that is not one of
them is an error. Each options object also takes `onWarning`, a function
that receives the warnings r5r would give (by default `console.warn`).

- **Points** are columns `{ id, lon, lat }` (arrays or typed arrays) or an
  array of `{ id, lon, lat }` objects. For `accessibility()` the
  destinations also hold the opportunity columns. Ids that are not strings
  are converted with `String()`, with a warning; points with missing
  coordinates are not routed, with a warning; duplicated ids are an error.
- **Dates and times** are strings `"YYYY-MM-DD HH:MM:SS"` (or with `T`, or
  without seconds), taken as they read: the clock time of the GTFS feeds.
  A `Date` also works, and is read in the local time zone of the
  JavaScript runtime, which is rarely the feeds' time zone: prefer strings.
- **Frequency-based GTFS** (`frequencies.txt`) is routed with Monte Carlo
  draws: results are reproducible, and equal to r5r's, only with a `seed`.
  Without one, a random seed is drawn for each call.
- `nThreads`, `verbose` and `progress` are checked as r5r checks them, but
  this version always routes on one thread and prints nothing.

## Results

Results are objects of columns named as r5r names them, never arrays of
row objects (a matrix can have millions of rows):

- `travelTimeMatrix()`: `from_id`, `to_id` (arrays of strings) and one
  `Int32Array` `travel_time_pXX` per percentile (`travel_time_p05`, ...),
  in whole minutes;
- `accessibility()`: `id`, `opportunity` (strings), `percentile`, `cutoff`
  (`Int32Array`; no `cutoff` with `"fixed_exponential"`) and
  `accessibility` (`Float64Array`), one row per origin, opportunity,
  percentile and cutoff;
- `expandedTravelTimeMatrix()` and `arrivalTravelTimeMatrix()`: `from_id`,
  `to_id`, `departure_time` (`"HH:MM:SS"`), `draw_number`, with
  `breakdown: true` the times `access_time`, `wait_time`, `ride_time`,
  `transfer_time`, `egress_time` (`Float64Array`, minutes), then `routes`
  (strings), `n_rides` (with `breakdown`) and `total_time`.

Missing values are `peabiru.NA_INT32` (-2147483648) in the `Int32Array`
columns, `NaN` in the `Float64Array` columns and `null` in the string
columns. Rows are origin-major, in the order of the inputs; pairs that are
not reached are left out, as in r5r.

To walk through the rows of a result:

```js
for (let i = 0; i < ttm.from_id.length; i++) {
  const t = ttm.travel_time_p50[i];
  if (t !== peabiru.NA_INT32) console.log(ttm.from_id[i], ttm.to_id[i], t);
}
```

## What is not supported

What peabiru does not support yet throws `peabiru.UnsupportedError`; it is
never silently ignored: public transport with bicycle or car access or
egress, transit modes other than `"TRANSIT"` (such as `"BUS"`), fares
(`fareStructure`, `maxFare`), scenarios (`newCarspeeds`, `carspeedScale`,
`newLts`), `outputDir`, expanded and arrival matrices by a street mode
alone. Elevation is not supported either: `buildNetwork()` takes no
elevation raster.

Not available in this version:

- **Cancelling** a running computation. It runs on the calling thread and
  cannot be interrupted from JavaScript; to be able to stop it, run it in a
  Worker (below) and call `worker.terminate()`.
- **Several threads.** Everything runs on one thread. The results never
  depend on the number of threads, so a later version can share the origins
  of one call out among a pool of Web Workers, each holding its own copy of
  the network, and give the same results.

## Feed order

r5r loads the GTFS feeds of its data directory in the order the operating
system lists the directory, and with more than one feed that order can
change a few results (another stop or route for some trips, in expanded and
arrival matrices). Here the feeds load in the order of the `gtfs` array. To
reproduce a network that r5r, or peabiru in R or Python, built from a
directory, pass the feeds in the order that directory lists them (on macOS
the r5r sample of Porto Alegre lists `poa_trensurb.zip` before
`poa_eptc.zip`; Linux lists them the other way round). `network.info().feeds`
gives the feed ids in loading order.

A feed's `name` stands for its file name: it names the feed in messages
and, when the feed has no `feed_info.feed_id`, gives the feed id (the name
without `.zip`), which prefixes its stop ids.

## In a Worker

A large matrix can take seconds, and on the main thread of a page the page
freezes meanwhile. Run peabiru in a module Worker instead:

```js
// worker.js
import * as peabiru from "peabiru";

let network;
self.onmessage = async ({ data }) => {
  try {
    if (data.type === "build") {
      network?.free();
      network = await peabiru.buildNetwork({ pbf: data.pbf, gtfs: data.gtfs });
      self.postMessage({ type: "built", info: network.info() });
    } else if (data.type === "ttm") {
      const m = peabiru.travelTimeMatrix(network, data.origins, data.destinations, data.options);
      const times = Object.keys(m).filter((k) => k.startsWith("travel_time_p")).map((k) => m[k].buffer);
      self.postMessage({ type: "ttm", result: m }, times); // move the typed arrays, do not copy them
    }
  } catch (e) {
    self.postMessage({ type: "error", message: e.message, name: e.name });
  }
};
```

```js
// main.js
const worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
worker.postMessage({ type: "build", pbf, gtfs }, [pbf.buffer, ...gtfs.map((f) => f.data.buffer)]);
worker.onmessage = ({ data }) => { /* "built", then the results */ };
```

A `Network` cannot be sent from one Worker to another: build it in the
Worker that routes on it. A Worker that loads the module itself can call
`peabiru.initSync(bytes)` with the bytes of `peabiru_bg.wasm` (exported as
`peabiru/peabiru_bg.wasm`), or `await peabiru.init(url)` to fetch it from
elsewhere.

## Loading the WebAssembly module

`buildNetwork()` loads `pkg/peabiru_bg.wasm` the first time it is called:
in Node from the file next to the package, elsewhere with `fetch()` from
`new URL("../pkg/peabiru_bg.wasm", import.meta.url)`, which bundlers such as
Vite and webpack 5 recognise and copy to the output. When the file is served
from somewhere else, call `await peabiru.init(urlOrResponseOrBytes)` before
anything else. The module is about 670 KB (280 KB compressed).

The package is an ES module only (`import`, not `require()` before Node
22).

## Building from source

In a checkout of the repository, with Rust (the `wasm32-unknown-unknown`
target: `rustup target add wasm32-unknown-unknown`) and
[wasm-pack](https://github.com/rustwasm/wasm-pack) installed:

```sh
cd js
sh build.sh                       # pkg/peabiru.js and pkg/peabiru_bg.wasm (release)
node --test tests/*.test.mjs      # the tests, on the sample network of the Python package
```

`build.sh` runs `wasm-pack build --target web --no-pack`, which compiles
the crate in `js/` (the bindings, `src/lib.rs`) with the core as a
dependency, runs `wasm-bindgen` and optimises the module with `wasm-opt`.
The JavaScript API (`lib/index.js`, `lib/index.d.ts`) is written by hand
around the generated `pkg/peabiru.js`. The crate is outside the Cargo
workspace of the repository, with its own `Cargo.lock`, like the R
package's crate.

`bench/compare/js_parity.mjs` compares the results with those of the
native command-line tool, cell by cell, on r5r's sample data
(`node bench/compare/js_parity.mjs poa spo sample`).

## Licence and credits

MIT (`LICENSE`) for peabiru's own code; the authors are listed in
`package.json`. The routing logic is ported from Conveyal's
[R5](https://github.com/conveyal/r5) 7.6 (MIT), and parts from Apache
Commons Math (Apache-2.0; its Mersenne Twister is BSD-3-Clause) and other
works; their notices are in `LICENSE-THIRD-PARTY`. The WebAssembly module
also contains the Rust crates listed, with their licences, in
`LICENSE-RUST-CRATES`.
