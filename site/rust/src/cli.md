# The command line

The `peabiru` binary (crate
[`peabiru-cli`](https://crates.io/crates/peabiru-cli) on crates.io) runs the engine from the shell: every function of the
R, Python and JavaScript packages is a command, with their argument names
in kebab-case, their defaults and their checks, so that a transport analyst
can work without R or Python and get the same numbers.

```sh
cargo install peabiru-cli            # Rust 1.81 or later
cargo build --release -p peabiru-cli # from the repository: target/release/peabiru
peabiru --help                       # the commands
peabiru matrix --help                # a command's options, with examples
```

## Inputs and outputs

- **Network**: a directory with one OpenStreetMap `.pbf` file, any number of
  GTFS `.zip` feeds and an optional `.tif` elevation raster, or a `.pbf` file with `--gtfs feed.zip`
  (repeatable) and `--tif raster.tif`. With a raster, walking and cycling
  have slope costs (`--elevation TOBLER`, the default, `MINETTI` or
  `NONE`). Feeds with GTFS errors of priority `HIGH` are refused
  (`--ignore-gtfs-errors` builds anyway, with a warning per
  feed). `--threads N` (0, the default, uses every core).
- **Points**: CSV with a header and the columns `id`, `lon`, `lat` (WGS 84),
  RFC 4180 quoting; `--origins` and `--destinations` (by default the
  origins). Points with a missing coordinate are not routed (a warning);
  duplicated ids are an error.
- **Results**: CSV on standard output or in `-o FILE`, with the columns of
  the R package's tables; missing values are empty fields. Geometries as
  GeoJSON (`--format geojson`) or as WKT in a CSV column. `--output-dir`
  writes one CSV file per origin (or pair), as the packages'
  `output_dir`. Progress and warnings on standard error (`-q` silences
  the progress).
- **Exit status**: 0 done, 1 a file or network that cannot be read or
  written, 2 a bad argument, 130 interrupted (Ctrl-C cancels the
  computation through the core's `CancelToken`).

## Commands

| Command | R function | Notes |
|---|---|---|
| `info` (alias `build`) | `build_network()`, `check_transit_availability()`, `get_gtfs_errors()` | a summary; `--service-dates`, `--dates d1,d2`, `--start-date`/`--end-date`; `--gtfs-errors` (without building) |
| `matrix` | `travel_time_matrix()` | `--mode WALK`, `BICYCLE`, `CAR`, or with `TRANSIT` (or `BUS,RAIL,...`); `--percentiles`, fares, scenarios, `--street-engine auto\|search\|hierarchy` |
| `expanded` | `expanded_travel_time_matrix()` | `--breakdown` |
| `arrival` | `arrival_travel_time_matrix()` | `--arrival DATETIME` |
| `accessibility` | `accessibility()` | `--opportunities`, `--decay-function`, `--decay-value`, `--cutoffs` |
| `isochrone` | `isochrone()` | polygons (`--zoom`, `--percentiles`) or `--lines`; GeoJSON or CSV |
| `itineraries` | `detailed_itineraries()` | `--all-to-all`, `--shortest-path false` (alternatives), `--suboptimal-minutes`, fares, `--osm-link-ids`, `--drop-geometry` |
| `pareto` | `pareto_frontier()` | `--fare-structure`, `--fare-cutoffs` |
| `options` | `trip_options()` | `--criteria`, `--max-options`, `--walk-resolution`; the options to `-o`, `--legs FILE`, `--departures FILE` |
| `locate` | `locate_facilities()` | `--demand`, `--candidates`, `--existing`, `--demand-column`, `--model max_cover\|set_cover\|p_median\|p_center`, `--n-facilities`, `--cutoff`, `--no-swap`; `--table sites\|candidates\|demand`; the summary on standard error |
| `catchment` | `floating_catchment()` | `--demand`, `--supply`, `--demand-column`, `--supply-column`, `--cutoff`, `--decay-function`, `--decay-value`; `--table demand\|supply` |
| `snap` | `find_snap()` | `--points`, `--radius`, `--mode` |
| `network` | `street_network_to_sf()`, `transit_network_to_sf()`, `street_network_bbox()` | `--layer edges\|vertices\|stops\|routes\|bbox` |
| `fares setup\|read\|write` | `setup_fare_structure()`, `read_fare_structure()`, `write_fare_structure()` | `.zip` files of five CSV tables; `read --output-dir` writes its CSV files, `write` makes the `.zip` from them |
| `completions SHELL` | | bash, zsh, fish, elvish, powershell |

Dates and times are `YYYY-MM-DDTHH:MM[:SS]`, the network's local time.
Limits take `Inf` where the packages do (`--max-walk-time Inf`, the
default, is the trip limit).

## Examples

```sh
# walking and public transport, three percentiles over a 10-minute window
peabiru matrix data/poa --origins points.csv --mode WALK,TRANSIT \
    --departure 2019-05-13T14:00 --percentiles 25,50,75 -o ttm.csv

# accessibility to jobs and schools, logistic decay, by bicycle
peabiru accessibility data/poa --origins points.csv --opportunities jobs,schools \
    --mode BICYCLE --decay-function logistic --decay-value 5 --cutoffs 30 -o access.csv

# isochrones by public transport as GeoJSON
peabiru isochrone data/poa --origins one.csv --departure 2019-05-13T14:00 \
    --cutoffs 15,30,45 -o isochrones.geojson

# a fare structure, then the Pareto frontier of time and fare
peabiru fares setup data/poa --base-fare 4.8 -o fares.zip
peabiru pareto data/poa --origins o.csv --destinations d.csv \
    --departure 2019-05-13T14:00 --fare-structure fares.zip --fare-cutoffs 0,4.8,9.6

# a congestion scenario
peabiru matrix data/poa --origins points.csv --mode CAR \
    --new-carspeeds poa_osm_congestion.csv --carspeed-scale 0.8
```

## Compared with the R package

`bench/compare/cli_parity.R` runs every command on the Porto Alegre
sample (timetables, fares, an elevation raster, scenarios) and on the São
Paulo sample (frequency-based routes) and compares each table with the R
function's, value for value (doubles bit for bit), lines and polygons by
their coordinates, and `--output-dir` files byte for byte. The deliberate
differences: without `--departure`, street-only commands take the current
time in UTC (the R package the local time; only the itineraries' `departure_time`
shows it), and polygons and lines are written as built, without the
`sf::st_make_valid()` of the R package (as Python and JavaScript return
them; a route through a single linked stop is a point). With
`--output-dir`, an id that cannot name a file in the directory (a path
separator, NUL; on Windows also `< > : " | ? *`) is an error, as in the
packages.

## For the repository's scripts

The benchmark and comparison scripts of the repository run older spellings
of the tool (`ttm`, `transit`, the positional forms of `accessibility`,
`expanded`, `isochrone`, `itineraries` and `options`, and diagnostic
commands such as `dump-edges`), also reachable as `peabiru dev <command>`.
They are not part of the tool's interface and may change at any time.
