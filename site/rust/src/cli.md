# The command line

The `peabiru` binary (crate `peabiru-cli`) runs the engine on CSV files. It
is mostly a driver for benchmarks and parity checks, so its interface is
plain and may change.

```sh
cargo build --release -p peabiru-cli
# the binary is target/release/peabiru (or under your shared target dir)
```

Points files are CSV with a header and at least `id`, `lon` and `lat`
columns. Unless said otherwise, the same points are the origins and the
destinations. Outputs are CSV in long format, one row per pair (or per
origin), with the columns of the R and Python packages.

## Street matrix

```sh
peabiru ttm <network dir | file.osm.pbf> <points.csv> <WALK|BICYCLE|CAR> <out.csv> [n_threads] [repeats]
```

Writes `from_id,to_id,travel_time_p50` for the pairs that have a time.

## Public transport matrix

```sh
peabiru transit <dir> <points.csv> <out.csv> --date YYYY-MM-DD --time HH:MM:SS \
    [--tw 10] [--percentiles 50] [--max-walk 120] [--max-trip 120] \
    [--max-rides 3] [--draws 5] [--seed N] [--threads 0] [--repeats 1]
```

## Accessibility

```sh
peabiru accessibility <dir | file.osm.pbf> <points.csv> <out.csv> --opportunities jobs,schools \
    [--mode WALK,TRANSIT | WALK | BICYCLE | CAR] \
    [--decay step|exponential|fixed_exponential|linear|logistic] [--decay-value V] \
    [--cutoffs 30,60] [--percentiles 50] [--date YYYY-MM-DD] [--time HH:MM:SS] \
    [--tw 10] [--max-walk N|Inf] [--max-trip 120] [--max-rides 3] [--draws 5] \
    [--seed N] [--walk-speed 3.6] [--bike-speed 12] [--max-lts 2] \
    [--destinations other.csv] [--threads 0] [--repeats 1]
```

The opportunity columns are read from the points file (or from
`--destinations`). Arguments are checked, and trip limits derived, as in the
R and Python packages.

## Expanded and arrival matrices

```sh
peabiru expanded <dir> <points.csv> <out.csv> --date YYYY-MM-DD --time HH:MM:SS \
    [--arrival] [--breakdown] [--origins N] [--tw 10] [--max-walk 120] \
    [--max-trip 120] [--max-rides 3] [--draws 5] [--seed N] [--threads 0]
```

With `--arrival`, `--date` and `--time` are the arrival. `--origins N` routes
only from the first N points.

## Isochrones

```sh
peabiru isochrone <dir | file.osm.pbf> <points.csv> <out> \
    [--mode WALK|BICYCLE|CAR|TRANSIT] [--cutoffs 0,15,30] [--zoom 10] [--lines] \
    [--percentiles 50] [--surface surface.csv] [--edges edges.csv] \
    [--date YYYY-MM-DD --time HH:MM:SS] [--tw 10] [--max-walk N|inf] [--max-rides 3] \
    [--draws 5] [--seed N] [--walk-speed 3.6] [--bike-speed 12] [--max-lts 2] \
    [--threads 0] [--cxx-library libc++|libstdc++]
```

Writes one tab-separated row per origin, cutoff and percentile with the
polygons as WKT (`--lines`: the street segments reached instead).
`--date` and `--time` are required with `TRANSIT`. `--cxx-library` orders
the rings as isoband built with that C++ library would (see
[Parity with r5r](parity.md)).

## Detailed itineraries

```sh
peabiru itineraries <dir> <origins.csv> <destinations.csv> <out.csv> \
    [--mode WALK|BICYCLE|CAR] [--transit TRANSIT|BUS,RAIL,...] [--egress WALK] \
    [--date YYYY-MM-DD] [--time HH:MM:SS] [--tw 10] [--max-walk 120] [--max-bike 120] \
    [--max-car 120] [--max-trip 120] [--walk-speed 3.6] [--bike-speed 12] \
    [--max-rides 3] [--max-lts 2] [--threads 0]
```

Routes origin `i` to destination `i` and writes one row per leg, with its
line as WKT. The command has no options for fares or alternatives; the
crate's `detailed_itineraries` has them.

## Environment variables

| Variable | Effect |
|---|---|
| `PEABIRU_ELEVATION=TOBLER\|MINETTI\|NONE` | slope costs from the directory's `.tif` raster; without it rasters are ignored (r5r's default is `TOBLER`) |
| `PEABIRU_IGNORE_GTFS_ERRORS=1` | build from feeds with errors of priority `HIGH`, with a warning per feed, instead of failing |
| `PEABIRU_SCENARIO=<file>` | a scenario file: lines `carspeed_scale <x>`, `car scale` or `car km/h`, `way <osm id> <value>` and `lts <osm id> <level>` (polygons and lines are not read) |
| `PEABIRU_STREET_MATRIX=dijkstra\|ch\|auto` | the engine of street matrices (`ttm`); the matrix is the same |

## Diagnostics

```sh
peabiru --version
peabiru osm-stats <file.osm.pbf> [n_threads]          # what the PBF reader sees
peabiru dump-edges <file.osm.pbf | dir>               # every directed edge, as CSV
peabiru dump-elevation <dir> <out prefix>             # elevation samples and slope factors
peabiru sample-raster <file.tif> <points> <out>       # R5's elevation reading at points
peabiru crs-transform <epsg code> <points> <out>      # GeoTools' transform of a raster CRS
```

`dump-edges` writes one row per directed edge with its permissions, speeds
and traffic stress, for comparing two networks edge by edge (see
[Parity with r5r](parity.md)); given a directory, it links the GTFS stops
first. The last three compare the elevation code with R5 and GeoTools
value by value; their point files hold the hex bits of the doubles.

Each command prints the build and routing times on standard output.
