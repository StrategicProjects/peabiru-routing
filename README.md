<h1 align="center"><img src="site/assets/logo.svg" alt="peabiru" width="252" height="64"></h1>

<p align="center">
<a href="https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml"><img src="https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
<a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
<a href="https://lifecycle.r-lib.org/articles/stages.html#experimental"><img src="https://img.shields.io/badge/lifecycle-experimental-orange.svg" alt="Lifecycle: experimental"></a>
<a href="crates/peabiru-core"><img src="https://img.shields.io/badge/rustc-1.81%2B-blue.svg" alt="rustc 1.81+"></a>
<a href="python"><img src="https://img.shields.io/badge/python-3.9%2B-blue.svg" alt="Python 3.9+"></a>
<a href="r"><img src="https://img.shields.io/badge/R-%3E%3D%204.2-blue.svg" alt="R >= 4.2"></a>
<a href="js"><img src="https://img.shields.io/badge/node-18%2B-blue.svg" alt="Node 18+"></a>
<a href="https://crates.io/crates/peabiru-core"><img src="https://img.shields.io/crates/v/peabiru-core.svg" alt="crates.io"></a>
<a href="https://docs.rs/peabiru-core"><img src="https://docs.rs/peabiru-core/badge.svg" alt="docs.rs"></a>
<a href="https://doi.org/10.5281/zenodo.24125477"><img src="https://zenodo.org/badge/DOI/10.5281/zenodo.24125477.svg" alt="DOI"></a>
<a href="https://pypi.org/project/peabiru/"><img src="https://img.shields.io/pypi/v/peabiru.svg" alt="PyPI"></a>
<a href="https://www.npmjs.com/package/peabiru"><img src="https://img.shields.io/npm/v/peabiru.svg" alt="npm"></a>
<!-- After the first release on CRAN, uncomment:
<a href="https://CRAN.R-project.org/package=peabiru"><img src="https://www.r-pkg.org/badges/version/peabiru" alt="CRAN"></a>
-->
</p>

A multimodal routing engine written in Rust, for R, Python, JavaScript, Rust
and the shell. It builds a network in memory from an OpenStreetMap extract,
GTFS feeds and, optionally, an elevation raster, and computes travel-time
matrices, accessibility, isochrones, detailed itineraries and fares by
walking, cycling, driving and public transport. It is a Rust port of Conveyal's [R5](https://github.com/conveyal/r5) routing engine.
It needs no Java runtime,
keeps nothing on disk, and the network is built once and shared by every
query and thread.

> **Authorship.** peabiru is an independent project. Its routing model is that of
> Conveyal's [R5](https://github.com/conveyal/r5), and its fare calculator and parts of its trip
> planner are translated from [r5r](https://github.com/ipea/r5r) (Ipea); both are
> MIT-licensed, and their notices are kept in `LICENSE-THIRD-PARTY`. peabiru is not
> affiliated with or endorsed by Conveyal, Ipea or the r5r team. The port was written
> with the assistance of AI coding tools.

| Package | For | Directory | Registry |
|---|---|---|---|
| `peabiru` | R | [`r/`](r) (extendr) | CRAN, not published yet |
| `peabiru` | Python | [`python/`](python) (PyO3, maturin) | [PyPI](https://pypi.org/project/peabiru/) |
| `peabiru` | JavaScript (browsers, Node) | [`js/`](js) (WebAssembly, wasm-bindgen) | [npm](https://www.npmjs.com/package/peabiru) |
| `peabiru-core` | Rust | [`crates/peabiru-core`](crates/peabiru-core) | [crates.io](https://crates.io/crates/peabiru-core) |
| `peabiru` | command line | [`crates/peabiru-cli`](crates/peabiru-cli) | [crates.io](https://crates.io/crates/peabiru-cli) |

The name comes from the Peabiru, the pre-Columbian network of trails that
crossed South America from the Atlantic to the Andes.

<p align="center"><a href="https://strategicprojects.github.io/peabiru-routing/#gallery"><img src="site/assets/figures/isochrones-rmr.svg" width="520" alt="Four maps of central Recife, one per mode, with the areas reached from Praça do Derby within 15, 30 and 45 minutes walking, cycling, driving and by public transport."></a><br>
<sub>Isochrones from Praça do Derby, Recife, by mode, on the network of the metropolitan region with its buses, metro and VLT (<a href="https://strategicprojects.github.io/peabiru-routing/#gallery">gallery of examples</a>; <a href="https://strategicprojects.github.io/peabiru-routing/demo/">try it in the browser</a>).</sub></p>

![Architecture: one OpenStreetMap .pbf and zero or more GTFS .zip feeds go into peabiru-core, a Rust crate that builds the street network and the transit layer in memory and computes every result; the R package, the Python package, the JavaScript package (WebAssembly) and the command-line tool are thin front-ends over it.](site/assets/diagrams/architecture.svg)

## What it computes

- **Travel-time matrices** between any number of origins and destinations:
  on foot, by bicycle and by car on the street network, and by walking plus
  public transport over a departure window, with percentiles of the travel
  time across the window.
- **Accessibility**: the opportunities reachable from each origin, with
  step, exponential, fixed exponential, linear and logistic decay functions
  and several cutoffs at once.
- **Expanded matrices**: the travel time of every departure minute, with the
  routes, the waiting, riding and transfer times of each trip.
- **Arrival-time matrices**: the latest trips that arrive by a given time.
- **Isochrones**: the area reached within each cutoff, as polygons traced
  from a travel-time surface or as the street segments reached.
- **Detailed itineraries**: the legs of the fastest trip, or of the
  alternatives (`shortest_path = FALSE`), with their lines, durations,
  waits, distances and routes.
- **Fares**: fare structures (`setup_fare_structure()`,
  `read_fare_structure()`, `write_fare_structure()`) in the matrices,
  accessibility and itineraries, travel times within a budget
  (`max_fare`), and the trade-off between time and money
  (`pareto_frontier()`).
- **Where to build** (since 0.1.5):
  `locate_facilities()` sites new schools, health units or any facility by
  maximal covering, set covering, p-median or p-center on the network's
  travel times, counting the units that already exist, and
  `floating_catchment()` measures where supply is short for the demand
  around it. These are peabiru's own, not part of R5 or r5r.

Public transport can be any subset of the transit modes (`"BUS"`,
`"RAIL"`, ...), reached by walking, bicycle or car and left the same ways.
Walking and cycling times can follow the slope of the streets, from an
elevation raster (Tobler or Minetti). Scenarios change the car speeds
(`new_carspeeds`, `carspeed_scale`) and the cyclists' levels of traffic
stress (`new_lts`) for one call, without rebuilding the network. The
network can be inspected: where points snap to the streets, its bounding
box, its streets, stops and routes, the dates with service, and the GTFS
errors of each feed; feeds with errors of priority `HIGH` are refused.
Results can be written to CSV files, one per origin (`output_dir`, in R and
Python).

What peabiru does not support is an error that says so; nothing is
silently ignored. A fare structure that names a fare calculator (a `type`)
instead of fare rules is one such case.

**Status:** early development. The interfaces may still change.

## How it works

**Building the network.** One `.pbf` becomes a street network with one edge
per direction, the modes allowed on it, car speeds and the cyclists' level of
traffic stress; the GTFS feeds become patterns of trips, each stop is linked
to the nearest walkable street, and walking tables for transfers and egress
are computed once.

![Network build in eight steps: read the extract; ways to edges with permissions, speeds and traffic stress; clean up; index, park-and-ride and turn restrictions; read the GTFS feeds; trips to patterns; link stops to streets; walking tables for transfers (1 km) and egress (2 km).](site/assets/diagrams/network-build.svg)

**Street routing.** Each point is linked to the nearest street open to the
mode, and a search over directed edges, cheapest first, runs from every
origin up to the time limit, with walking and cycling speeds, traffic stress,
car speeds, turn costs and turn restrictions.

![Street routing: link each point to the nearest street open to the mode; search directed edges from the origin, earliest arrival first, up to the time limit; reach each destination through either end of its edge.](site/assets/diagrams/street-routing.svg)

**Public transport.** For each origin, a walk reaches the nearby stops; RAPTOR
then runs for every minute of the departure window, from the last minute back
to the first so that each minute reuses the later one's results, with Monte
Carlo draws for frequency-based trips; a walk from the stops reaches each
destination, and the times of all the iterations are summarised as
percentiles.

![Transit travel-time matrix: walk from the origin; RAPTOR over each departure minute, latest first, with Monte Carlo draws for frequency-based trips; walk from stops to each destination; percentiles of the iteration times.](site/assets/diagrams/transit-matrix.svg)

<details>
<summary><b>Accessibility</b>: opportunities weighted by a decay function of the travel time</summary>

![Accessibility is the sum over destinations of opportunities times a decay weight of the travel-time percentile in seconds; the five decay functions are drawn for a 30 minute cutoff.](site/assets/diagrams/accessibility.svg)

</details>

<details>
<summary><b>Expanded and arrival matrices</b>: every departure minute, and the latest trip that arrives in time</summary>

![Expanded matrix: one row per departure minute and draw with the trip breakdown. Arrival matrix: departures scanned from the latest minute back, keeping the first trip that arrives in time.](site/assets/diagrams/expanded-arrival.svg)

</details>

## Example

In R:

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

In Python:

```python
import datetime as dt
import peabiru

network = peabiru.build_network(peabiru.sample_data_path())
points = peabiru.sample_points()

ttm = peabiru.travel_time_matrix(
    network, points, points,
    mode=["WALK", "TRANSIT"],
    departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    percentiles=[25, 50, 75],
    seed=42,
)
```

In JavaScript (Node; in a browser, the bytes come from `fetch()`):

<!-- example: site/js/examples/landing.mjs:landing (checked by site/js/run_examples.mjs) -->
```js
// npm install peabiru
import { readFile } from "node:fs/promises";
import * as peabiru from "peabiru";

// the sample network of the Python package (python/peabiru/data/spo)
const network = await peabiru.buildNetwork({
  pbf: await readFile("spo_osm.pbf"),
  gtfs: [{ name: "spo_gtfs.zip", data: await readFile("spo_gtfs.zip") }],
});
const points = {
  id: ["a", "b", "c"],
  lon: [-46.6290, -46.6321, -46.6355],
  lat: [-23.5563, -23.5577, -23.5601],
};

const ttm = await peabiru.travelTimeMatrix(network, points, points, {
  mode: ["WALK", "TRANSIT"],
  departureDatetime: "2019-05-13 14:00:00",
  percentiles: [25, 50, 75],
  seed: 42,
});
```

All three use the small network of São Paulo that ships with the R and
Python packages. More in the README of each package ([R](r/README.md),
[Python](python/README.md), [JavaScript](js/README.md),
[Rust](crates/peabiru-core/README.md)) and in the documentation sites, built
by [`site/build.sh`](site/README.md).

## Installing

The Python, JavaScript and Rust packages are on their registries:

```sh
pip install peabiru
npm install peabiru
cargo add peabiru-core
```

The R package is not on CRAN yet. From a clone, with Rust (rustc 1.81 or
later):

```sh
R CMD INSTALL r                                     # R package
cd python && maturin develop --uv --release         # Python package
cd js && sh build.sh                                # JavaScript package (needs wasm-pack)
cargo build --release -p peabiru-cli                # command-line tool
```

## Correctness

The routing model follows that of Conveyal's
[R5](https://github.com/conveyal/r5). The repository keeps reference results for every
function on the sample networks of Porto Alegre and São Paulo, and
[`bench/compare`](bench/compare) checks peabiru against them value by value,
on Linux, macOS and Windows, on every push to `main`. Each package has one
page comparing it with r5r:
[R](https://strategicprojects.github.io/peabiru-routing/r/articles/coming-from-r5r.html),
[Python](https://strategicprojects.github.io/peabiru-routing/python/coming-from-r5/),
[JavaScript](https://strategicprojects.github.io/peabiru-routing/js/differences.html),
[Rust](https://strategicprojects.github.io/peabiru-routing/rust/parity.html).

## Development

```sh
cargo test                                          # core and command-line tool
cd python && maturin develop --uv && .venv/bin/python -m pytest
Rscript -e 'devtools::test("r")'
cd js && sh build.sh && node --test tests/*.test.mjs
node site/js/run_examples.mjs                       # the examples of the JavaScript guide
site/build.sh                                       # documentation sites, into _site/
```

### Continuous integration

- [`ci.yml`](.github/workflows/ci.yml), on every push and pull request: the
  Rust core and command-line tool on Linux, macOS and Windows, the oldest
  supported Rust, the R package, the Python wheel and the npm package.
- [`r5r-parity.yml`](.github/workflows/r5r-parity.yml), on every push to
  `main` (and by hand): the comparisons of [`bench/compare`](bench/compare),
  which run r5r 2.5.0 (Java 21 and the R5 jar) next to peabiru on the same
  runner, on Linux, Windows and macOS, and fail when a comparison differs or
  is skipped. Linux and Windows matter most: there the isochrone rings are
  ordered as libstdc++ orders them, differently from macOS, and the file
  system lists the GTFS feeds of a directory in another order (the feeds
  load in that order).
- `python-release.yml` and `npm-release.yml` build, test and publish the
  releases.

## Licence and credits

peabiru is distributed under the MIT licence ([`LICENSE-MIT`](LICENSE-MIT)).
To cite it, see [`CITATION.cff`](CITATION.cff).

Its routing logic is ported from [R5](https://github.com/conveyal/r5) (MIT, © Conveyal).
The fare calculator and parts of the trip planner are translated from
the Java code of [r5r](https://github.com/ipea/r5r) (MIT, © r5r contributors;
developed at Ipea), and many functions share their names, arguments and
result layouts with r5r's.
Polygon isochrones
port the algorithm of isoband (MIT), and parts of Apache Commons Math 3
(Apache-2.0) and fdlibm 5.3 are translated, so that the numbers are the
same on every platform.
The notices and licence texts of these and of the other works peabiru
derives from are in [`LICENSE-THIRD-PARTY`](LICENSE-THIRD-PARTY), which is
distributed with every package. The sample network in the R and Python
packages contains map data © OpenStreetMap contributors, under the
[Open Database License](https://www.openstreetmap.org/copyright).

The Python extension module, the R package's library, the WebAssembly module
of the JavaScript package and the command-line tool also contain code from
other Rust crates (pyo3, extendr, miniz_oxide and
others, under MIT, Apache-2.0 and similar licences). Their licences are in a
`LICENSE-RUST-CRATES` file shipped with each of those packages, generated
from the lockfiles by `tools/third_party_notices.py`.
