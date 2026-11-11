# peabiru for JavaScript

<img src="../assets/logo.svg" alt="peabiru" width="252" height="64">

[![CI](https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml/badge.svg)](https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/StrategicProjects/peabiru-routing/blob/main/LICENSE-MIT)
[![Lifecycle: experimental](https://img.shields.io/badge/lifecycle-experimental-orange.svg)](https://lifecycle.r-lib.org/articles/stages.html#experimental)
[![Node 18+](https://img.shields.io/badge/node-18%2B-blue.svg)](getting-started.md)
<!-- After the first release on npm, uncomment:
[![npm](https://img.shields.io/npm/v/peabiru.svg)](https://www.npmjs.com/package/peabiru)
-->

The npm package `peabiru` computes travel-time matrices, accessibility,
and minute-by-minute (expanded) and arrival-time matrices on multimodal
networks built from an OpenStreetMap extract (`.pbf`) and GTFS public
transport feeds (`.zip`), by walking, cycling, driving and public
transport. Its routing engine is the Rust core of peabiru compiled to
WebAssembly: it runs in browsers, in bundlers and in Node (18 or later),
with no server and no Java, and gives the numbers of the R and Python
packages, which give those of r5r.

```js
{{#include ../examples/first-matrix.mjs:matrix}}
```

The same engine is behind:

- the R package `peabiru` ([its documentation](../r/index.html));
- the Python package `peabiru` ([its documentation](../python/index.html));
- the Rust crate `peabiru-core` and the command-line tool
  ([the Rust guide](../rust/index.html)).

This guide covers the JavaScript API: [getting started](getting-started.md)
in Node and in a browser, the [routing functions](routing.md) and the
columns they return, [threads and workers](threads.md),
[bundlers](bundlers.md), what differs from
[the R and Python packages](differences.md), and the
[API reference](api.md), generated from the package's type declarations.

## Status

Version 0.0.1, not published on npm yet: build it from the repository (see
[Getting started](getting-started.md)). Implemented: street matrices (walk,
bicycle, car), walk + public transport matrices with percentiles and Monte
Carlo draws, accessibility with five decay functions, and expanded and
arrival matrices. Not yet: isochrones, detailed itineraries, fares and
Pareto frontiers, bicycle or car access to public transport, elevation.
What is not supported throws `UnsupportedError`; nothing is silently
ignored. The API may still change.

## The examples

Every example in this guide is a file in `site/js/examples/` of the
repository, included here verbatim, and so is the output shown under it.
`node site/js/run_examples.mjs` runs them all on the São Paulo sample
network of the Python package (`python/peabiru/data/spo/`): the Node
examples with `node`, the browser examples in headless Chrome, as plain ES
modules and, with `VITE` set to a Vite executable, bundled by Vite. It
fails if any of them prints something other than the output shown here.

## Credits

The routing logic is a port of Conveyal's [R5](https://github.com/conveyal/r5)
(MIT, © Conveyal); the exponential of the decay functions reproduces Apache
Commons Math 3 (Apache-2.0), which R5 uses. The interface follows
[r5r](https://github.com/ipea/r5r) (Ipea), whose results are the reference
for the tests. The sample network contains map data © OpenStreetMap
contributors, under the [Open Database License](https://www.openstreetmap.org/copyright).
