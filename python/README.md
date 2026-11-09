# peabiru (Python)

Multimodal routing on [OpenStreetMap](https://www.openstreetmap.org) street
networks and [GTFS](https://gtfs.org) public transport: travel-time
matrices, accessibility, and minute-by-minute (expanded) and arrival-time
matrices, by walking, cycling, driving and public transport.

The functions, their arguments and their results are those of the R package
[r5r](https://ipeagit.github.io/r5r/) 2.5.0, and so are the values: the
routing engine is a Rust port of [R5](https://github.com/conveyal/r5) (the
engine r5r drives, version 7.6) and is checked against r5r call for call,
with identical results, including the last bit of accessibility values. No
Java is needed. The same Rust core powers the R package of the same name.

```python
import datetime as dt
import peabiru

network = peabiru.build_network(peabiru.sample_data_path())
points = peabiru.sample_points()          # id, lon, lat, opportunities

ttm = peabiru.travel_time_matrix(
    network, points, points,
    mode=["WALK", "TRANSIT"],
    departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    percentiles=[25, 50, 75],
    seed=42,
)

access = peabiru.accessibility(
    network, points, points,
    opportunities_colnames="jobs",
    mode="BICYCLE",
    decay_function="logistic",
    cutoffs=[15, 30],
    decay_value=5,
)
```

| Function | r5r equivalent |
|---|---|
| `build_network(data_path)` | `build_network()` |
| `travel_time_matrix(...)` | `travel_time_matrix()` |
| `accessibility(...)` | `accessibility()` |
| `expanded_travel_time_matrix(...)` | `expanded_travel_time_matrix()` |
| `arrival_travel_time_matrix(...)` | `arrival_travel_time_matrix()` |

Points are `pandas.DataFrame`s with `id`, `lon` and `lat` (or, with
geopandas installed, a `GeoDataFrame` of points in EPSG:4326); results are
`pandas.DataFrame`s with r5r's columns, in r5r's row order. What peabiru
does not support yet (transit with bicycle or car access, fares, scenarios,
elevation, ...) raises `peabiru.UnsupportedError`; nothing is silently
ignored.

## Install

```sh
pip install peabiru            # once published on PyPI
pip install "peabiru[geo]"     # with geopandas
```

Requires Python 3.9 or later and pandas. From a clone of the repository
(needs a Rust toolchain):

```sh
cd python
maturin develop --uv --release
```

## Development

```sh
cd python
uv venv && uv pip install pandas pytest geopandas
maturin develop --uv
.venv/bin/python -m pytest                         # tests, doc examples, doctests
uv pip install "mkdocs>=1.6" "mkdocs-material>=9.5" "mkdocstrings[python]>=0.26"
.venv/bin/mkdocs build --strict                    # documentation site in site/
```

Parity with r5r (needs R with r5r 2.5.0 and a JDK 21), from the repository
root:

```sh
JAVA_HOME=/path/to/jdk21 python/.venv/bin/python bench/compare/py_parity.py poa spo
```

## Sample data

`peabiru.sample_data_path()` is a small network cut from r5r's Sao Paulo
sample data. Its `spo_osm.pbf` contains map data (c) OpenStreetMap
contributors, available under the
[Open Database License](https://www.openstreetmap.org/copyright).

## License

MIT. peabiru ports R5 (MIT, Conveyal) and parts of Apache Commons Math
(Apache-2.0).
