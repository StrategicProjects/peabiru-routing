# peabiru <img src="man/figures/logo.png" align="right" height="139" alt="peabiru hexagon logo" />

<!-- badges: start -->
[![CI](https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml/badge.svg)](https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/StrategicProjects/peabiru-routing/blob/main/LICENSE-MIT)
[![Lifecycle: experimental](https://img.shields.io/badge/lifecycle-experimental-orange.svg)](https://lifecycle.r-lib.org/articles/stages.html#experimental)
[![R >= 4.2](https://img.shields.io/badge/R-%3E%3D%204.2-blue.svg)](https://github.com/StrategicProjects/peabiru-routing/tree/main/r)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.24125477.svg)](https://doi.org/10.5281/zenodo.24125477)
<!-- After the first release on CRAN, uncomment:
[![CRAN status](https://www.r-pkg.org/badges/version/peabiru)](https://CRAN.R-project.org/package=peabiru)
-->
<!-- badges: end -->

Travel-time matrices and accessibility by walking, cycling, driving and
public transport, on networks built from an OpenStreetMap extract and GTFS
public transport feeds. The routing engine is written in Rust and runs inside
the R session: no Java is needed, and the network is built once, in memory,
and reused by every call. The same engine is behind the Python and
JavaScript packages and the command-line tool of the same name.

<a href="https://github.com/StrategicProjects/peabiru-routing/blob/main/r/vignettes/articles/isochrones.Rmd"><img src="https://raw.githubusercontent.com/StrategicProjects/peabiru-routing/main/site/assets/figures/isochrones-by-mode.svg" width="480" alt="Four maps of Porto Alegre, one per mode, with the areas reached from Farroupilha Park within 10, 20 and 30 minutes walking, cycling, driving and by public transport."></a>

| Function | What it computes |
|---|---|
| `build_network()` | a network from one `.pbf` and any number of GTFS feeds |
| `travel_time_matrix()` | travel times by `"WALK"`, `"BICYCLE"`, `"CAR"` or `c("WALK", "TRANSIT")`, with percentiles over a departure window |
| `accessibility()` | opportunities reachable from each origin, with five decay functions |
| `expanded_travel_time_matrix()` | the trip of every departure minute, with its breakdown |
| `arrival_travel_time_matrix()` | the latest trip that arrives by a given time |
| `isochrone()` | the areas or street segments reached within each cutoff |

Not implemented yet: detailed itineraries, public transport with
bicycle or car access, subsets of transit modes, fares, scenarios and
elevation. Asking for them stops with an error; nothing is silently ignored.

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

## Coming from r5r

The functions, arguments and defaults are those of
[r5r](https://github.com/ipea/r5r) 2.5.0, and so are the results: with the
same arguments and `seed`, r5r returns the same `data.frame` (`identical()`
after converting r5r's `data.table`). The article "Coming from r5r" lists
what is the same, what differs on purpose and what is not there yet.

## Documentation

The package website is built from this directory with pkgdown, together with
the rest of the project's documentation: run `site/build.sh` at the root of
the repository and open `_site/r/index.html`. It has a getting-started
vignette and articles on travel-time matrices, accessibility, expanded and
arrival matrices, coming from r5r, how the engine works, and benchmarks.

## Licence and credits

MIT. The routing logic is ported from R5 (MIT, © Conveyal), and r5r (Ipea)
defined the interface and supplies the reference results for the tests.
The notices of the works peabiru derives from are in
`inst/LICENSE-THIRD-PARTY`. The package's compiled code also contains Rust
crates by other authors (extendr, miniz_oxide and others): they are listed
in `inst/AUTHORS`, with their licence texts in `inst/LICENSE-RUST-CRATES`.
