# Coming from R or Python

The JavaScript package has the functions, arguments, defaults, checks and
result columns of the R and Python packages of peabiru, and gives the same
values: the engine is the same Rust core, compiled to WebAssembly.
`bench/compare/js_parity.mjs` in the repository compares its results with
those of the native command-line tool cell by cell, on the calling thread
and on a pool of workers. What differs is what JavaScript makes different.

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
| a result table (tibble, pandas `DataFrame`) | an object of columns: arrays of strings and typed arrays |
| an `sf` table / `GeoDataFrame` | a GeoJSON `FeatureCollection` |
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
| `output_dir`: one CSV file per origin | not available (no file system): any value but `null` throws `UnsupportedError` |

An option that is not one of the function's options is an error, as an
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

The R and Python packages load the GTFS feeds of a directory in the order
the operating system lists it; with more than one feed that order can
change a few results of expanded and arrival matrices (another stop or
route for some trips). Here the feeds load in the order of the `gtfs`
array. To reproduce a network built from a directory, pass the feeds in
the order that directory lists them: on macOS the sample directory of
Porto Alegre lists `poa_trensurb.zip` before `poa_eptc.zip`, and Linux
lists them the other way round. `network.info().feeds` gives the feed ids
in loading order.

A feed's `name` stands for its file name: it names the feed in messages
and, when the feed has no `feed_info.feed_id`, gives the feed id (the name
without `.zip`), which prefixes its stop ids.

## Errors

Argument errors read as those of the R package in JavaScript's terms
(`null`, `boolean`, `string`, `number`, `Date`, `Array`; positions counted
from 1; bullets and line breaks dropped), with the option names in
camelCase. The test suite checks 1,367 cases shared by the three packages
(`bench/compare/argument_errors.json` in the repository), which lists the
few cases worded differently on purpose: a number or boolean given as a
mode (`mode`, `modeEgress`, `by`) is reported by its type, where R reports
its text as not a mode; `arrivalDatetime` is checked itself (R checks the
departure, `arrival_datetime - max_trip_duration`); `allToAll` is named as
such; a missing `fareCutoffs` is the default; and `departureDatetime` also
takes a string, so a string that is not `"YYYY-MM-DD HH:MM:SS"` says what
it must be.

Errors are a `TypeError` (a value of the wrong type) or a `RangeError` (a
value out of range); what peabiru does not do is an
[`UnsupportedError`](api.md#UnsupportedError); what the engine finds wrong
in the data (a fare structure, a scenario, a GTFS feed, a raster) is an
`Error`. Errors raised in a worker reach the caller with the same class
and message.

## Dates and times

Dates and times can be `Date`s, read in the runtime's local time zone;
strings `"YYYY-MM-DD HH:MM:SS"` are read as they are written.

## Isochrone polygons

`isochrone()` returns the polygons as the contouring gives them, always
`MultiPolygon`s: they are not repaired or snapped to 1e-7 degrees, and the
rings of one polygon may touch at a vertex. Their coordinates can differ
from the native packages' in the last digits (about 1e-14 degrees),
because WebAssembly has its own `sinh` and `atan`. The line isochrones are
identical.
