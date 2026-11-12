# peabiru-core <img src="https://raw.githubusercontent.com/StrategicProjects/peabiru-routing/main/site/assets/logo-mark.svg" align="right" width="64" height="64" alt="peabiru logo">

[![CI](https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml/badge.svg)](https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/StrategicProjects/peabiru-routing/blob/main/LICENSE-MIT)
[![Lifecycle: experimental](https://img.shields.io/badge/lifecycle-experimental-orange.svg)](https://lifecycle.r-lib.org/articles/stages.html#experimental)
[![rustc 1.81+](https://img.shields.io/badge/rustc-1.81%2B-blue.svg)](https://github.com/StrategicProjects/peabiru-routing)
[![crates.io](https://img.shields.io/crates/v/peabiru-core.svg)](https://crates.io/crates/peabiru-core)
[![docs.rs](https://docs.rs/peabiru-core/badge.svg)](https://docs.rs/peabiru-core)

The Rust engine of [peabiru](https://github.com/StrategicProjects/peabiru-routing):
multimodal routing on street networks from OpenStreetMap and public
transport from GTFS, in memory, without a Java runtime. It builds a network
from one `.pbf` extract, any number of GTFS feeds and, optionally, an
elevation raster, and computes

- travel-time matrices by walking, cycling and driving, and by public
  transport (any subset of its modes, reached and left on foot, by bicycle
  or by car) over a departure window, with percentiles;
- accessibility, with step, exponential, fixed exponential, linear and
  logistic decay;
- expanded (every departure minute and draw) and arrival-time matrices with
  the breakdown of each trip;
- isochrones: travel-time surfaces on a Web Mercator grid and their polygons,
  or the street segments reached within each cutoff;
- detailed itineraries, leg by leg with their lines: the fastest trip or
  the alternatives;
- fares: travel times within a budget, fares of itineraries, and the Pareto
  frontier of travel time and fare;
- slope costs for walking and cycling from a GeoTIFF raster (Tobler or
  Minetti);
- scenarios: car speeds and levels of traffic stress changed for one call;
- what a network holds: where points snap to the streets, the street and
  transit networks, the service dates, and the GTFS errors of each feed
  (feeds with errors of priority `HIGH` are refused unless asked otherwise).

The same crate is behind the R, Python and JavaScript (WebAssembly)
packages and the `peabiru` command-line tool. The network is plain data,
built once and shared by reference between threads; the crate's only
dependency is `miniz_oxide`.

```rust
use peabiru_core::{
    build_from_dir, street_travel_time_matrix, transit_travel_time_matrix, BuildOptions, Mode,
    RouteParams, TransitData, TransitParams,
};

// a directory with one OpenStreetMap .pbf and the GTFS .zip feeds: here the
// São Paulo sample of the repository; use your own data directory
let (net, layer) = build_from_dir("r/inst/extdata/spo", &BuildOptions::default())?;
let points = vec![(-23.5563, -46.6290), (-23.5577, -46.6321)]; // (lat, lon)

// walking, with r5r's defaults
let walk = street_travel_time_matrix(&net, &points, &points, Mode::Walk, &RouteParams::default())?;
println!("{:?} min", walk.get(0, 1));

// walking and public transport: parameters start from r5r's defaults
let data = TransitData::new(&net, &layer); // once per network
let mut params = TransitParams::default();
params.date = 20190513;
params.departure_time = 14 * 3600;
params.seed = Some(42);
let ttm = transit_travel_time_matrix(&net, &layer, &data, &points, &points, &params)?;
println!("{:?} min at percentiles {:?}", ttm.get(0, 1), ttm.percentiles());
```

Everything public is at the root of the crate. The parameter structs are
non-exhaustive: start from `Default` and set the fields you need, as above.
Results come in types of their own (`StreetMatrix`, `TransitMatrix`,
`Accessibility`, `ExpandedMatrix`, ...) that give single values, rows, or
the whole buffer without copying; `CsvOutput` writes them to one CSV file
per origin.

`build_from_dir` recognises the files by their extension in lower case only
(`.pbf`, `.zip`, `.tif`), as r5r does: a `map.PBF` or a `feed.ZIP` is
ignored. `build_from_bytes` builds the same network from the files' bytes
held in memory, with the feeds in the order you give them. The crate also
builds for `wasm32-unknown-unknown`, where it runs on the calling thread
only.

The `examples/` directory has complete programs: street and transit
matrices, accessibility, expanded matrices, isochrones, detailed
itineraries, fares, elevation, scenarios and network inspection. Each takes
a network directory as its first argument: one `.pbf`, the GTFS `.zip`
feeds and a `.csv` of points with `id`, `lon` and `lat`. In a checkout of
the [repository](https://github.com/StrategicProjects/peabiru-routing) they
default to the São Paulo sample, as in
`cargo run --release -p peabiru-core --example transit_matrix`. The
[Rust guide](https://strategicprojects.github.io/peabiru-routing/rust/)
walks through them; its sources are under
[`site/rust/src`](https://github.com/StrategicProjects/peabiru-routing/tree/main/site/rust/src).

**Status**: early development; the interfaces may still change before 1.0.
R5's own fare calculators are not ported (fares follow r5r's rule-based
fare structure).

## Compatibility with r5r

The functions follow [r5r](https://github.com/ipea/r5r) 2.5.0 (R5 7.6):
the same parameters, defaults and checks, and the same results, value for
value. The repository's parity suites compare them on r5r's sample data.

## Licence and credits

MIT (`LICENSE-MIT`). The routing logic is ported from Conveyal's
[R5](https://github.com/conveyal/r5) 7.6 (MIT). The fare calculator, the
fare structure and the multi-criteria branch of the trip planner are
translations of r5r's Java code (MIT), and polygon isochrones of the
isoband algorithm (MIT). `fastmath.rs`, `fastmath_trig.rs` and part of
`jcompat.rs` are translations of parts of Apache Commons Math 3.0
(Apache-2.0), and `fdlibm.rs` of fdlibm 5.3. The notices and licence texts
of these works are in `LICENSE-THIRD-PARTY`.
