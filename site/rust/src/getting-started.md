# Getting started

## Add the crate

The crate is on [crates.io](https://crates.io/crates/peabiru-core):

```toml
[dependencies]
peabiru-core = "0.0.1"
```

The minimum supported Rust version is 1.81.

## Input data

A network is built from one OpenStreetMap extract in PBF format and, for
public transport, any number of GTFS feeds. The convention is a directory
holding exactly these files:

```text
data/spo/
├── spo_osm.pbf      # streets
├── spo_gtfs.zip     # public transport (optional, several allowed)
└── elevation.tif    # elevation (optional, read with BuildOptions::elevation)
```

Files are recognised by their extension in lower case only, as in r5r: a
`map.PBF` or a `feed.ZIP` in the directory is ignored.

The examples use `r/inst/extdata/spo` in the repository, a small network
around Avenida Paulista, in São Paulo (cut from r5r's sample data), with 27
points in `spo_points.csv`; pass another directory as
the first argument to use your own data.

## Build a network

`peabiru_core::build_from_dir` reads the directory, builds the street
network and the transit layer, and links the stops to the streets:

```rust,ignore
use peabiru_core::{build_from_dir, BuildOptions, TransitData};

let (net, layer) = build_from_dir("r/inst/extdata/spo", &BuildOptions::default())?;
// Walking transfers and egress distances, computed once per network and
// shared by every public transport query:
let data = TransitData::new(&net, &layer);
println!("{} stops in {} feeds", layer.n_stops(), layer.feed_ids().len());
```

`BuildOptions::default()` uses all cores to decode the PBF file; set
`n_threads` on it to use fewer. As r5r's `build_network()` does, the build
fails when a GTFS feed has errors of priority `HIGH` (set
`ignore_gtfs_errors` to build what can be read; see
[Inspecting a network](inspect.md#feeds-with-errors-of-priority-high)), and
`elevation` applies slope costs from a `.tif` raster in the directory (see
[Elevation](elevation.md)). For street-only routing,
`peabiru_core::build_network(path_to_pbf, &opts)` reads just the `.pbf`.
`build_from_bytes` builds the same network from the files' bytes, for
programs without a file system (the JavaScript package uses it).

The network is read-only data in memory: share it between threads by
reference; every query function takes `&StreetNetwork` (and `&TransitLayer`,
`&TransitData`). Its contents are private; methods such as `n_vertices()`,
`n_stops()` or `feed_reports()` describe it.

## Points

Points are `(lat, lon)` pairs in WGS 84 degrees, in the order you want the
results. Origins and destinations are separate slices; nothing requires them
to be the same.

## Parameters

Parameter structs (`RouteParams` for streets, `TransitParams` for public
transport, `AccessibilityParams`, `ItineraryParams`, `BuildOptions`) implement `Default` with
the defaults of the R, Python and JavaScript packages; start from it and set only what
differs:

```rust,ignore
use peabiru_core::{street_travel_time_matrix, Mode, RouteParams};

let mut params = RouteParams::default();
params.max_trip_duration_min = 60;
let m = street_travel_time_matrix(&net, &points, &points, Mode::Bicycle, &params)?;
println!("{:?} min from the first point to the second", m.get(0, 1));
```

The structs are non-exhaustive (later versions may add fields), so they
cannot be written as struct literals outside the crate, not even with
`..Default::default()`. Invalid values are rejected with
`RouteError::Invalid` before any routing.

Next: [how the engine works](how-it-works.md), or straight to the examples,
starting with [street matrices](street-matrix.md).
