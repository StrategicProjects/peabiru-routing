# peabiru-cli

The `peabiru` command-line tool: the routing engine of
[peabiru](https://github.com/StrategicProjects/peabiru-routing)
(`peabiru-core`) from the shell. It builds a network from an OpenStreetMap
`.pbf` extract and GTFS feeds and writes travel-time matrices, accessibility
and expanded (trip-by-trip) matrices as CSV files. It also serves as the
benchmark and comparison driver of the repository, so each command prints
the size of the network and the time it took on standard output.

For analysis work, the R and Python packages of the same project offer the
same computations with more options and checks.

**Status**: version 0.0.1, early development; not published yet.

## Building

From a checkout of the repository, with Rust 1.81 or later:

```sh
cargo build --release -p peabiru-cli     # the binary is target/release/peabiru
```

## Inputs

- **Network**: a directory with exactly one OpenStreetMap extract (`.pbf`)
  and any number of GTFS feeds (`.zip`), or, for the street-only commands,
  the `.pbf` file itself. Files are recognised by their lower-case
  extension only: `map.PBF` or `feed.ZIP` are ignored.
- **Points**: a CSV file with a header and the columns `id`, `lon` and
  `lat` (WGS 84), found by name; other columns are ignored, except the
  opportunity columns named in `accessibility`. The points are both the
  origins and the destinations.

The examples use the sample network of the repository, a small area of
São Paulo in `r/inst/extdata/spo` (map data © OpenStreetMap contributors,
under the Open Database License; see `LICENSE-THIRD-PARTY`), with 27
points in `spo_points.csv`.

## Commands

`peabiru --help` lists them with their options; `peabiru --version` prints
the version.

**`ttm`**: travel-time matrix by walking, cycling or driving.

```sh
peabiru ttm r/inst/extdata/spo/spo_osm.pbf r/inst/extdata/spo/spo_points.csv WALK walk.csv
```

writes `from_id,to_id,travel_time_p50` (minutes), one row per pair reached
(all 729 here).
Optional positional arguments: the number of threads (0, the default, uses
all cores) and the number of repetitions for timing.

**`transit`**: travel-time matrix by walking and public transport, over a
departure window.

```sh
peabiru transit r/inst/extdata/spo r/inst/extdata/spo/spo_points.csv transit.csv \
  --date 2019-05-13 --time 14:00:00 --seed 42
```

`--date` and `--time` are required. Options: `--tw` (time window, minutes,
default 10), `--percentiles` (comma-separated, default 50), `--max-walk`,
`--max-trip` (minutes, default 120), `--max-rides` (default 3), `--draws`
(per minute, default 5), `--seed`, `--threads`. The output has one
`travel_time_pNN` column per percentile and one row per pair reached.

**`accessibility`**: cumulative or decay-weighted opportunities reachable
from each point.

```sh
peabiru accessibility r/inst/extdata/spo r/inst/extdata/spo/spo_points.csv access.csv \
  --opportunities jobs --mode WALK,TRANSIT --cutoffs 30 --date 2019-05-13 --time 14:00:00
```

writes `id,opportunity,percentile,cutoff,accessibility`. Options also
include `--decay` (`step`, `exponential`, `fixed_exponential`, `linear`,
`logistic`) with `--decay-value`, `--walk-speed`, `--bike-speed`,
`--max-lts` and `--destinations other.csv`; the comment at the top of
`src/accessibility.rs` lists them all.

**`expanded`**: one row per origin, destination, departure minute and
draw, optionally with the breakdown of each trip; with `--arrival`, the
arrival-time matrix instead.

```sh
peabiru expanded r/inst/extdata/spo r/inst/extdata/spo/spo_points.csv expanded.csv \
  --date 2019-05-13 --time 14:00:00 --breakdown --origins 2
```

`--origins N` keeps the first N points as origins (here 2 origins × 27
destinations × 10 minutes × 5 draws = 2,700 rows).

**`osm-stats`** and **`dump-edges`**: diagnostics of an OpenStreetMap
extract. `peabiru osm-stats map.osm.pbf` prints counts of highway ways and
nodes; `peabiru dump-edges map.osm.pbf > edges.csv` writes every directed
edge of the street network with its length, permissions, car speed and
cycling level of traffic stress.

A bad argument ends the command with a one-line message and exit status 2;
a file that cannot be read or a network that cannot be built, with exit
status 1. A GTFS feed that r5r refuses to build a network from (tables in
a subfolder of the `.zip`, no `agency.txt`, trips that refer to a stop or
route that does not exist) is built, with a warning line on stderr.

## Licence and credits

MIT (`LICENSE-MIT`). The routing logic is ported from Conveyal's
[R5](https://github.com/conveyal/r5) 7.6 (MIT), and parts of the engine are
translations of Apache Commons Math 3.0 (Apache-2.0); their notices are in
`LICENSE-THIRD-PARTY`. The binary also contains the crates `miniz_oxide`
and `adler2`, whose licences are in `LICENSE-RUST-CRATES`.
