# Coming from r5py / r5r

peabiru's functions have the names, arguments and defaults of the R package
**r5r 2.5.0**, and its engine is a Rust port of R5 7.6, the engine r5r
drives: result columns, row order and values are r5r's. Every function is
compared with r5r itself, call for call, on r5r's two sample cities (Porto
Alegre and Sao Paulo), and the results are identical, including the last bit
of the accessibility values. This page lists how a call translates, and every
place where peabiru differs from r5r on purpose. Besides r5r's functions,
peabiru has one of its own, [`trip_options()`](trip-options.md), with no r5r
counterpart; it is not a difference and is not listed here.

## From r5r

Calls translate one to one; only the R types become Python types.

| r5r (R) | peabiru (Python) |
|---|---|
| `r5r_network <- build_network(path)` | `network = peabiru.build_network(path)` |
| `travel_time_matrix(r5r_network, origins, destinations, ...)` | `peabiru.travel_time_matrix(network, origins, destinations, ...)` |
| `data.frame` / `sf` POINT with `id`, `lon`, `lat` | `pandas.DataFrame` with `id`, `lon`, `lat` / `geopandas.GeoDataFrame` of points |
| `mode = c("WALK", "TRANSIT")` | `mode=["WALK", "TRANSIT"]` (or a string) |
| `departure_datetime = as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")` | `departure_datetime=datetime.datetime(2019, 5, 13, 14, 0)` (naive or aware) |
| `max_walk_time = Inf`, `n_threads = Inf` | `max_walk_time=math.inf`, `n_threads=math.inf` (the default; `None` means the same) |
| `NULL` (`seed`, `cutoffs`, `decay_value`, ...) | `None` |
| `c(0, 15, 30)` (`cutoffs`, `percentiles`, ...) | a list or tuple, `[0, 15, 30]` |
| result `data.table` / `sf` | result `pandas.DataFrame` / `geopandas.GeoDataFrame` |
| `integer` columns with `NA` (`travel_time_pXX`, `n_rides`, `travel_time`) | nullable `Int32` with `<NA>` |
| `integer` columns without `NA` (`percentile`, `cutoff`, `draw_number`, `option`, `segment`, ...) | `int32` |
| `double` columns (`accessibility`, times of expanded matrices, ...) | `float64`, `NaN` where r5r has `NA` |
| `double` OpenStreetMap ids (`osm_id` of `street_network_to_sf()` and line isochrones) | `int64` (R has no 64-bit integers) |
| `character` columns (`from_id`, `to_id`, `id`, `opportunity`, `departure_time`, `routes`, ...) | object dtype holding `str`; a missing value is `None` |
| `logical` columns (`found`, `walk`, `car`, ...) | `bool` |
| `Date` column of `check_transit_availability()` | `datetime64` |
| a fare structure (a `list` of settings and `data.table`s) | a `dict` with the same keys, and `pandas.DataFrame` tables |
| `street_network_bbox(output = "bbox")` / `"vector"` / `"polygon"` | a tuple `(xmin, ymin, xmax, ymax)` / a dict with those keys / a `GeoSeries` |

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

## Deliberate differences

### General

<!-- diff:no-java -->
- **No Java.** There is no JVM, no `java.parameters` and no R5 jar. r5r's
  five functions that manage Java (`setup_r5()`, `stop_r5()`,
  `download_r5()`, `r5r_cache()`, `r5r_sitrep()`) have no Python
  counterpart: there is nothing to manage. There is no `r5r_core` argument
  either.

<!-- diff:result-type -->
- **Results are pandas objects**: `pandas.DataFrame`s, and
  `geopandas.GeoDataFrame`s where r5r returns `sf` objects (also the tables
  of a fare structure and the edges, vertices, stops and routes of the
  network). Column names, order and values are r5r's; the types are the
  Python equivalents in the table above.

<!-- diff:network-in-memory -->
- **The network lives in memory only.** Nothing is written to the data
  directory (no `network.dat`, no `gtfs_errors.csv`); a
  [`Network`][peabiru.Network] cannot be pickled and is built again in each
  process (it takes seconds even for a whole city). `overwrite` and
  `temp_dir` are accepted, checked as r5r checks them, and have no effect.

<!-- diff:n-threads-build -->
- **`build_network()` takes `n_threads`**, the threads used to read the
  `.pbf`.

<!-- diff:verbose-progress -->
- **`verbose` and `progress`** are checked as r5r checks them, but peabiru
  prints no progress counter; `verbose=True` only reports the build of the
  network, on standard error.

<!-- diff:error-classes -->
- **Errors are Python exceptions** with r5r's messages, word for word
  (checkmate's and cli's), except where the network class is named
  (`peabiru.Network`): `TypeError` for an argument of the wrong type,
  `ValueError` for an invalid value, `RuntimeError` for a file that cannot
  be read or a network that cannot be built, and
  [`peabiru.UnsupportedError`][peabiru.UnsupportedError] (a
  `NotImplementedError`) for the refusals listed below. r5r's warnings
  (ids that are not strings, missing coordinates, missing opportunity
  counts, few services running on the date, ...) are `UserWarning`s.

### Where peabiru is deterministic and r5r is not

<!-- diff:gtfs-errors-order -->
- **[`get_gtfs_errors()`][peabiru.get_gtfs_errors] reads the feeds again**
  (r5r reads the `gtfs_errors.csv` its `build_network()` wrote) and lists the
  rows feed by feed in the order gtfs-lib finds them; r5r's order is that of
  a Java `HashSet` and changes from run to run. The rows and columns are the
  same.

<!-- diff:elevation-strictmath -->
- **Elevation rasters in a projected system.** In a raster that is not in
  EPSG:4326, r5r reads one pixel or the other for a street point within
  nanometres (5e-8 m, or 2e-13 degrees) of a pixel boundary or of the
  raster's edge, depending on the last bits of the JVM's sines and cosines;
  peabiru reads the pixel Java's `StrictMath` gives, on every machine, and
  warns once with the number of such points (see
  [Elevation](elevation.md)).

<!-- diff:street-expanded -->
- **Expanded and arrival matrices need public transport.** With a street
  mode alone, r5r reports the same trip for every minute of a departure time
  taken from the computer's clock, not from `departure_datetime`; peabiru
  raises `UnsupportedError` there (use
  [`travel_time_matrix()`][peabiru.travel_time_matrix] for street modes).

Not a difference: without `seed`, frequency-based GTFS feeds give random
results in both packages; with a `seed`, peabiru's are r5r's.

### Warnings peabiru adds

<!-- diff:elevation-wrapped -->
- **Voids and out-of-range elevations.** Voids and values beyond
  +/-3276.7 m are stored wrapped around as r5r stores them (R5 keeps
  decimetres in 16-bit integers: SRTM's -32768 voids become 0 m, -9999
  becomes 3108.2 m, and a NaN no-data value is stored as 0 m), so the slope
  costs are r5r's; peabiru warns, counting them and the samples equal to the
  raster's no-data value. r5r is silent. (The `StrictMath` item above also
  warns.)

### Errors where r5r fails later or routes on corrupted values

<!-- diff:errors-instead-of-failures -->
- **Clear errors instead of failures.** peabiru raises an error of its own,
  saying what is wrong, where r5r lets a value through and then fails in
  Java or with an internal R error: an empty `percentiles`; a missing id with
  `output_dir` (Java's `NullPointerException`); an `arrival_datetime` that is
  not a `datetime`; an isochrone `polygon_output` that is not a boolean, or a
  fractional `zoom` (r5r passes it to R5); a `carspeed_scale` of NaN; a fare
  structure whose fare type is not in `fares_per_type` or that misses a
  route of the network (Java's `NullPointerException`);
  `get_gtfs_errors()` on a folder that does not exist.

<!-- diff:scenario-wrap -->
- **Scenarios R5 would corrupt.** Scenarios whose car speeds R5's 16-bit
  cm/s would wrap or bring to 0 or less (a negative factor or speed in
  `new_carspeeds`, a polygon `scale` of `math.inf` or 0, a `carspeed_scale`
  far above 1) and a `carspeed_scale` beyond the 32-bit float range are
  errors; r5r routes on the wrapped speeds, hands R5 Infinity, or fails
  dividing by zero. Polygons that are not valid, and ties between polygons
  that peabiru cannot settle exactly as R5 does, are errors too (see
  [Scenarios](scenarios.md)).

### Refusals that remain

r5r accepts these; peabiru stops with an error that says so, never
ignoring the request:

<!-- diff:refuse-fare-calculators -->
- R5's own fare calculators (a fare structure with a `type`):
  `UnsupportedError`.

<!-- diff:refuse-elevation-formats -->
- An elevation raster that is not a one-band GeoTIFF, north up,
  uncompressed or compressed with LZW, Deflate or PackBits, in EPSG:4326 or
  in a system whose GeoTools transform peabiru reproduces (SIRGAS 2000 4674,
  SAD69 4618, Corrego Alegre 4225 and their UTM zones used in Brazil,
  SIRGAS 2000 / Brazil Polyconic 5880, WGS 84 / UTM 32601-32660 and
  32701-32760, Pseudo-Mercator 3857) given by its EPSG code; a `.prj` file
  next to the raster; a raster named `.tiff` (r5r fails on it); more than one
  raster in the data directory (r5r reads the last one listed); a street
  point that `StrictMath` puts on a projected raster's outer edge (r5r fails
  there too). These are `RuntimeError`s from `build_network()`.

<!-- diff:refuse-multiple-pbf -->
- More than one `.pbf` file in the data directory (r5r uses one of them):
  `ValueError`.

<!-- diff:refuse-pareto-output-dir -->
- [`pareto_frontier()`][peabiru.pareto_frontier] with `output_dir` (r5r
  writes CSV files): `UnsupportedError`.

<!-- diff:refuse-itinerary-cases -->
- [`detailed_itineraries()`][peabiru.detailed_itineraries]: the few pairs
  whose r5r result depends on paths peabiru does not reproduce are an error
  (`ValueError`, starting with `not supported:`) naming the pair, rather than
  a different itinerary: a car search backwards (`mode_egress="CAR"`) through
  a turn restriction with two or more via ways, when the itineraries change
  with the order R5 leaves those via edges in (see
  [Detailed itineraries](itineraries.md#car-egress-and-turn-restrictions)),
  and a ride along a GTFS shape cut by `shape_dist_traveled`.

(The expanded and arrival matrices by a street mode alone, above, are also
a refusal.)

### Output details

<!-- diff:write-fare-zip -->
- [`write_fare_structure()`][peabiru.write_fare_structure] writes the same
  CSV files as r5r into an uncompressed `.zip`, so the archive's bytes
  differ from r5r's (which also holds file dates); each package reads the
  other's files. As in r5r 2.5.0, `debug_path` and `debug_info` are kept in
  the structure and no debug file is written.

<!-- diff:isochrone-make-valid -->
- [`isochrone()`][peabiru.isochrone] returns the polygons r5r hands to
  `sf::st_make_valid()`, vertex for vertex on the same platform, without
  that last s2 rebuild: they cover the same areas but are not snapped to
  1e-7 degrees and are always `MultiPolygon`s. Pass them to
  `shapely.make_valid()` if your tools need valid geometries.

<!-- diff:platform-order -->
- Not a difference, but worth knowing: GTFS feeds are loaded in the order
  in which the operating system lists the directory, as R5 loads them, so
  with more than one feed the expanded and arrival matrices can differ
  slightly between macOS and Linux (another stop or route for a few trips),
  exactly as r5r's do; isochrone rings follow the C++ library isoband is
  built with (libc++ on macOS, libstdc++ on Linux and Windows), as r5r's.

### Only in Python

- **Required arguments after optional ones** have a default of `None`
  (Python cannot have a required argument after optional ones):
  `arrival_datetime` of
  [`arrival_travel_time_matrix()`][peabiru.arrival_travel_time_matrix], and
  `fare_structure` and `fare_cutoffs` of
  [`pareto_frontier()`][peabiru.pareto_frontier]. Leaving them out is an
  error, as in r5r.
- **Ids** that are not strings are converted with `str()`, with r5r's
  warning. For integer ids this is what R's `as.character()` gives; for
  floating-point ids the text can differ (`str(1.0)` is `"1.0"`, R gives
  `"1"`).
- **Naive datetimes** are read as clock times. For
  `arrival_travel_time_matrix()`, `arrival_datetime - max_trip_duration` is
  then computed on the clock time; an aware datetime gets R's `POSIXct`
  arithmetic on the absolute time (which matters only across a
  daylight-saving change).
- **A route of [`transit_network_to_sf()`][peabiru.transit_network_to_sf]
  whose line has a single point** (a pattern with one linked stop) is a
  shapely `Point`; r5r's `sf` holds a one-point `LINESTRING`, which shapely
  cannot represent.

<!-- diff:deprecated-args -->
- **Deprecated arguments**: there is no `r5r_core` (r5r's old name for the
  network argument). `isochrone(sample_size=...)` is accepted, as r5r
  accepts its deprecated argument: giving it only warns.

### Kept for equal results

Some behaviours are r5r's (and R5's), kept so that the results are equal:

- walking trips are routed from the destinations when there are more
  origins than destinations (and no elevation raster), as r5r does, and the
  columns swapped back;
- the Monte Carlo draws of frequency-based feeds come from R5's random
  number generator (a Mersenne Twister seeded from `seed` and the origin's
  coordinates), so the same `seed` gives r5r's numbers;
- accessibility uses the `FastMath.exp()` of Apache Commons Math that R5
  calls, which makes the values bit-identical to r5r's;
- departure times past midnight are written past `24:00:00`, as R5 writes
  them, and the expanded and arrival matrices accept at most 5,000
  destinations, R5's limit;
- `CAR_PARK` and `BICYCLE_RENT` are rejected, as r5r rejects them.

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
| `max_bicycle_traffic_stress` (default 3) | `max_lts` (default 2, as in r5r) |
| `max_public_transport_rides` (default 8) | `max_rides` (default 3, as in r5r) |
| result column `travel_time` (with the default `percentiles=[50]`) | `travel_time_p50` (always one column per percentile) |
| `TransportNetwork(..., elevation_model="dem.tif", elevation_cost_function=ElevationCostFunction.TOBLER)` | a `.tif` raster in the directory and `build_network(directory, elevation="TOBLER")` (see [Elevation](elevation.md)) |
| `DetailedItineraries(network, origins, destinations, force_all_to_all=True, ...)` | `detailed_itineraries(network, origins, destinations, all_to_all=True, ...)` |
| `Isochrones(network, origins, isochrones=[timedelta(minutes=15), ...], ...)` | `isochrone(network, origins, cutoffs=[15, ...], ...)` (minutes) |

Results are equal to r5py's only where r5py runs the same R5 version with
the same parameters; peabiru is validated against r5r, not r5py.
