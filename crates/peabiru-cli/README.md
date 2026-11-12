# peabiru-cli

`peabiru`, the command-line tool of
[peabiru](https://github.com/StrategicProjects/peabiru-routing): multimodal
routing on OpenStreetMap and GTFS from the shell, without R, Python or Java.
It builds a network from an OpenStreetMap `.pbf` extract, GTFS feeds and,
optionally, an elevation raster, and writes travel-time matrices,
accessibility, isochrones, detailed itineraries, fares and network exports
as CSV or GeoJSON. The engine is the crate
[`peabiru-core`](https://crates.io/crates/peabiru-core), the same as behind
the R, Python and JavaScript packages, so the numbers are theirs (and
r5r 2.5.0's): the options take those packages' argument names in
kebab-case, with the same defaults and checks.

## Install

```sh
cargo install peabiru-cli      # Rust 1.81 or later; installs the binary `peabiru`
```

From a checkout of the repository: `cargo build --release -p peabiru-cli`
(the binary is `target/release/peabiru`).

## Inputs and outputs

- **Network**: a directory with one OpenStreetMap `.pbf` file, any number of
  GTFS `.zip` feeds and an optional `.tif` elevation raster, read as r5r's
  `build_network()` reads it (lower-case extensions; the feeds in the order
  the file system lists them); or a `.pbf` file with `--gtfs feed.zip`
  (repeatable, loaded in the order given) and `--tif raster.tif`. With a
  raster, walking and cycling have slope costs (`--elevation TOBLER`, the
  default, `MINETTI` or `NONE`). A feed with GTFS errors of priority HIGH
  is refused, as r5r refuses it (`peabiru info DIR --gtfs-errors` lists
  them; `--ignore-gtfs-errors` builds anyway). The network is built in
  memory for each command, in about a second for a city of the size of
  Porto Alegre.
- **Points**: CSV files with a header and the columns `id`, `lon` and `lat`
  (WGS 84), found by name, quoted fields allowed (RFC 4180). Accessibility
  reads its opportunity columns from the destinations.
- **Results**: CSV on standard output, or in `-o FILE`; geometries as
  GeoJSON (`--format geojson`) or as WKT in a CSV column; `--output-dir`
  writes one file per origin (or pair) as r5r's `output_dir`. Missing
  values are empty fields. Progress and warnings go to standard error
  (`-q` silences the progress).
- **Exit status**: 0 done, 1 a file or network that cannot be read or
  written, 2 a bad argument, 130 interrupted. Ctrl-C stops a computation
  between origins.

## A tour

The examples use the São Paulo sample of the repository,
`r/inst/extdata/spo` (cut from r5r's sample data; map data © OpenStreetMap
contributors, under the Open Database License, see `LICENSE-THIRD-PARTY`),
with 27 points in `spo_points.csv`.

```console
$ peabiru info r/inst/extdata/spo
network: r/inst/extdata/spo
street vertices: 2609
street edges: 5518
turn restrictions: 0
bounding box: -46.6477135 -23.5653412 -46.6113004 -23.5371085 (xmin ymin xmax ymax)
elevation: NONE
feeds: spo_gtfs
stops: 654 (148 linked to the streets)
trip patterns: 36
services: 6
frequency-based trips: yes
service dates: 2008-01-01 to 2020-05-01 (4505 dates with service)
warnings: 0
```

A travel-time matrix by public transport (`travel_time_matrix()`): origins
and destinations are the same points unless `--destinations` is given.

```console
$ peabiru matrix r/inst/extdata/spo --origins r/inst/extdata/spo/spo_points.csv \
    --mode WALK,TRANSIT --departure 2019-05-13T14:00 --percentiles 25,50,75 --seed 42
peabiru matrix: network built: 2609 street vertices, 654 stops and 36 trip patterns from 1 feed (0.04 s)
from_id,to_id,travel_time_p25,travel_time_p50,travel_time_p75
89a8100c00fffff,89a8100c00fffff,0,0,0
89a8100c00fffff,89a8100c007ffff,7,7,7
89a8100c00fffff,89a8100c077ffff,10,10,10
...
peabiru matrix: 27 x 27 pairs, 729 rows (0.09 s)
```

Cumulative accessibility to jobs within 30 and 60 minutes
(`accessibility()`):

```console
$ peabiru accessibility r/inst/extdata/spo --origins r/inst/extdata/spo/spo_points.csv \
    --opportunities jobs --mode WALK,TRANSIT --departure 2019-05-13T14:00 \
    --cutoffs 30,60 --seed 42 -q
id,opportunity,percentile,cutoff,accessibility
89a8100c00fffff,jobs,50,30,95249
89a8100c00fffff,jobs,50,60,110736
89a8100c007ffff,jobs,50,30,110282
...
```

Walking isochrones as GeoJSON (`isochrone()`; `--lines` for the street
segments reached instead of polygons):

```console
$ peabiru isochrone r/inst/extdata/spo --origins one.csv --mode WALK --cutoffs 10,20 -q -o walk.geojson
```

The other commands:

| Command | What it writes | R function |
|---|---|---|
| `info` (`build`) | the network's size, feeds, elevation, warnings; `--service-dates`, `--dates`, `--start-date`/`--end-date`; `--gtfs-errors` | `build_network()`, `check_transit_availability()`, `get_gtfs_errors()` |
| `matrix` | travel times by a street mode or public transport | `travel_time_matrix()` |
| `expanded` | every trip by departure minute and draw (`--breakdown`) | `expanded_travel_time_matrix()` |
| `arrival` | the latest departure arriving by `--arrival` | `arrival_travel_time_matrix()` |
| `accessibility` | opportunities reached, with five decay functions | `accessibility()` |
| `isochrone` | polygons or street segments by cutoff | `isochrone()` |
| `itineraries` | trips leg by leg, with their lines; alternatives with `--shortest-path false` | `detailed_itineraries()` |
| `pareto` | travel time against fare at each of `--fare-cutoffs` | `pareto_frontier()` |
| `options` | the trips not dominated on time, fare, transfers and walking (`--legs`, `--departures`) | `trip_options()` |
| `snap` | where points meet the streets | `find_snap()` |
| `network` | `--layer edges`, `vertices`, `stops`, `routes` or `bbox` | `street_network_to_sf()`, `transit_network_to_sf()`, `street_network_bbox()` |
| `fares setup`, `fares read`, `fares write` | fare structures (`.zip`, the format of r5r's `write_fare_structure()`) | `setup_fare_structure()`, `read_fare_structure()`, `write_fare_structure()` |
| `completions` | shell completions (bash, zsh, fish, elvish, PowerShell) | |

Every command has `--help` with its options and an example, for instance:

```console
$ peabiru fares setup r/inst/extdata/spo --base-fare 4.4 -o fares.zip
$ peabiru pareto data/poa --origins o.csv --destinations d.csv \
    --departure 2019-05-13T14:00 --fare-structure fares.zip --fare-cutoffs 0,4.8,9.6
$ peabiru matrix data/poa --origins points.csv --mode CAR \
    --new-carspeeds congestion.csv --carspeed-scale 0.8
```

Scenarios (`--new-carspeeds`, `--carspeed-scale`, `--new-lts`) take CSV
tables (`osm_id,max_speed,speed_type`; `osm_id,lts`) or GeoJSON polygons
(`poly_id`, `scale`, `priority`) and lines (`line_id`, `lts`, `priority`),
as r5r's arguments of the same names.

Differences from the R package: the departure is a date and clock time
without a time zone, read as the network's local time; without
`--departure`, street-only commands use the current time in UTC (r5r uses
the current local time; it only shows in the itineraries'
`departure_time`); polygons and lines are written as built, without the
`sf::st_make_valid()` the R package applies (as the Python and JavaScript
packages return them).

## Licence

MIT AND Apache-2.0 AND BSD-3-Clause: see `LICENSE-MIT`,
`LICENSE-THIRD-PARTY` (the works the engine derives from) and
`LICENSE-RUST-CRATES` (the crates compiled into the binary).
