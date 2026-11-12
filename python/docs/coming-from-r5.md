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
- **Where r5r's result depends on the machine, peabiru's does not.** In an
  elevation raster that is not in EPSG:4326, r5r reads one pixel or the
  other for a street point within nanometres of a pixel boundary, depending
  on the last bits of the JVM's sines and cosines; peabiru reads the one
  Java's `StrictMath` gives, on every machine, and warns with the number of
  such points. Voids and values beyond +/-3276.7 m, which r5r stores
  wrapped around in silence, are stored the same way, with a warning.
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
- **Scenario limits**: a scenario whose car speeds R5's 16-bit cm/s would
  wrap or bring to 0 or less (a negative factor or speed, a polygon `scale`
  of `inf` or 0, a `carspeed_scale` far above 1) and a `carspeed_scale`
  beyond the 32-bit float range are errors, where r5r routes on the
  wrapped speeds (or fails dividing by zero); a `carspeed_scale` of NaN,
  which r5r lets through and then fails on with an internal R error, is a
  `ValueError` that says so.
- **GTFS errors are not written to the data folder**: as r5r,
  `build_network()` refuses feeds with errors of priority `"HIGH"`, but it
  does not write `gtfs_errors.csv`; `get_gtfs_errors()` checks the feeds
  again instead.

### Kept for equal results

Some behaviours are r5r's (and R5's), kept so that the results are equal:

- walking trips are routed from the destinations when there are more
  origins than destinations, as r5r does, and the columns swapped back;
- the Monte Carlo draws of frequency-based feeds come from R5's random
  number generator (a Mersenne Twister seeded from `seed` and the origin's
  coordinates), so the same `seed` gives r5r's numbers;
- accessibility uses the `FastMath.exp()` of Apache Commons Math that R5
  calls, which makes the values bit-identical to r5r's;
- departure times past midnight are written past `24:00:00`, as R5 writes
  them, and the expanded and arrival matrices accept at most 5,000
  destinations, R5's limit;
- GTFS feeds are loaded in the order in which the operating system lists
  the directory, as R5 loads them. With more than one feed, the expanded
  and arrival matrices can therefore differ slightly between machines
  (another stop or route for a few trips), exactly as r5r's do.

## From r5py

[r5py](https://r5py.readthedocs.io) is a Python wrapper of R5 too, with its
own API. peabiru follows r5r's API instead, so that R and Python users share
one vocabulary and one set of results. The main correspondences, with
r5py's names as of r5py 1.1.7:

| r5py | peabiru |
|---|---|
| `TransportNetwork(osm_pbf, gtfs=[...])` or `TransportNetwork.from_directory(path)` | `build_network(directory)`, a directory with one `.pbf` and the GTFS `.zip` files |
| `TravelTimeMatrix(network, origins=..., destinations=..., ...)` | `travel_time_matrix(network, origins, destinations, ...)` |
| origins as a `GeoDataFrame` with `id` and point geometry | a `DataFrame` with `id`, `lon`, `lat`, or the same `GeoDataFrame` |
| `transport_modes=[TransportMode.TRANSIT, TransportMode.WALK]` | `mode=["WALK", "TRANSIT"]` |
| `departure=datetime(...)` | `departure_datetime=datetime(...)` |
| `departure_time_window=timedelta(minutes=10)` | `time_window=10` (minutes) |
| `max_time=timedelta(hours=2)` | `max_trip_duration=120` (minutes) |
| `max_time_walking=timedelta(...)` | `max_walk_time=...` (minutes) |
| `speed_walking=3.6`, `speed_cycling=12.0` | `walk_speed=3.6`, `bike_speed=12` |
| `max_bicycle_traffic_stress=2` | `max_lts=2` |
| `max_public_transport_rides` (default 8) | `max_rides` (default 3, as in r5r) |
| result column `travel_time` (with the default `percentiles=[50]`) | `travel_time_p50` (always one column per percentile) |

Results are equal to r5py's only where r5py runs the same R5 version with
the same parameters; peabiru is validated against r5r, not r5py.

## Not supported yet

Each of these raises `peabiru.UnsupportedError` instead of being ignored:

- fares: `fare_structure`, and a finite `max_fare`;
- an elevation raster that is not a one-band GeoTIFF, uncompressed or
  compressed with LZW, Deflate or PackBits, in EPSG:4326 or one of the
  systems whose GeoTools transform peabiru reproduces (SIRGAS 2000, SAD69,
  Corrego Alegre and their UTM zones used in Brazil, SIRGAS 2000 / Brazil
  Polyconic, WGS 84 / UTM, Pseudo-Mercator); a `.prj` file next to the
  raster; a raster named `.tiff` (r5r fails on it) and
  more than one raster in the data directory (r5r reads the last one
  listed);
- expanded and arrival matrices by a street mode alone (r5r takes the
  departure time from the clock there);
- more than one `.pbf` file in the data directory.

[`isochrone()`][peabiru.isochrone] follows r5r's `isochrone()` up to the
polygons r5r hands to `sf::st_make_valid()` on the same platform (vertex for
vertex: the order of the rings and the vertex each starts at follow the C++
library isoband is built with, libc++ on macOS and libstdc++ on Linux and
Windows, and so does peabiru); that last
step, an s2 rebuild in R, is not reproduced, so the polygons cover the same
areas as r5r's but are not snapped to 1e-7 degrees and are always
`MultiPolygon`s.

[`find_snap()`][peabiru.find_snap], [`street_network_bbox()`][peabiru.street_network_bbox],
[`street_network_to_sf()`][peabiru.street_network_to_sf],
[`transit_network_to_sf()`][peabiru.transit_network_to_sf],
[`check_transit_availability()`][peabiru.check_transit_availability] and
[`get_gtfs_errors()`][peabiru.get_gtfs_errors] follow r5r's functions of the
same names, with GeoDataFrames for r5r's `sf` objects; `get_gtfs_errors()`
reads the feeds again rather than a `gtfs_errors.csv`, and lists the rows in
the order gtfs-lib finds them (r5r's order changes from run to run). r5r's
Java housekeeping functions (`setup_r5()`, `stop_r5()`, `download_r5()`,
`r5r_cache()`, `r5r_sitrep()`) have no Python counterpart: there is no Java.

r5r's `detailed_itineraries()` is available for the fastest trip
(`shortest_path=True`) without fares. r5r's `pareto_frontier()`,
`travel_time_matrix()` with fares and the other functions not listed in the
[API reference](reference.md) are not available yet. `CAR_PARK` and
`BICYCLE_RENT` are rejected, as r5r rejects them.
