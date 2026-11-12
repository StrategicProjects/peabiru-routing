# peabiru for Rust

<img src="../assets/logo.svg" alt="peabiru" width="252" height="64">

[![CI](https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml/badge.svg)](https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/StrategicProjects/peabiru-routing/blob/main/LICENSE-MIT)
[![Lifecycle: experimental](https://img.shields.io/badge/lifecycle-experimental-orange.svg)](https://lifecycle.r-lib.org/articles/stages.html#experimental)
[![rustc 1.81+](https://img.shields.io/badge/rustc-1.81%2B-blue.svg)](getting-started.md)
<!-- After the first release on crates.io, uncomment:
[![crates.io](https://img.shields.io/crates/v/peabiru-core.svg)](https://crates.io/crates/peabiru-core)
[![docs.rs](https://docs.rs/peabiru-core/badge.svg)](https://docs.rs/peabiru-core)
-->

`peabiru-core` is a multimodal routing engine written in Rust: it builds a
network in memory from an OpenStreetMap extract (`.pbf`), GTFS public
transport feeds (`.zip`) and, optionally, an elevation raster (`.tif`), and
computes travel-time matrices, accessibility, per-trip (expanded and
arrival) matrices, isochrones, detailed itineraries, fares and Pareto
frontiers on it, by walking, cycling, driving and public transport. It
needs no Java runtime, keeps nothing on disk, and spreads the origins of
each query over threads.

The crate is the engine behind four front-ends:

- the R package `peabiru` ([its documentation](../r/index.html));
- the Python package `peabiru` ([its documentation](../python/index.html)),
  which returns pandas data frames;
- the JavaScript package `peabiru` ([its documentation](../js/index.html)),
  the crate compiled to WebAssembly, for browsers and Node;
- the `peabiru` command-line tool (crate `peabiru-cli`), described in
  [The command line](cli.md).

This guide is for using the crate directly from Rust, and for the command
line. The examples are the files in `crates/peabiru-core/examples/` of the
repository, included here verbatim and compiled by `cargo build --examples`;
they run on the small São Paulo network bundled with the R package.

```sh
cargo run --release -p peabiru-core --example transit_matrix
```

## Status

Published on [crates.io](https://crates.io/crates/peabiru-core); this guide
follows the repository. Implemented:

- street matrices (walk, bicycle, car), with car turn costs and turn
  restrictions, and contraction hierarchies that speed up large matrices
  without changing them;
- public transport matrices: any subset of the transit modes, reached and
  left on foot, by bicycle or by car, with percentiles and Monte Carlo draws
  for frequency-based feeds;
- accessibility with five decay functions;
- expanded and arrival matrices;
- isochrones (polygons and street lines);
- detailed itineraries: the fastest trip, or the alternatives;
- fares, travel times within a budget and Pareto frontiers of time and fare;
- slope costs from an elevation raster (Tobler, Minetti);
- scenarios: car speeds and levels of traffic stress changed per call;
- network inspection (snapping, the street and transit networks, service
  dates) and the GTFS errors of each feed, with the refusal of feeds with
  errors of priority `HIGH`;
- CSV output, one file per origin.

Not ported: R5's own fare calculators (only the rule-based fare
structure). For how the results are checked, see the
[comparison with r5r](parity.md).

The API is a small set of functions and types at the root of the crate;
the engine's internals are private, and the parameter structs are
non-exhaustive, so that new fields and functions can be added without
breaking your code. It may still change before 1.0. The
[API reference](api.md) (rustdoc) documents the current one.

## Credits

The routing logic is a port of Conveyal's [R5](https://github.com/conveyal/r5)
(MIT, © Conveyal). Polygon isochrones port the
algorithm of the R package isoband (MIT); the exponential of the decay
functions and other numerics reproduce Apache Commons Math 3 (Apache-2.0),
and the trigonometry of the raster projections fdlibm, as R5 uses them. The
notices are in `LICENSE-THIRD-PARTY`.
