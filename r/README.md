# peabiru <img src="man/figures/logo.png" align="right" height="139" alt="peabiru hexagon logo" />

Travel-time matrices and accessibility on transport networks built from an
OpenStreetMap extract and GTFS public transport feeds, with the functions,
arguments and **results** of [r5r](https://github.com/ipeaGIT/r5r) 2.5.0. The
routing engine is a port of Conveyal's [R5](https://github.com/conveyal/r5)
7.6 to Rust, shared with the Python package and the command-line tool of the
same name, so no Java is needed.

| r5r function | peabiru |
|---|---|
| `build_network()` | yes (one `.pbf`, any number of GTFS feeds) |
| `travel_time_matrix()` | yes: `"WALK"`, `"BICYCLE"`, `"CAR"`, `c("WALK", "TRANSIT")` |
| `accessibility()` | yes, all five decay functions |
| `expanded_travel_time_matrix()`, `arrival_travel_time_matrix()` | yes, walk + transit |
| `isochrone()`, `detailed_itineraries()`, `pareto_frontier()` | not yet |

Options that are not implemented yet (public transport with bicycle or car
access, subsets of transit modes, fares, scenarios, elevation) stop with an
error; nothing is silently ignored.

## Installation

Not on CRAN yet. From a clone of
[the repository](https://github.com/StrategicProjects/peabiru-routing), with
Rust (rustc >= 1.81) installed:

```sh
R CMD INSTALL r
```

## Example

```r
library(peabiru)

data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path)
points <- read.csv(file.path(data_path, "spo_points.csv"))

ttm <- travel_time_matrix(
  network, points, points,
  mode = c("WALK", "TRANSIT"),
  departure_datetime = as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo"),
  percentiles = c(25, 50, 75),
  seed = 42
)
```

With the same arguments and `seed`, r5r returns the same `data.frame`
(`identical()` after converting r5r's `data.table`).

## Documentation

The package website is built from this directory with pkgdown, together with
the rest of the project's documentation: run `site/build.sh` at the root of
the repository and open `_site/r/index.html`. It has a getting-started
vignette and articles on travel-time matrices, accessibility, expanded and
arrival matrices, migrating from r5r, how the engine works, and benchmarks.

## Licence and credits

MIT. The routing logic is ported from R5 (MIT, © Conveyal) as driven by r5r
(IPEA), whose results are the reference for every test.
