# peabiru-core <img src="https://raw.githubusercontent.com/StrategicProjects/peabiru-routing/main/site/assets/logo-mark.svg" align="right" width="64" height="64" alt="peabiru logo">

[![CI](https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml/badge.svg)](https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/StrategicProjects/peabiru-routing/blob/main/LICENSE-MIT)
[![Lifecycle: experimental](https://img.shields.io/badge/lifecycle-experimental-orange.svg)](https://lifecycle.r-lib.org/articles/stages.html#experimental)
[![rustc 1.81+](https://img.shields.io/badge/rustc-1.81%2B-blue.svg)](https://github.com/StrategicProjects/peabiru-routing)
<!-- After the first release on crates.io, uncomment:
[![crates.io](https://img.shields.io/crates/v/peabiru-core.svg)](https://crates.io/crates/peabiru-core)
[![docs.rs](https://docs.rs/peabiru-core/badge.svg)](https://docs.rs/peabiru-core)
-->

The Rust engine of [peabiru](https://github.com/StrategicProjects/peabiru-routing):
multimodal routing on street networks from OpenStreetMap and public
transport from GTFS, in memory, without a Java runtime. It builds a network
from one `.pbf` extract and any number of GTFS feeds and computes

- travel-time matrices by walking, cycling and driving, and by walking plus
  public transport over a departure window, with percentiles;
- accessibility, with step, exponential, fixed exponential, linear and
  logistic decay;
- expanded (every departure minute and draw) and arrival-time matrices with
  the breakdown of each trip.

The same crate is behind the R package, the Python package and the `peabiru`
command-line tool. The network is plain data, built once and shared by
reference between threads; the crate's only dependency is `miniz_oxide`.

```rust
use peabiru_core::street::BuildOptions;
use peabiru_core::transit::build_from_dir;
use peabiru_core::transit::matrix::{travel_time_matrix, TransitData, TransitParams};

let (net, layer) = build_from_dir("data/spo", &BuildOptions::default())?;
let data = TransitData::new(&net, &layer);
let points = vec![(-23.5563, -46.6290), (-23.5577, -46.6321)]; // (lat, lon)
let params = TransitParams {
    date: 20190513,
    departure_time: 14 * 3600,
    seed: Some(42),
    ..TransitParams::default()
};
let ttm = travel_time_matrix(&net, &layer, &data, &points, &points, &params)?;
```

More in the `examples/` directory (`cargo run --release -p peabiru-core
--example transit_matrix`) and in the Rust guide built by `site/build.sh` in
the repository.

**Status**: version 0.0.1, early development; not published yet. Isochrones,
detailed itineraries, fares, bicycle or car access to public transport and
elevation are not implemented.

## Licence and credits

MIT (`LICENSE-MIT`). The routing logic is ported from Conveyal's
[R5](https://github.com/conveyal/r5) 7.6 (MIT), and the results are checked
value for value against those of [r5r](https://github.com/ipea/r5r)
2.5.0. `fastmath.rs` and part of `jcompat.rs` are translations of parts of
Apache Commons Math 3.0 (Apache-2.0). The notices and licence texts of these works are in
`LICENSE-THIRD-PARTY`.
