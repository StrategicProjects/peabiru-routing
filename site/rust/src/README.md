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
network in memory from an OpenStreetMap extract (`.pbf`) and GTFS public
transport feeds (`.zip`), and computes travel-time matrices, accessibility,
and per-trip (expanded and arrival) matrices on it, by walking, cycling,
driving and public transport. It needs no Java runtime, keeps nothing on
disk, and spreads the origins of each query over threads.

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

Version 0.0.1, not published yet. Implemented: street matrices (walk,
bicycle, car), walk + public transport matrices with percentiles and Monte
Carlo draws, accessibility with five decay functions, expanded and arrival matrices,
and isochrones. Not yet: detailed itineraries, fares and Pareto
frontiers, bicycle or car access to public transport, elevation. The results
are checked against r5r's on every scenario in the repository; see
[Parity with r5r](parity.md).

The API is a small set of functions and types at the root of the crate;
the engine's internals are private, and the parameter structs are
non-exhaustive, so that new fields and functions can be added without
breaking your code. It may still change before 1.0. The
[API reference](api.md) (rustdoc) documents the current one.

## Credits

The routing logic is a port of Conveyal's [R5](https://github.com/conveyal/r5)
(MIT, © Conveyal); the exponential of the decay functions reproduces Apache
Commons Math 3 (Apache-2.0), which R5 uses. The interface of the R and Python
packages follows [r5r](https://github.com/ipea/r5r) (Ipea), whose results
are the reference for the tests.
