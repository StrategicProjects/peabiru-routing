# Coming from r5py / r5r

peabiru's reference is **r5r 2.5.0**, which drives R5 7.6: the functions,
argument names, defaults, checks, result columns, row order and values are
r5r's. Every function is compared with r5r itself, call for call, on r5r's
two sample cities (Porto Alegre and Sao Paulo), and the results are
identical, including the last bit of the accessibility values.

## From r5r

Calls translate one to one; only the R types become Python types.

| r5r (R) | peabiru (Python) |
|---|---|
| `r5r_network <- build_network(path)` | `network = peabiru.build_network(path)` |
| `travel_time_matrix(r5r_network, origins, destinations, ...)` | `peabiru.travel_time_matrix(network, origins, destinations, ...)` |
| `data.frame` / `sf` POINT with `id`, `lon`, `lat` | `pandas.DataFrame` with `id`, `lon`, `lat` / `geopandas.GeoDataFrame` of points |
| `mode = c("WALK", "TRANSIT")` | `mode=["WALK", "TRANSIT"]` (or a string) |
| `departure_datetime = as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")` | `departure_datetime=datetime.datetime(2019, 5, 13, 14, 0)` (naive or aware) |
| `max_walk_time = Inf`, `n_threads = Inf` | `max_walk_time=math.inf`, `n_threads=None` (or `math.inf`) |
| `NULL` (`seed`, `cutoffs`, `decay_value`, ...) | `None` |
| result `data.table` | result `pandas.DataFrame` |
| `integer` columns with `NA` (`travel_time_pXX`, `n_rides`) | nullable `Int32` with `<NA>` |
| `integer` columns without `NA` (`percentile`, `cutoff`, `draw_number`) | `int32` |
| `double` columns (`accessibility`, times of expanded matrices) | `float64`, `NaN` where r5r has `NA` |
| `character` columns (`from_id`, `to_id`, `id`, `opportunity`, `departure_time`, `routes`) | object dtype holding `str`; a missing `routes` is `None` |

For example, this r5r call

```r
ttm <- travel_time_matrix(
  r5r_network, points, points,
  mode = c("WALK", "TRANSIT"),
  departure_datetime = as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo"),
  percentiles = c(25, 75),
  max_walk_time = 15,
  seed = 42
)
```

is, in peabiru:

```python
import datetime as dt
import peabiru

network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
points = peabiru.sample_points()
ttm = peabiru.travel_time_matrix(
    network, points, points,
    mode=["WALK", "TRANSIT"],
    departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    percentiles=[25, 75],
    max_walk_time=15,
    seed=42,
    n_threads=2,
)
print(ttm.dtypes)
```

```text
from_id            object
to_id              object
travel_time_p25     Int32
travel_time_p75     Int32
dtype: object
```

### What differs

- **No Java, no `r5r_core`.** The network is a Python object in memory;
  nothing is written to the data directory (no `network.dat`), and
  `overwrite` and `temp_dir` are accepted but have no effect. A network
  cannot be pickled: build it again in each process.
- **`build_network()` takes `n_threads`** (threads used to read the `.pbf`).
- **`verbose` and `progress`** are checked as r5r checks them, but peabiru
  prints no progress counter; `verbose=True` only reports the build of the
  network (on standard error).
- **Errors are Python exceptions**: `TypeError` for an argument of the wrong
  type, `ValueError` for an invalid value (the messages follow r5r's
  checkmate assertions), and `peabiru.UnsupportedError` (a
  `NotImplementedError`) for what is not supported yet. r5r's warnings
  (non-character ids, missing coordinates, missing opportunity counts, few
  services running on the date) are Python `UserWarning`s.
- **Ids** that are not strings are converted with `str()`. For integer ids
  this is what R's `as.character()` gives; for floating-point ids the text
  can differ (`str(1.0)` is `"1.0"`, R gives `"1"`).
- **Naive datetimes** are read as clock times. For
  `arrival_travel_time_matrix()`, `arrival_datetime - max_trip_duration` is
  then computed on the clock time; an aware datetime gets R's `POSIXct`
  arithmetic on the absolute time (which matters only across a
  daylight-saving change).
- **`arrival_datetime`** has a default of `None` (Python cannot have a
  required argument after optional ones); leaving it out is the same error
  as in r5r.

## From r5py

[r5py](https://r5py.readthedocs.io) is a Python wrapper of R5 too, with its
own API. peabiru follows r5r's API instead, so that R and Python users share
one vocabulary and one set of results. The main correspondences, with
r5py's names as of its 1.x releases:

| r5py | peabiru |
|---|---|
| `TransportNetwork(osm_pbf, gtfs=[...])` | `build_network(directory)`, a directory with one `.pbf` and the GTFS `.zip` files |
| `TravelTimeMatrix(network, origins=..., destinations=..., ...)` | `travel_time_matrix(network, origins, destinations, ...)` |
| origins as a `GeoDataFrame` with `id` and point geometry | a `DataFrame` with `id`, `lon`, `lat`, or the same `GeoDataFrame` |
| `transport_modes=[TransportMode.TRANSIT, TransportMode.WALK]` | `mode=["WALK", "TRANSIT"]` |
| `departure=datetime(...)` | `departure_datetime=datetime(...)` |
| `departure_time_window=timedelta(minutes=10)` | `time_window=10` (minutes) |
| `max_time=timedelta(hours=2)` | `max_trip_duration=120` (minutes) |
| `max_time_walking=timedelta(...)` | `max_walk_time=...` (minutes) |
| `speed_walking=3.6`, `speed_cycling=12.0` | `walk_speed=3.6`, `bike_speed=12` |
| `max_bicycle_traffic_stress=2` | `max_lts=2` |
| `max_public_transport_rides=4` | `max_rides=3` (r5r's default) |
| result column `travel_time` | `travel_time_p50` (one column per percentile) |

Results are equal to r5py's only where r5py runs the same R5 version with
the same parameters; peabiru is validated against r5r, not r5py.

## Not supported yet

Each of these raises `peabiru.UnsupportedError` instead of being ignored:

- public transport with `BICYCLE` or `CAR` access (`mode=["BICYCLE", "TRANSIT"]`),
  and a `mode_egress` other than `"WALK"`;
- routing on a subset of transit modes (`"BUS"`, `"RAIL"`, ...) instead of
  `"TRANSIT"`;
- fares: `fare_structure`, and a finite `max_fare`;
- scenarios: `new_carspeeds`, `carspeed_scale` other than 1, `new_lts`;
- writing results to `output_dir`;
- elevation: a `.tif` raster in the data directory requires
  `elevation="NONE"`;
- expanded and arrival matrices by a street mode alone (r5r takes the
  departure time from the clock there);
- more than one `.pbf` file in the data directory.

r5r's `isochrone()`, `detailed_itineraries()`, `pareto_frontier()`,
`travel_time_matrix()` with fares and the other functions not listed in the
[API reference](reference.md) are not available yet. `CAR_PARK` and
`BICYCLE_RENT` are rejected, as r5r rejects them.
