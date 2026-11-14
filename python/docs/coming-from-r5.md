# Compared with r5r and r5py

[r5r](https://ipea.github.io/r5r/) is the R interface to
[R5](https://github.com/conveyal/r5), Conveyal's routing engine in Java,
developed at Ipea; [r5py](https://r5py.readthedocs.io) is a Python wrapper of
the same engine. peabiru is an independent package with an engine of its
own, written in Rust. This is the one page of the documentation that
compares them: what peabiru shares with r5r, where it differs, and how to
move a script over.

## What is shared

- **The routing model.** peabiru's engine is a port of R5 7.6, the version
  r5r 2.5.0 runs: the street network built from OpenStreetMap, the transit
  layer built from GTFS, the searches, the Monte Carlo draws of
  frequency-based feeds, the decay functions, the fare router.
- **The fine print of the model.** Details that other pages state as plain
  facts about peabiru are R5's: the multi-criteria router (McRAPTOR) used
  with fares and for alternative itineraries, and its sampling of departure
  times; the fare rules (R5's rule-based in-routing fare calculator), with
  fares read as 32-bit floats; the levels of traffic stress; the limits
  (5,000 destinations for the expanded and arrival matrices, egress
  searched up to 5 km by bicycle or 30 minutes by car); elevations in
  16-bit decimetres and car speeds in 16-bit centimetres per second; the
  checks of the GTFS feeds (those of gtfs-lib, R5's GTFS library); and the
  numbers in the files of `output_dir`, written as Java writes a `double`
  (`2.0`, `1.0E-4`).
- **The vocabulary.** The functions peabiru has in common with r5r have
  the same names, and their arguments the same names, order and defaults,
  apart from the items listed below.
- **The results.** For the same call, peabiru and r5r 2.5.0 return the same
  rows, in the same order, with the same values, including the last bit of
  the accessibility values. This is checked in peabiru's repository, not
  assumed: `bench/compare/py_parity.py` runs 130 calls through both packages
  on r5r's two sample cities (Porto Alegre and Sao Paulo) and compares
  every value; `bench/compare/py_output_dir.py` compares the files written
  with `output_dir` byte for byte (151 files); `bench/compare/py_formals.py`
  compares the arguments and defaults of every function (209 arguments);
  and `bench/compare/py_argument_errors.py` checks the cases of invalid
  arguments shared by the R, Python and JavaScript packages. On the larger
  examples of this documentation the two also agree where they were
  compared: in [Recife](example-recife-caruaru.md), the 17 cells of Boa
  Viagem without a car route have none in r5r 2.5.0 either, and in the
  fares analysis of [Porto Alegre](example-scenarios.md), on 103 origins,
  r5r 2.5.0 gives the same access as peabiru, origin by origin, with the
  fare structure and without it.

Besides the functions it shares with r5r, peabiru has functions of its own,
which r5r has not: [`trip_options()`](trip-options.md), and
[`locate_facilities()` and `floating_catchment()`](facility-location.md).

## Moving a script from r5r

Calls translate one to one; the R types become Python types, and r5r's
`r5r_network` argument is called `network`.

| r5r (R) | peabiru (Python) |
|---|---|
| `r5r_network <- build_network(path)` | `network = peabiru.build_network(path)` |
| `travel_time_matrix(r5r_network, origins, destinations, ...)` | `peabiru.travel_time_matrix(network, origins, destinations, ...)` |
| `travel_time_matrix(r5r_network = net, ...)` | `peabiru.travel_time_matrix(network=net, ...)` |
| `setup_r5()`, `stop_r5()`, `download_r5()`, `r5r_cache()`, `r5r_sitrep()`, `options(java.parameters = ...)` | nothing: there is no Java to set up, stop or give memory to |
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

## Differences

### General

<!-- diff:no-java -->
- **No Java.** There is no JVM, no `java.parameters` and no R5 jar. r5r's
  five functions that manage Java (`setup_r5()`, `stop_r5()`,
  `download_r5()`, `r5r_cache()`, `r5r_sitrep()`) do not exist in peabiru:
  there is nothing to manage.

<!-- diff:deprecated-args -->
- **Argument names.** r5r's `r5r_network` is `network` in peabiru. r5r's
  deprecated arguments do not exist: `r5r_core` (its old name for the
  network argument) and `isochrone(sample_size=...)`.

<!-- diff:result-type -->
- **Results are pandas objects**: `pandas.DataFrame`s, and
  `geopandas.GeoDataFrame`s where r5r returns `sf` objects (also the tables
  of a fare structure and the edges, vertices, stops and routes of the
  network). Column names, order and values are the same; the types are the
  Python equivalents in the table above.

<!-- diff:sf-any-crs -->
- **Geometries in another coordinate reference system** (R only). The R
  package of peabiru reprojects an `sf` input in any CRS to WGS 84, with a
  message, where r5r stops with an error. In Python, as in r5r, a
  `GeoDataFrame` of points or of scenario polygons or lines must be in
  EPSG:4326 (a `ValueError` otherwise): reproject it with
  `GeoDataFrame.to_crs(4326)`.

<!-- diff:network-in-memory -->
- **The network lives in memory only.** Nothing is written to the data
  directory (r5r writes `network.dat` and `gtfs_errors.csv` there); a
  [`Network`][peabiru.Network] cannot be pickled and is built again in each
  process (it takes seconds even for a whole city). `overwrite` and
  `temp_dir` are accepted and checked, and have no effect.

<!-- diff:n-threads-build -->
- **`build_network()` takes `n_threads`**, the threads used to read the
  `.pbf`.

<!-- diff:verbose-progress -->
- **`verbose` and `progress`** are checked, but peabiru prints no progress
  counter; `verbose=True` only reports the build of the network, on
  standard error.

<!-- diff:error-classes -->
- **Errors are Python exceptions**, with messages of peabiru's own. The
  checks of the arguments are the same as r5r's, and most messages read as
  r5r's (checkmate's and cli's wording) in Python's terms: `None` for
  `NULL`, `bool`, `str`, `float`, `int` and `datetime` for R's types,
  positions counted from 1 as in R, cli's bullets and line breaks dropped,
  `network` and `peabiru.Network` where the network is named. Some are
  worded differently: r5r turns numbers and logical values given as a mode
  (`mode`, `mode_egress`, `by`) into text and reports the text as not a
  mode, where Python reports the value's type; `arrival_datetime` is
  checked itself (r5r checks the departure,
  `arrival_datetime - max_trip_duration`); `all_to_all` is named as such
  (r5r names the internal `unique_ids`); and a missing `fare_cutoffs` is
  the default. The repository checks the 1,367 cases shared by the three
  packages (`bench/compare/argument_errors.json`): 1,215 read the same in
  Python, and 152 are the ones worded differently.
  Classes: `TypeError` for an argument of the wrong type,
  `ValueError` for an invalid value, `RuntimeError` for a file that cannot
  be read or a network that cannot be built, and
  [`peabiru.UnsupportedError`][peabiru.UnsupportedError] (a
  `NotImplementedError`) for the refusals listed below. Warnings (ids that
  are not strings, missing coordinates, missing opportunity counts, few
  services running on the date, ...) are `UserWarning`s.

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
results in both packages; with a `seed`, the draws are the same.

### Warnings peabiru adds

<!-- diff:elevation-wrapped -->
- **Voids and out-of-range elevations.** Voids and values beyond
  +/-3276.7 m are stored wrapped around in both packages (R5 keeps
  decimetres in 16-bit integers: SRTM's -32768 voids become 0 m, -9999
  becomes 3108.2 m, and a NaN no-data value is stored as 0 m), so the slope
  costs are the same; peabiru warns, counting them and the samples equal to
  the raster's no-data value. r5r is silent. (The `StrictMath` item above
  also warns.) r5r warns on every call with a scenario that walking and
  cycling may be affected by missing elevation; peabiru warns, in its own
  words, only on a network with elevation.

### Errors where r5r fails later or routes on corrupted values

<!-- diff:errors-instead-of-failures -->
- **Clear errors instead of failures.** peabiru raises an error of its own,
  saying what is wrong, where r5r lets a value through and then fails in
  Java or with an internal R error: an empty `percentiles`; a missing
  destination id, or any missing id with `output_dir` (Java's
  `NullPointerException`); an `arrival_datetime` that is not a `datetime`;
  an isochrone `polygon_output` that is not a boolean, or a fractional
  `zoom` (r5r passes it to R5); origins without rows in `isochrone()`; a
  `carspeed_scale` of NaN; an `osm_id`, `max_speed` or `lts` of a scenario
  table that Java cannot parse (`NumberFormatException`); a zero-length
  `radius` in `find_snap()`; a fare structure with a column r5r does not
  know, whose fare type is not in `fares_per_type` or that misses a route
  of the network (Java's `NullPointerException`);
  `write_fare_structure()` on a structure that is not one;
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

### Refusals

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
  next to the raster (r5r uses its system instead of the GeoTIFF's); a
  raster named `.tiff` (r5r fails on it); more than one raster in the data
  directory (r5r reads the last one listed); a street point that
  `StrictMath` puts on a projected raster's outer edge (r5r fails there
  too). These are `RuntimeError`s from `build_network()`.

<!-- diff:refuse-multiple-pbf -->
- More than one `.pbf` file in the data directory (r5r uses one of them):
  `ValueError`.

<!-- diff:output-dir-ids -->
- **Ids that cannot name a file, with `output_dir`.** The files are named
  after the ids (`from_<id>.csv`), so an id holding a path separator (`/` or
  `\`) or NUL, and on Windows also `< > : " | ? *`, raises `ValueError`
  naming the id, before anything is routed or written. r5r fails on such an
  id with Java's `FileNotFoundException` (`a/b`, `../x`) or writes a file of
  another name (`from_a\b.csv` on Linux and macOS).
  [`trip_options()`][peabiru.trip_options] follows the same rule;
  [`detailed_itineraries()`][peabiru.detailed_itineraries] keeps the check
  r5r makes.

<!-- diff:refuse-pareto-output-dir -->
- [`pareto_frontier()`][peabiru.pareto_frontier] with `output_dir` (r5r
  writes CSV files): `UnsupportedError`.

<!-- diff:car-egress-history -->
- [`detailed_itineraries()`][peabiru.detailed_itineraries] with
  `mode_egress="CAR"`: where a car search crosses a turn restriction with two
  or more via ways, r5r's itineraries can depend on the searches that ran
  before (R5 reverses those via edges in place); peabiru returns what r5r
  gives on a freshly built network for that pair alone with one thread, with
  a warning naming the pairs some other order would change (see
  [Detailed itineraries](itineraries.md#car-egress-and-turn-restrictions)).
  In r5r a car matrix, accessibility or Pareto frontier computed after such
  a call could differ too (none did on Porto Alegre); peabiru's are those of
  a fresh network.

(The expanded and arrival matrices by a street mode alone, above, are also
a refusal.)

### Output details

<!-- diff:write-fare-zip -->
- [`write_fare_structure()`][peabiru.write_fare_structure] writes the same
  CSV files as r5r, deflated in the same order, but the archive's bytes
  differ from r5r's (which also holds file dates); each package reads the
  other's files. As in r5r 2.5.0, `debug_path` and `debug_info` are kept in
  the structure and no debug file is written.

<!-- diff:isochrone-make-valid -->
- [`isochrone()`][peabiru.isochrone] returns the polygons r5r hands to
  `sf::st_make_valid()`, vertex for vertex on the same platform, without
  that last s2 rebuild: they cover the same areas but are not snapped to
  1e-7 degrees and are always `MultiPolygon`s. Pass them to
  `shapely.make_valid()` if your tools need valid geometries. Line
  isochrones route to the centroids r5r takes with `sf::st_centroid()`, to
  within about 1e-14 degrees.

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
- **Ids** that are not strings are converted with `str()`, with a warning,
  as r5r converts them. For integer ids this is what R's `as.character()`
  gives; for floating-point ids the text can differ (`str(1.0)` is `"1.0"`,
  R gives `"1"`).
- **Naive datetimes** are read as clock times. For
  `arrival_travel_time_matrix()`, `arrival_datetime - max_trip_duration` is
  then computed on the clock time; an aware datetime gets R's `POSIXct`
  arithmetic on the absolute time (which matters only across a
  daylight-saving change).
- **A route of [`transit_network_to_sf()`][peabiru.transit_network_to_sf]
  whose line has a single point** (a pattern with one linked stop) is a
  shapely `Point`; r5r's `sf` holds a one-point `LINESTRING`, which shapely
  cannot represent.

### The same on purpose

Some behaviours that may surprise are R5's or r5r's, and peabiru has them
too, so that the results are equal:

- walking trips are routed from the destinations when there are more
  origins than destinations (and no elevation raster), and the columns
  swapped back;
- the Monte Carlo draws of frequency-based feeds come from R5's random
  number generator (a Mersenne Twister seeded from `seed` and the origin's
  coordinates), so the same `seed` gives the same numbers;
- accessibility uses the `FastMath.exp()` of Apache Commons Math that R5
  calls, which makes the values bit-identical;
- departure times past midnight are written past `24:00:00`, as R5 writes
  them, and the expanded and arrival matrices accept at most 5,000
  destinations, R5's limit;
- `max_lts` defaults to 2 and `max_rides` to 3;
- `CAR_PARK` and `BICYCLE_RENT` are rejected;
- [`build_network()`][peabiru.build_network] refuses a GTFS feed with
  errors of priority `HIGH`;
- [`detailed_itineraries()`][peabiru.detailed_itineraries] refuses public
  transport on a network with frequency-based trips, numbers the options
  in the same order, and gives the same OpenStreetMap ids with
  `osm_link_ids=True`;
- with fares, exactly 200 Monte Carlo draws, or too many for the window,
  are errors (r5r issue 602);
- a call with a scenario routes without slope costs (r5r issue 555).

## From r5py

r5py has an API of its own. The main correspondences, with r5py's names as
of r5py 1.1.7:

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
| `max_bicycle_traffic_stress` (default 3) | `max_lts` (default 2) |
| `max_public_transport_rides` (default 8) | `max_rides` (default 3) |
| result column `travel_time` (with the default `percentiles=[50]`) | `travel_time_p50` (always one column per percentile) |
| `TransportNetwork(..., elevation_model="dem.tif", elevation_cost_function=ElevationCostFunction.TOBLER)` | a `.tif` raster in the directory and `build_network(directory, elevation="TOBLER")` (see [Elevation](elevation.md)) |
| `DetailedItineraries(network, origins, destinations, force_all_to_all=True, ...)` | `detailed_itineraries(network, origins, destinations, all_to_all=True, ...)` |
| `Isochrones(network, origins, isochrones=[timedelta(minutes=15), ...], ...)` | `isochrone(network, origins, cutoffs=[15, ...], ...)` (minutes) |

Results are equal to r5py's only where r5py runs the same R5 version with
the same parameters; peabiru is compared with r5r, not with r5py.

## References

Pereira, R. H. M., Saraiva, M., Herszenhut, D., Braga, C. K. V., and
Conway, M. W. (2021). r5r: Rapid Realistic Routing on Multimodal Transport
Networks with R5 in R. *Findings*, 21262.
[doi:10.32866/001c.21262](https://doi.org/10.32866/001c.21262)

Conway, M. W., Byrd, A., and van der Linden, M. (2017). Evidence-Based
Transit and Land Use Sketch Planning Using Interactive Accessibility
Methods on Combined Schedule and Headway-Based Networks. *Transportation
Research Record*, 2653(1), 45-53.
[doi:10.3141/2653-06](https://doi.org/10.3141/2653-06)
