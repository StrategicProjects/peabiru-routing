# Getting started

## Add the crate

The crate is not on crates.io yet. Use it from the repository:

```toml
[dependencies]
peabiru-core = { git = "https://github.com/StrategicProjects/peabiru-routing" }
```

The minimum supported Rust version is 1.81.

## Input data

A network is built from one OpenStreetMap extract in PBF format and, for
public transport, any number of GTFS feeds. The convention is a directory
holding exactly these files:

```text
data/spo/
├── spo_osm.pbf      # streets
└── spo_gtfs.zip     # public transport (optional, several allowed)
```

The examples use `r/inst/extdata/spo` in the repository, a small network
around Avenida Paulista, in São Paulo (cut from r5r's sample data), with 27
points in `spo_points.csv`; pass another directory as
the first argument to use your own data.

## Build a network

`peabiru_core::transit::build_from_dir` reads the directory, builds the
street network and the transit layer, and links the stops to the streets:

```rust,ignore
use peabiru_core::street::BuildOptions;
use peabiru_core::transit::{build_from_dir, matrix::TransitData};

let (net, layer) = build_from_dir("data/spo", &BuildOptions { n_threads: 0 })?;
// Walking transfers and egress distances, computed once per network and
// shared by every public transport query:
let data = TransitData::new(&net, &layer);
```

`n_threads = 0` means all cores (the threads decode the PBF file). For
street-only routing, `peabiru_core::street::build_network(path_to_pbf, &opts)`
reads just the `.pbf`.

The network is plain data in memory: share it between threads by reference;
every query function takes `&StreetNetwork` (and `&TransitLayer`,
`&TransitData`).

## Points

Points are `(lat, lon)` pairs in WGS 84 degrees, in the order you want the
results. Origins and destinations are separate slices; nothing requires them
to be the same.

## Parameters

Parameter structs (`RouteParams` for streets, `TransitParams` for public
transport) implement `Default` with the defaults of the R and Python
packages, so only what differs needs to be written. Invalid values are
rejected with an error before any routing.

Next: [how the engine works](how-it-works.md), or straight to the examples,
starting with [street matrices](street-matrix.md).
