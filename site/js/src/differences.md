# Compared with r5r

[r5r](https://github.com/ipea/r5r) is the R package of Ipea that runs
Conveyal's R5 routing engine on a Java virtual machine. This page is for
those who know it: what peabiru shares with r5r, what differs, and how to
move a script over. It compares the JavaScript package with r5r 2.5.0
(R5 7.6).

## What is shared

- **The routing model is R5's.** peabiru's engine is a port of R5 to Rust:
  the same street network built from OpenStreetMap, the same linking of
  stops and points, the same public transport search with percentiles and
  Monte Carlo draws, the same fare rules, slope costs and scenarios.
- **The results are the same.** The repository compares peabiru with r5r
  on its test networks (Porto Alegre and São Paulo, and the whole city of
  Porto Alegre) on every push: travel-time matrices, accessibility,
  expanded and arrival matrices, isochrones, detailed itineraries, fares
  and Pareto frontiers come out identical, row for row and value for
  value, with the exceptions listed below. The JavaScript package is
  compared cell by cell with peabiru's native tool
  (`bench/compare/js_parity.mjs`). With frequency-based feeds the results
  are random without a `seed`, in both; with the same `seed` they are
  equal.
- **The functions have the same shape**: one function per kind of result,
  with the same arguments (here options in camelCase), defaults and result
  columns.

`tripOptions()` (see [Trip options](trip-options.md)) is peabiru's own
planner and has no counterpart in r5r.

## Moving a script over

| r5r | peabiru in JavaScript |
|---|---|
| `options(java.parameters = "-Xmx4G")`, `library(r5r)` | `import * as peabiru from "peabiru"`: no Java, no memory setting |
| `r5r_network <- build_network(data_path)` | `const network = await peabiru.buildNetwork({ pbf, gtfs: [{ name, data }], tif })`: the bytes of the files |
| `travel_time_matrix(r5r_network, origins, destinations, mode = ..., ...)` | `await peabiru.travelTimeMatrix(network, origins, destinations, { mode, ... })` |
| arguments in snake_case | options in camelCase (`departureDatetime`, `maxTripDuration`) |
| `data.frame` or `sf` points | columns `{ id, lon, lat }` or an array of `{ id, lon, lat }` objects, in WGS 84 |
| `POSIXct` | a string `"YYYY-MM-DD HH:MM:SS"` or a `Date` |
| a `data.table` result | an object of columns (arrays of strings and typed arrays) |
| an `sf` result | a GeoJSON `FeatureCollection` |
| `NA` | [`NA_INT32`](api.md#NA_INT32) in integer columns, `NaN` in double columns, `null` in strings |
| warnings | `onWarning` (default `console.warn`); those of the build also in `network.buildWarnings()` |
| `n_threads` | `nThreads` in `buildNetwork()`: workers, each with a copy of the network |
| `stop_r5()`, `rJava::.jgc()` | `network.free()` |
| `setup_r5()`, `download_r5()`, `r5r_cache()`, `r5r_sitrep()` | nothing to set up |
| `read_fare_structure(path)`, `write_fare_structure(fs, path)` | `readFareStructure(bytes)`, `writeFareStructure(fs)`: bytes, in the same file format |
| `get_gtfs_errors(r5r_network)` | `await getGtfsErrors([{ name, data }])` |
| `output_dir` | not available (no file system) |

Every call that reads or routes on a network is asynchronous: `await` it.
See [Coming from R or Python](r-python.md) for what JavaScript makes
different from peabiru's other packages, which applies here too.

## Differences

The differences from r5r 2.5.0 are deliberate, and the same in peabiru's
three packages; they are listed here as they apply to JavaScript.

### General

<!-- diff:no-java -->
- **No Java**: no JVM, no memory setting, no R5 jar. r5r's five functions
  that manage Java and its files (`setup_r5()`, `stop_r5()`,
  `download_r5()`, `r5r_cache()`, `r5r_sitrep()`) have nothing to manage
  here and do not exist. `version` and `coreVersion()` give the versions
  of the package and of its Rust core.

<!-- diff:result-type -->
- **Results** are objects of columns (arrays of strings, typed arrays) and
  GeoJSON `FeatureCollection`s where r5r returns `data.table`s and `sf`
  tables; the tables of a fare structure are objects of columns too.
  Column names, order and values are the same (see the table above for
  missing values).

<!-- diff:sf-any-crs -->
- **Geometries in another coordinate reference system** (R only). The R
  package of peabiru reprojects an `sf` input in any CRS to WGS 84, with a
  message, where r5r stops with an error. In JavaScript points are
  longitudes and latitudes, and scenario polygons and lines GeoJSON, in
  WGS 84 as GeoJSON requires: there is no CRS to reproject from.

<!-- diff:network-in-memory -->
- **The network lives in memory only**, built from the bytes of the files
  (`buildNetwork({ pbf, gtfs, tif })`) rather than from a directory:
  nothing is written anywhere (no `network.dat`, no `gtfs_errors.csv`), a
  network cannot be saved or sent to another Worker, and it is built again
  in each session. r5r's `overwrite` and `temp_dir` have nothing to do and
  are not options. `network.free()` releases its memory.

<!-- diff:n-threads-build -->
- `buildNetwork()` takes `nThreads`, which r5r's `build_network()` does
  not have: here, the number of workers to route on, each with its own
  copy of the network (see [Threads and workers](threads.md)).

<!-- diff:verbose-progress -->
- `verbose` and `progress` are checked, but nothing is printed: no
  progress counter, no report of the build.

<!-- diff:error-classes -->
- **Errors**: most argument errors read as r5r's (both word them as
  checkmate does) in JavaScript's terms, with the option names in
  camelCase; see [Coming from R or Python](r-python.md#errors) for the
  terms, the error classes and the few cases worded differently. Messages
  never name r5r or its network argument: `detailedItineraries()` on a
  network with frequencies fails with `Assertion on 'network' failed`.

<!-- diff:deprecated-args -->
- **Deprecated arguments**: r5r's deprecated arguments do not exist here:
  there is no `r5rCore` (its old name for the network argument) and
  `isochrone()` has no `sampleSize`. Giving one is an error, as any
  unknown option is.

### Where peabiru is deterministic and r5r is not

<!-- diff:gtfs-errors-order -->
- [`getGtfsErrors()`](network.md#gtfs-errors) reads the feeds again (r5r
  reads the `gtfs_errors.csv` its `build_network()` wrote) and lists the
  errors feed by feed, in the order gtfs-lib finds them; r5r's order is
  that of a Java `HashSet` and changes from run to run. The rows and
  columns are the same.

<!-- diff:elevation-strictmath -->
- In an [elevation raster](elevation.md) not in EPSG:4326, r5r reads one
  pixel or the other for a street point within nanometres (5e-8 m, 2e-13
  degrees) of a pixel boundary or of the raster's edge, depending on the
  last bits of the JVM's sines and cosines; peabiru reads the pixel Java's
  `StrictMath` gives, on every machine, and warns once with the number of
  such points.

<!-- diff:street-expanded -->
- r5r computes expanded and arrival matrices with a street mode alone, but
  reports the same trip for every minute, from a departure time taken from
  the computer's clock rather than from `departure_datetime`.
  `expandedTravelTimeMatrix()` and `arrivalTravelTimeMatrix()` throw
  `UnsupportedError` there: use `travelTimeMatrix()` for street modes.

### Warnings peabiru adds

<!-- diff:elevation-wrapped -->
- Elevation voids and values beyond +/-3276.7 m are stored wrapped around,
  as R5 stores them (16-bit integers of decimetres: SRTM's -32768 becomes
  0 m, -9999 becomes 3108.2 m; a NaN no-data value is stored as 0 m), so
  the slope costs are the same as r5r's; peabiru warns, counting them and
  the samples equal to the raster's no-data value, where r5r is silent.
  The pixel read with `StrictMath` (above) also warns.

### Errors where r5r fails later or routes on corrupted values

<!-- diff:errors-instead-of-failures -->
- peabiru throws an error of its own that says what is wrong where r5r
  lets a value through and then fails, in Java or in R: empty
  `percentiles`; an `arrivalDatetime` that is not a date and time; a
  fractional `zoom` in `isochrone()`; a `carspeedScale` of `NaN`; a
  missing destination id; `isochrone()` with no origins; a scenario value
  that is not a number; `writeFareStructure()` with something that is not
  a fare structure; a fare structure with a column that is not one of its
  table's, whose fare type is not in `fares_per_type`, or that leaves out
  a route of the network.

<!-- diff:scenario-wrap -->
- [Scenarios](scenarios.md) whose car speeds R5's 16-bit centimetres per
  second would wrap around or bring to 0 or less (a negative factor or
  speed in `newCarspeeds`, a polygon `scale` of `Infinity` or 0, a
  `carspeedScale` far above 1), and a `carspeedScale` beyond the range of
  a 32-bit float, are errors; r5r routes on the wrapped speeds, hands R5
  `Infinity`, or fails dividing by zero. Polygons that are not valid, and
  ties between polygons that peabiru cannot settle exactly as R5 does, are
  errors too. r5r warns that a scenario drops the slope costs of elevation
  on every network; peabiru only on a network that has elevation.

### What peabiru refuses

These are errors that say so, where r5r computes something:

<!-- diff:refuse-fare-calculators -->
- R5's own fare calculators (a `fareStructure` with a `type`):
  `UnsupportedError`.

<!-- diff:refuse-elevation-formats -->
- An elevation raster other than a one-band GeoTIFF, north up,
  uncompressed or compressed with LZW, Deflate or PackBits, in EPSG:4326
  or one of the systems listed in [Elevation](elevation.md#rasters), given
  by its EPSG code in the file; and a street point that `StrictMath` puts
  on a projected raster's outer edge (r5r fails there too). There is one
  raster, `tif`, by construction.

<!-- diff:refuse-multiple-pbf -->
- r5r takes one of several `.pbf` files in its data directory; here there
  is one `pbf` by construction.

<!-- diff:car-egress-history -->
- `detailedItineraries()` with `modeEgress: "CAR"`: where a car search
  crosses a turn restriction with two or more via ways, r5r's itineraries
  can depend on the searches that ran before (R5 reverses those via edges
  in place); peabiru returns what r5r gives on a freshly built network for
  that pair alone with one thread, with a warning (`onWarning`) naming the
  pairs some other order would change. In r5r a car matrix, accessibility
  or Pareto frontier computed after such a call could differ too (none did
  on Porto Alegre); peabiru's are those of a fresh network. See
  [Detailed itineraries](itineraries.md#car-egress-and-turn-restrictions).

### Output details

<!-- diff:write-fare-zip -->
- `writeFareStructure()` writes the same CSV files as r5r's
  `write_fare_structure()`, deflated in the same order, but the bytes of
  the archive differ from r5r's (which also holds file dates); each
  package reads the other's files. As in r5r 2.5.0, `debugPath` and
  `debugInfo` are kept in the structure and no debug file is written.

<!-- diff:isochrone-make-valid -->
- `isochrone()` returns the polygons r5r hands to `sf::st_make_valid()`,
  without that last rebuild with s2: they cover the same areas but their
  vertices are not snapped to 1e-7 degrees, the rings of one polygon may
  touch at a vertex, and they are always `MultiPolygon`s.

<!-- diff:platform-order -->
- r5r loads the GTFS feeds of its directory in the order the operating
  system lists it (see [Feed order](r-python.md#feed-order)); here the
  order is that of the `gtfs` array. r5r's isochrone rings start where
  isoband, built with the platform's C++ library, puts them (libc++ on
  macOS, libstdc++ on Linux and Windows); the JavaScript package gives the
  macOS order on every platform, and its polygon coordinates can differ in
  the last digits (about 1e-14 degrees), because WebAssembly has its own
  `sinh` and `atan`. The line isochrones are identical.

### What only JavaScript does differently

- `outputDir` (writing results to CSV files) has no file system to write
  to: any value but `null` throws `UnsupportedError`.
- `getGtfsErrors()` takes the feeds' bytes, as `buildNetwork()` does,
  since a network does not keep them; `readFareStructure()` takes the
  bytes of a `.zip` file and `writeFareStructure()` returns them.
- `nThreads` is a number of workers, each with its own copy of the
  network; an `AbortSignal` in `signal` stops a call.
- Dates and times can be `Date`s, read in the runtime's local time zone;
  strings `"YYYY-MM-DD HH:MM:SS"` are read as they are written.
- An option that is not one of the function's options is an error.

## Citing r5r

Pereira, R. H. M., Saraiva, M., Herszenhut, D., Braga, C. K. V., & Conway,
M. W. (2021). r5r: Rapid Realistic Routing on Multimodal Transport
Networks with R5 in R. *Findings*, 21262.
<https://doi.org/10.32866/001c.21262>
