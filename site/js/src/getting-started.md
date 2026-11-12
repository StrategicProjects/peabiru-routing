# Getting started

## Install

The package is on [npm](https://www.npmjs.com/package/peabiru):

```sh
npm install peabiru
```

To build it from a clone of the repository instead, you need Rust, the
`wasm32-unknown-unknown` target and
[wasm-pack](https://github.com/rustwasm/wasm-pack):

```sh
rustup target add wasm32-unknown-unknown
cd peabiru-routing/js
sh build.sh                      # pkg/peabiru.js and pkg/peabiru_bg.wasm
node --test tests/*.test.mjs     # optional: the package's tests
```

Then add that build to your project from the directory (npm links to it), or
from a tarball made with `npm pack` in `js/` (a copy of the files that
would be published):

```sh
npm install /path/to/peabiru-routing/js
npm install /path/to/peabiru-routing/js/peabiru-<version>.tgz   # after npm pack
```

The package is an ES module only (`import`, not `require()` before Node
22), with its TypeScript declarations in `lib/index.d.ts`. It needs Node 18
or later, or a browser with WebAssembly and ES modules.

## Input data

A network is built from the bytes of one OpenStreetMap extract in PBF
format and, for public transport, of any number of GTFS feeds (and,
optionally, of an [elevation raster](elevation.md)). The package
reads no files itself: you pass it the bytes, read from disk in Node,
fetched or picked by the user in a browser.

The examples use the sample network of the Python package
(`python/peabiru/data/spo/` in the repository): the streets around Avenida
Paulista, in São Paulo, the SPTrans frequency-based GTFS feed, and 27
points in `spo_points.csv`, with columns `id`, `lon`, `lat` and the
opportunity counts `population`, `schools`, `jobs` and `healthcare`. They
read the points with this small function (`sample.mjs`):

```js
{{#include ../examples/sample.mjs:parsePoints}}
```

## In Node

```js
{{#include ../examples/first-matrix.mjs:all}}
```

```text
{{#include ../examples/output/first-matrix.txt}}
```

`buildNetwork()` loads the WebAssembly module the first time it is called,
reads the extract and the feeds, and builds the network in the module's
memory, where it stays until `network.free()`. Every function that reads
or routes on a network returns a Promise. A matrix is an object of columns (`from_id`, `to_id`, and one
`Int32Array` per percentile), one row per origin and destination reached,
in the order of the origins: see [Routing functions](routing.md).

## In a browser

The same code runs in a page, with `fetch()` instead of `readFile()`. With
`nThreads` above 1 the network is built in Web Workers and routing does not
block the page (see [Threads and workers](threads.md)):

```js
{{#include ../examples/browser-first-matrix.mjs:all}}
```

```text
{{#include ../examples/output/browser-first-matrix.txt}}
```

Without a bundler, map the package name to its files with an import map
(here the package is in `node_modules/peabiru/`, served with the page; the
worker script and the `.wasm` file are found from `lib/index.js`, in `lib/`
and `pkg/`, so serve both directories):

```html
{{#include ../examples/browser.html:importmap}}
```

Served as `application/wasm`, the module compiles while it downloads
(with another type it is compiled once downloaded). With Vite there is
nothing to configure: see [Bundlers and loading the module](bundlers.md).

Files that the user picks in an `<input type="file">` are `File` objects,
whose bytes come from `file.arrayBuffer()`:

```js
{{#include ../examples/browser-files.mjs:files}}
```

```text
{{#include ../examples/output/browser-files.txt}}
```

The GTFS feeds load in the order of the `gtfs` array; with more than one
feed that order can change a few results of expanded and arrival matrices
(see [Coming from R or Python](r-python.md#feed-order)).

## What the network holds

```js
{{#include ../examples/network-info.mjs:info}}
```

```text
{{#include ../examples/output/network-info.txt}}
```

`transitAvailability()` counts the services running on a date.
`buildNetwork()` throws when a GTFS feed has errors
of priority `"HIGH"`; `getGtfsErrors()` lists them. `coreVersion()` gives the version of the Rust core, once the
module is loaded:

```js
{{#include ../examples/network-info.mjs:init}}
```
