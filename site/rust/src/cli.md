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

## Diagnostics

```sh
peabiru --version
peabiru osm-stats <file.osm.pbf> [n_threads]   # what the PBF reader sees
peabiru dump-edges <file.osm.pbf>              # every directed edge, as CSV
```

`dump-edges` writes one row per directed edge with its permissions, speeds
and traffic stress, for comparing two networks edge by edge (see
[Parity with r5r](parity.md)).

Each command prints the build and routing times on standard output.
