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

Travel-time matrices, accessibility, isochrones and detailed itineraries by
walking, cycling, driving and public transport, with fares, slope costs and
scenarios, on networks built from an OpenStreetMap extract, GTFS public
transport feeds and an optional elevation raster. The routing engine is written in Rust and runs inside
the R session: no Java is needed, and the network is built once, in memory,
and reused by every call. The same engine is behind the Python and
JavaScript packages and the command-line tool of the same name.

<a href="https://github.com/StrategicProjects/peabiru-routing/blob/main/r/vignettes/articles/isochrones.Rmd"><img src="https://raw.githubusercontent.com/StrategicProjects/peabiru-routing/main/site/assets/figures/isochrones-by-mode.svg" width="480" alt="Four maps of Porto Alegre, one per mode, with the areas reached from Farroupilha Park within 10, 20 and 30 minutes walking, cycling, driving and by public transport."></a>

| Function | What it computes |
|---|---|
| `build_network()` | a network from one `.pbf`, any number of GTFS feeds and an optional elevation raster (`.tif`) |
| `travel_time_matrix()` | travel times by `"WALK"`, `"BICYCLE"`, `"CAR"` or public transport (`"TRANSIT"` or any of its modes, reached by walking, bicycle or car), with percentiles over a departure window |
| `accessibility()` | opportunities reachable from each origin, with five decay functions |
| `expanded_travel_time_matrix()` | the trip of every departure minute, with its breakdown |
| `arrival_travel_time_matrix()` | the latest trip that arrives by a given time |
| `isochrone()` | the areas or street segments reached within each cutoff |
| `detailed_itineraries()` | the legs of each trip, with their lines (the fastest trip, or the alternatives) |
| `setup_fare_structure()`, `read_fare_structure()`, `write_fare_structure()` | fare structures, which the routing functions take as `fare_structure` |
| `pareto_frontier()` | the trade-off between travel time and fare |
| `find_snap()`, `street_network_bbox()`, `street_network_to_sf()`, `transit_network_to_sf()` | where points snap, and the street and transit networks as `sf` |
| `check_transit_availability()`, `get_gtfs_errors()` | the dates with service, and the errors in the GTFS feeds |
| `setup_r5()`, `stop_r5()`, `download_r5()`, `r5r_cache()`, `r5r_sitrep()` | r5r's functions for Java, kept so that r5r scripts run |

The routing functions also take scenarios of car speeds (`new_carspeeds`,
`carspeed_scale`) and of traffic stress for cyclists (`new_lts`). What the
engine cannot compute exactly as R5 does, such as R5's own fare calculators
(a fare structure with a `type`), stops with an error; nothing is silently
ignored.

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

The package exports the 22 functions of [r5r](https://github.com/ipea/r5r)
2.5.0, with their arguments and defaults, and the results are r5r's: with
the same arguments and `seed`, r5r returns the same rows, columns and
values. peabiru returns tibbles by default;
`options(peabiru.output = "data.table")` gives r5r's `data.table`s,
`identical()` to r5r's results, class included, and
`options(peabiru.output = "data.frame")` base data frames. The article
"Coming from r5r" lists what is the same and every difference.

## Documentation

The package website is built from this directory with pkgdown, together with
the rest of the project's documentation: run `site/build.sh` at the root of
the repository and open `_site/r/index.html`. It has a getting-started
vignette and articles on travel-time matrices, accessibility, expanded and
arrival matrices, detailed itineraries, fares, elevation, scenarios,
inspecting a network, coming from r5r, how the engine works, and
benchmarks. To go from points to maps with sf, dplyr and ggplot2, start with
["peabiru with sf and the tidyverse"](https://github.com/StrategicProjects/peabiru-routing/blob/main/r/vignettes/articles/sf-tidyverse.Rmd):
an sf grid in any coordinate reference system, the routing functions with
the native pipe, dplyr on the tibbles they return, joins back to the
geometry and `geom_sf()` maps.

## Licence and credits

MIT. The routing logic is ported from R5 (MIT, © Conveyal), and r5r (Ipea)
defined the interface and supplies the reference results for the tests.
The notices of the works peabiru derives from are in
`inst/LICENSE-THIRD-PARTY`. The package's compiled code also contains Rust
crates by other authors (extendr, miniz_oxide and others): they are listed
in `inst/AUTHORS`, with their licence texts in `inst/LICENSE-RUST-CRATES`.
