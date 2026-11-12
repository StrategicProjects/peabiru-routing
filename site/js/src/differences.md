# Coming from R or Python

The JavaScript package has the functions, arguments, defaults, checks and
result columns of the R and Python packages, which have r5r's, and gives
the same values: the engine is the same Rust core, compiled to
WebAssembly. `bench/compare/js_parity.mjs` in the repository compares its
results with those of the native command-line tool cell by cell on r5r's
sample data, on the calling thread and on a pool of workers. What differs
is what JavaScript makes different.

## The calls

| R and Python | JavaScript |
|---|---|
| `build_network(data_path)`: a directory with one `.pbf` and the GTFS `.zip` files | `await buildNetwork({ pbf, gtfs: [{ name, data }] })`: the bytes of the files |
| `travel_time_matrix(network, origins, destinations, mode = ..., ...)` | `await travelTimeMatrix(network, origins, destinations, { mode, ... })` |
| arguments in snake_case: `departure_datetime`, `max_trip_duration` | options in camelCase: `departureDatetime`, `maxTripDuration` |
| `NULL` / `None` | `null` |
| `Inf` / `math.inf` | `Infinity` |
| points as a `data.frame`, `sf`, pandas `DataFrame` or `GeoDataFrame` | columns `{ id, lon, lat }` or an array of `{ id, lon, lat }` objects |
| `POSIXct` / `datetime` | a string `"YYYY-MM-DD HH:MM:SS"` (or a `Date`, read in local time) |
| a result `data.frame` / pandas `DataFrame` | an object of columns: arrays of strings and typed arrays |
| `NA` in integer columns | [`NA_INT32`](api.md#NA_INT32) (-2147483648) |
| `NA` in double columns | `NaN` |
| `NA` in character columns | `null` |
| warnings (`warning()`, `warnings.warn`) | `onWarning` (default `console.warn`) |
| `n_threads` threads sharing one network | `nThreads` workers, each with its own copy of the network |
| interrupting with Ctrl-C (or `setTimeLimit()` in R) | an `AbortSignal` in `signal` |
| the network is freed by the garbage collector | `network.free()` (the garbage collector does not see WebAssembly memory) |
| `sample_data_path()`, `sample_points()` (Python) | no bundled sample: the examples read the Python package's |
| `read_fare_structure(file_path)`, `write_fare_structure(fs, file_path)` | `readFareStructure(bytes)`; `writeFareStructure(fs)` returns the bytes |
| `get_gtfs_errors(network)` | `await getGtfsErrors([{ name, data }])`: the feeds' bytes |
| the `Date` column `date` of `check_transit_availability()` | strings `"YYYY-MM-DD"` |

An option that is not one of the package's options is an error, as an
unknown argument is in R and Python.

## Every call is asynchronous

Every function that reads or routes on a network returns a Promise, also
on a network without workers: `await` it. On the calling thread the
computation itself still blocks the thread while it runs (see
[Threads and workers](threads.md)). `readFareStructure()`,
`writeFareStructure()`, `coreVersion()`, `initSync()` and `network.info()`
are synchronous.

## Threads

In R and Python `n_threads` threads share one network in memory. In
JavaScript a thread is a worker, and workers do not share WebAssembly
memory, so each worker builds and holds its own copy of the network: the
memory grows with the number of workers (about 75 MiB per worker on the
Porto Alegre sample), and the default is one thread with no workers. The
results do not depend on the number of workers. See
[Threads and workers](threads.md).

## Feed order

r5r, and the R and Python packages, load the GTFS feeds of a directory in
the order the operating system lists it; with more than one feed that
order can change a few results of expanded and arrival matrices (another
stop or route for some trips). Here the feeds load in the order of the
`gtfs` array. To reproduce a network built from a directory, pass the
feeds in the order that directory lists them: on macOS the r5r sample of
Porto Alegre lists `poa_trensurb.zip` before `poa_eptc.zip`, and Linux
lists them the other way round. `network.info().feeds` gives the feed ids
in loading order.

A feed's `name` stands for its file name: it names the feed in messages
and, when the feed has no `feed_info.feed_id`, gives the feed id (the name
without `.zip`), which prefixes its stop ids.

## Differences from r5r

The three packages differ from r5r 2.5.0 in the same deliberate ways,
listed here as they apply to JavaScript. Results are otherwise r5r's: the
same rows, in the same order, with the same column names and values.
Missing coordinates, ids that are not strings and duplicated ids are
treated as in r5r, with the same warnings and errors. Without a `seed`,
frequency-based feeds give random results in r5r and in peabiru alike (a
new seed for each call); with the same `seed`, peabiru's results are
r5r's.

### General

<!-- diff:no-java -->
- **No Java**: no JVM, no memory setting, no R5 jar. r5r's five functions
  that manage Java and its files (`setup_r5()`, `stop_r5()`,
  `download_r5()`, `r5r_cache()`, `r5r_sitrep()`) have nothing to manage
  here and do not exist (the R package keeps them so that r5r scripts
  run). `version` and `coreVersion()` give the versions of the package and
  of its Rust core.

<!-- diff:result-type -->
- **Results** are objects of columns (arrays of strings, typed arrays) and
  GeoJSON `FeatureCollection`s where r5r returns `data.table`s and `sf`
  tables; the tables of a fare structure are objects of columns too.
  Column names, order and values are r5r's (see the table above for
  missing values).

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
- `verbose` and `progress` are checked as r5r checks them, but nothing is
  printed: no progress counter, no report of the build.

<!-- diff:error-classes -->
- **Errors**: argument errors give the messages of r5r's checks, with the
  option names in camelCase, as a `TypeError` (a value of the wrong type)
  or a `RangeError` (a value out of range); what peabiru does not do is an
  [`UnsupportedError`](api.md#UnsupportedError); what the engine finds
  wrong in the data (a fare structure, a scenario, a GTFS feed, a raster)
  is an `Error`. Errors raised in a worker reach the caller with the same
  class and message.

<!-- diff:deprecated-args -->
- **Deprecated arguments**: there is no `r5rCore` (r5r's old name for the
  network argument). `isochrone()` takes r5r's deprecated `sampleSize`,
  which only warns, as in r5r.

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
  the slope costs are r5r's; peabiru warns, counting them and the samples
  equal to the raster's no-data value, where r5r is silent. The pixel read
  with `StrictMath` (above) also warns.

### Errors where r5r fails later or routes on corrupted values

<!-- diff:errors-instead-of-failures -->
- peabiru throws an error of its own that says what is wrong where r5r
  lets a value through and then fails, in Java or in R: empty
  `percentiles`; an `arrivalDatetime` that is not a date and time; a
  fractional `zoom` in `isochrone()`; a `carspeedScale` of `NaN`;
  `writeFareStructure()` with something that is not a fare structure; a
  fare structure whose fare type is not in `fares_per_type` or that leaves
  out a route of the network.

<!-- diff:scenario-wrap -->
- [Scenarios](scenarios.md) whose car speeds R5's 16-bit centimetres per
  second would wrap around or bring to 0 or less (a negative factor or
  speed in `newCarspeeds`, a polygon `scale` of `Infinity` or 0, a
  `carspeedScale` far above 1), and a `carspeedScale` beyond the range of
  a 32-bit float, are errors; r5r routes on the wrapped speeds, hands R5
  `Infinity`, or fails dividing by zero. Polygons that are not valid, and
  ties between polygons that peabiru cannot settle exactly as R5 does, are
  errors too.

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

<!-- diff:refuse-itinerary-cases -->
- `detailedItineraries()`: the few pairs whose itinerary in r5r depends on
  something peabiru does not reproduce are an error naming the pair
  (`not supported: ...`) rather than a different itinerary: a car search
  that crosses a turn restriction with two or more via ways in a way that
  matters (R5 reverses those via edges in place, so r5r's result depends
  on the searches that ran before), and a ride along a GTFS shape cut by
  `shape_dist_traveled`. See
  [Detailed itineraries](itineraries.md#pairs-that-are-refused).

### Output details

<!-- diff:write-fare-zip -->
- `writeFareStructure()` writes the same CSV files as r5r's
  `write_fare_structure()`, stored uncompressed in a `.zip`, so the bytes
  of the archive differ from r5r's (which also holds file dates); each
  package reads the other's files. As in r5r 2.5.0, `debugPath` and
  `debugInfo` are kept in the structure and no debug file is written.

<!-- diff:isochrone-make-valid -->
- `isochrone()` returns the polygons r5r hands to `sf::st_make_valid()`,
  without that last rebuild with s2: they cover the same areas but their
  vertices are not snapped to 1e-7 degrees, the rings of one polygon may
  touch at a vertex, and they are always `MultiPolygon`s. (The R package
  returns r5r's `sf` exactly.)

<!-- diff:platform-order -->
- r5r loads the GTFS feeds of its directory in the order the operating
  system lists it (see [Feed order](#feed-order)); here the order is that
  of the `gtfs` array. r5r's isochrone rings start where isoband, built
  with the platform's C++ library, puts them (libc++ on macOS, libstdc++
  on Linux and Windows); the JavaScript package gives the macOS order on
  every platform, and its polygon coordinates can differ from the native
  packages' in the last digits (about 1e-14 degrees), because WebAssembly
  has its own `sinh` and `atan`. The line isochrones are identical.

### What only JavaScript does differently

- `outputDir` (writing results to CSV files) has no file system to write
  to: any value but `null` throws `UnsupportedError`.
- `getGtfsErrors()` takes the feeds' bytes, as `buildNetwork()` does,
  since a network does not keep them; `readFareStructure()` takes the
  bytes of a `.zip` file and `writeFareStructure()` returns them.
- `nThreads` is a number of workers, each with its own copy of the
  network; an `AbortSignal` in `signal` stops a call, where R and Python
  stop on Ctrl-C.
- Dates and times can be `Date`s, read in the runtime's local time zone;
  strings `"YYYY-MM-DD HH:MM:SS"` are read as they are written.
- An option that is not one of the function's options is an error, as an
  unknown argument is in R and Python.
