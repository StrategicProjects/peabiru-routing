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

An option that is not one of the package's options is an error, as an
unknown argument is in R and Python.

## Every call is asynchronous

Every function returns a Promise, also on a network without workers:
`await` it. On the calling thread the computation itself still blocks the
thread while it runs (see [Threads and workers](threads.md)).

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

## What is the same

The options that are not supported are the same in the three packages, and
throw [`UnsupportedError`](api.md#UnsupportedError) here (an error of the
same name in Python, an error in R): fares, scenarios,
`outputDir`, and elevation. Missing coordinates, ids that are not strings
and duplicated ids are treated as in r5r, with the same warnings and
errors.
