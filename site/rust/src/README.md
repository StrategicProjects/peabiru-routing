# peabiru for Rust

<img src="../assets/logo.svg" alt="peabiru" width="252" height="64">

`peabiru-core` is a multimodal routing engine: it builds a network from an
OpenStreetMap extract (`.pbf`) and GTFS public transport feeds (`.zip`), and
computes travel-time matrices, accessibility, and per-trip (expanded and
arrival) matrices on it. It reproduces the results of
[r5r](https://github.com/ipeaGIT/r5r) 2.5.0, which drives Conveyal's
[R5](https://github.com/conveyal/r5) 7.6, value for value, without a Java
runtime.

The crate is the engine behind three front-ends:

- the R package `peabiru` ([its documentation](../r/index.html)), with r5r's
  functions and arguments;
- the Python package `peabiru` (in development, [its documentation](../python/index.html));
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

Version 0.0.1, not published yet. What is implemented gives r5r's results on
every scenario checked so far: street matrices (walk, bicycle, car), walk +
public transport matrices with percentiles and Monte Carlo draws,
accessibility with the five decay functions, and expanded and arrival
matrices. Not yet: isochrones, detailed itineraries, fares and Pareto
frontiers, bicycle or car access to public transport, elevation.

The API mirrors R5's structure more than a polished Rust interface: expect
it to change before 1.0. The [API reference](api.md) (rustdoc) documents the
current one.

## Credits

The routing logic is a port of R5 (MIT, © Conveyal); the exponential of the
decay functions reproduces Apache Commons Math 3 (Apache-2.0), which R5 uses.
r5r (IPEA) defines the interface and supplies the reference results.
