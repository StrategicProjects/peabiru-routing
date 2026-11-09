# peabiru

A multimodal routing engine written in Rust. It builds a network from an
OpenStreetMap extract and GTFS feeds and computes travel-time matrices and
accessibility by walking, cycling, driving and public transport. The same
engine is behind an R package, a Python package, a Rust crate and a
command-line tool, and it needs no Java runtime.

| Package | For | Directory | Registry |
|---|---|---|---|
| `peabiru` | R | [`r/`](r) (extendr) | CRAN, not published yet |
| `peabiru` | Python | [`python/`](python) (PyO3, maturin) | PyPI, not published yet |
| `peabiru-core` | Rust | [`crates/peabiru-core`](crates/peabiru-core) | crates.io, not published yet |
| `peabiru` | command line | [`crates/peabiru-cli`](crates/peabiru-cli) | |

The name comes from the Peabiru, the pre-Columbian network of trails that
crossed South America from the Atlantic to the Andes.

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

Not implemented yet: isochrones, detailed itineraries, public transport
reached by bicycle or car, subsets of transit modes, fares, scenarios and
elevation. Asking for any of them is an error; nothing is silently ignored.

**Status:** version 0.0.1, early development. The interfaces may still
change.

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

Both use the small network of São Paulo that ships with the packages. More
in [`python/README.md`](python/README.md) and in the documentation of each
package.

## Installing from source

Nothing is published yet. From a clone, with Rust (rustc 1.81 or later):

```sh
R CMD INSTALL r                                     # R package
cd python && maturin develop --uv --release         # Python package
cargo build --release -p peabiru-cli                # command-line tool
```

## Compatibility with r5r

The functions of the R and Python packages have the names, arguments and
defaults of [r5r](https://github.com/ipeaGIT/r5r) 2.5.0, so code written for
r5r needs few changes, and they return the same values: the engine is a port
of Conveyal's [R5](https://github.com/conveyal/r5) 7.6, the engine r5r
drives. [`bench/`](bench) runs the same requests on both and compares the
outputs pair by pair on r5r's sample networks of Porto Alegre and São Paulo;
the comparisons are exact.

## Development

```sh
cargo test                                          # core and command-line tool
cd python && maturin develop --uv && .venv/bin/python -m pytest
Rscript -e 'devtools::test("r")'
```

## Licence and credits

peabiru is distributed under the MIT licence ([`LICENSE-MIT`](LICENSE-MIT)).
To cite it, see [`CITATION.cff`](CITATION.cff).

Its routing logic is ported from R5 (MIT, © Conveyal) and its interface comes
from r5r, developed at Ipea. Two source files are translations of parts of
Apache Commons Math 3 (Apache-2.0). The notices and licence texts of these
works are in [`LICENSE-THIRD-PARTY`](LICENSE-THIRD-PARTY), which is
distributed with every package. The sample network in the R and Python
packages contains map data © OpenStreetMap contributors, under the
[Open Database License](https://www.openstreetmap.org/copyright).
