# peabiru

[![CI](https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml/badge.svg)](https://github.com/StrategicProjects/peabiru-routing/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/StrategicProjects/peabiru-routing/blob/main/LICENSE-MIT)
[![Lifecycle: experimental](https://img.shields.io/badge/lifecycle-experimental-orange.svg)](https://lifecycle.r-lib.org/articles/stages.html#experimental)
[![Python 3.9+](https://img.shields.io/badge/python-3.9%2B-blue.svg)](#install)
[![PyPI](https://img.shields.io/pypi/v/peabiru.svg)](https://pypi.org/project/peabiru/)

peabiru computes travel-time matrices and accessibility on multimodal
transport networks (walking, cycling, driving and public transport) built
from an [OpenStreetMap](https://www.openstreetmap.org) extract and
[GTFS](https://gtfs.org) feeds.

The routing engine is written in Rust and runs inside the Python process:
no Java is needed, the network is built once in memory and reused by every
call, and results come back as `pandas.DataFrame`s. The same Rust core
powers the R package of the same name, with the same functions and
arguments. Coming from r5r or r5py? See
[Coming from r5py / r5r](coming-from-r5.md).

| Function | What it computes |
|---|---|
| [`build_network()`](reference.md#peabiru.build_network) | a network from a directory with one `.pbf` and any GTFS `.zip` |
| [`travel_time_matrix()`](travel-time-matrices.md) | travel times between every origin and destination |
| [`accessibility()`](accessibility.md) | cumulative or decay-weighted access to opportunities |
| [`expanded_travel_time_matrix()`](expanded-arrival.md) | travel times minute by minute, with a breakdown of each trip |
| [`arrival_travel_time_matrix()`](expanded-arrival.md#arrival-matrices) | the latest trip that arrives by a given time |
| [`isochrone()`](example-isochrones.md) | the areas or street segments reached within each cutoff |
| [`detailed_itineraries()`](itineraries.md) | the legs of each trip, with their lines |
| [`setup_fare_structure()`](fares.md), [`pareto_frontier()`](fares.md#the-pareto-frontier) | transit fares, and the trade-off between travel time and fare |
| [`find_snap()`](network-inspection.md), [`street_network_to_sf()`](network-inspection.md#streets), ... | where points snap, the streets, stops and routes of a network |
| [`get_gtfs_errors()`](gtfs-errors.md) | the errors of the GTFS feeds |

## Install

peabiru needs Python 3.9 or later; its only required dependency is
[pandas](https://pandas.pydata.org). Install
[geopandas](https://geopandas.org) as well to pass points as a
`GeoDataFrame`.

```sh
pip install peabiru
pip install "peabiru[geo]"     # with geopandas
```

Wheels are built for Linux with glibc (x86_64 and aarch64), macOS (arm64
and x86_64) and Windows (x64). On Linux with musl (Alpine) and on Windows on
Arm, pip builds the package from the source distribution, which needs a Rust
toolchain (rustc 1.81 or later).

From a clone of the repository, with a Rust toolchain and
[maturin](https://www.maturin.rs):

```sh
cd python
maturin develop --uv --release
```

## Get started

peabiru ships a small network for examples and tests: the streets around
Avenida Paulista, in Sao Paulo, with the frequency-based GTFS feed of
SPTrans, cut from the sample data of the R package r5r. Build a network
from its directory:

```python
import peabiru

network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
print(network.info())
```

```text
{'vertices': 2609, 'edges': 5518, 'elevation': 'NONE', 'feeds': ['spo_gtfs'], 'stops': 654, 'patterns': 36, 'services': 6, 'frequencies': True}
```

`print(network)` shows the same summary, with the data path.

The network lives in memory and is reused by every routing call. Origins and
destinations are `pandas.DataFrame`s with columns `id`, `lon` and `lat` (in
WGS 84); the sample comes with 27 points of a hexagonal grid:

```python
points = peabiru.sample_points()
print(points.head())
```

```text
                id        lon        lat  population  schools  jobs  healthcare
0  89a8100c00fffff -46.628965 -23.556312        3648        0   153           0
1  89a8100c007ffff -46.632067 -23.557703        3936        0   551           0
2  89a8100c077ffff -46.628874 -23.553245        6514        1  1124           0
3  89a8100c03bffff -46.631976 -23.554637        3670        0  7112           0
4  89a8100c033ffff -46.635078 -23.556029         893        0  3298           0
```

Walking times between all of them:

```python
ttm = peabiru.travel_time_matrix(network, points, points, mode="WALK", n_threads=2)
print(ttm.head())
print(len(ttm), "pairs")
```

```text
           from_id            to_id  travel_time_p50
0  89a8100c00fffff  89a8100c00fffff                0
1  89a8100c00fffff  89a8100c007ffff                7
2  89a8100c00fffff  89a8100c077ffff               10
3  89a8100c00fffff  89a8100c03bffff                8
4  89a8100c00fffff  89a8100c033ffff               13
729 pairs
```

By public transport, leaving at 14:00 on a Monday and over a 10-minute
window of departure times, the 25th, 50th and 75th percentiles of the travel
time. The sample feed is frequency-based, so the departures are drawn at
random: give a `seed` to make the result reproducible.

```python
import datetime as dt

ttm = peabiru.travel_time_matrix(
    network, points, points,
    mode=["WALK", "TRANSIT"],
    departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    percentiles=[25, 50, 75],
    seed=42,
    n_threads=2,
)
print(ttm.iloc[20:25].to_string())
```

```text
            from_id            to_id  travel_time_p25  travel_time_p50  travel_time_p75
20  89a8100c00fffff  89a8100c153ffff               31               32               33
21  89a8100c00fffff  89a8100c38fffff               27               28               29
22  89a8100c00fffff  89a8100c387ffff               21               21               22
23  89a8100c00fffff  89a8100c14bffff               29               29               30
24  89a8100c00fffff  89a8100c143ffff               28               29               29
```

And the number of jobs reachable within 15 and 30 minutes:

```python
access = peabiru.accessibility(
    network, points, points,
    opportunities_colnames="jobs",
    mode=["WALK", "TRANSIT"],
    departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    cutoffs=[15, 30],
    seed=42,
    n_threads=2,
)
print(access.head())
```

```text
                id opportunity  percentile  cutoff  accessibility
0  89a8100c00fffff        jobs          50      15        12452.0
1  89a8100c00fffff        jobs          50      30        95249.0
2  89a8100c007ffff        jobs          50      15        13914.0
3  89a8100c007ffff        jobs          50      30       110282.0
4  89a8100c077ffff        jobs          50      15        13099.0
```

## What peabiru refuses

A request peabiru cannot honour exactly is an error, never silently
ignored: R5's own fare calculators, expanded or arrival matrices by a street
mode alone and `pareto_frontier()` with `output_dir` raise
[`UnsupportedError`](reference.md#peabiru.UnsupportedError); some elevation
rasters, more than one `.pbf` file and a few itinerary pairs are errors too.
The full list, with every other deliberate difference from r5r, is under
[Coming from r5py / r5r](coming-from-r5.md#deliberate-differences).

## Sample data

The sample network in `peabiru.sample_data_path()` is cut from the Sao Paulo
sample data of r5r. `spo_osm.pbf` contains map data &copy; OpenStreetMap
contributors, available under the
[Open Database License](https://www.openstreetmap.org/copyright);
`spo_gtfs.zip` is the SPTrans GTFS feed shipped with r5r, without
`shapes.txt`.
