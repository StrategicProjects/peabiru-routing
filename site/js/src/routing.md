# Routing functions

| Function | What it computes |
|---|---|
| [`travelTimeMatrix()`](api.md#travelTimeMatrix) | travel times between every origin and destination |
| [`accessibility()`](api.md#accessibility) | cumulative or decay-weighted access to opportunities |
| [`expandedTravelTimeMatrix()`](api.md#expandedTravelTimeMatrix) | travel times minute by minute, with a breakdown of each trip |
| [`arrivalTravelTimeMatrix()`](api.md#arrivalTravelTimeMatrix) | the latest trip that arrives by a given time |

Each takes the network, the origins, the destinations and an options
object, and returns a Promise of an object of columns.

## Arguments

The options are r5r's arguments, with r5r's defaults, in camelCase:
`departure_datetime` is `departureDatetime`, `max_trip_duration` is
`maxTripDuration`, and so on (all of them are in the
[API reference](api.md#RoutingOptions)). An option name that is not one of
them is an error, so a typo does not go unnoticed.

- **Points** are columns `{ id, lon, lat }` (arrays or typed arrays) or an
  array of `{ id, lon, lat }` objects. For `accessibility()` the
  destinations also hold the opportunity columns. Ids that are not strings
  are converted with `String()`, with a warning; points with missing
  coordinates are not routed, with a warning; duplicated ids are an error.
- **Dates and times** are strings `"YYYY-MM-DD HH:MM:SS"` (or with `T`, or
  without seconds), read as they are written: the clock time of the GTFS
  feeds. A `Date` also works, but it is read in the local time zone of the
  JavaScript runtime, which is rarely the feeds' time zone: prefer strings.
- **Frequency-based GTFS** (`frequencies.txt`) is routed with Monte Carlo
  draws: results are reproducible, and equal to r5r's, only with a `seed`.
- `onWarning` receives the warnings that r5r would give (by default
  `console.warn`); `signal` takes an `AbortSignal` and `nThreads` the
  number of workers to use (see [Threads and workers](threads.md)).

## Results

Results are objects of columns, named as r5r names them, never arrays of
row objects (a matrix can have millions of rows). The id columns are arrays
of strings; the numbers are typed arrays:

| Function | Columns |
|---|---|
| `travelTimeMatrix()` | `from_id`, `to_id`; one `Int32Array` `travel_time_pXX` per percentile (`travel_time_p05`, ...), in whole minutes |
| `accessibility()` | `id`, `opportunity`; `percentile` and `cutoff` (`Int32Array`; no `cutoff` with `"fixed_exponential"`); `accessibility` (`Float64Array`) |
| `expandedTravelTimeMatrix()`, `arrivalTravelTimeMatrix()` | `from_id`, `to_id`, `departure_time` (`"HH:MM:SS"`), `draw_number` (`Int32Array`); with `breakdown: true`, `access_time`, `wait_time`, `ride_time`, `transfer_time`, `egress_time` (`Float64Array`, minutes); `routes` (strings); `n_rides` (`Int32Array`, with `breakdown`); `total_time` (`Float64Array`) |

Rows are origin-major, in the order of the inputs; pairs that are not
reached are left out, as in r5r. Missing values are
[`NA_INT32`](api.md#NA_INT32) (-2147483648) in the `Int32Array` columns,
`NaN` in the `Float64Array` columns and `null` in the string columns.

## Street matrices

A network built from the `.pbf` alone routes on the streets. Walking,
cycling (with the cyclist's highest level of traffic stress, `maxLts`) and
driving (with turn restrictions and turn costs):

```js
{{#include ../examples/street-matrix.mjs:street}}
```

```text
{{#include ../examples/output/street-matrix.txt:1}}
```

To find one pair, walk the rows; for many lookups, build a `Map` from the
ids once:

```js
{{#include ../examples/street-matrix.mjs:lookup}}
```

```text
{{#include ../examples/output/street-matrix.txt:2}}
```

The first street matrices of a mode on a network search the streets from
every origin; once enough origins have been routed, the network builds a
contraction hierarchy for that mode, keeps it, and later matrices use it.
The results are the same either way; only the time changes.

## Public transport matrices

`mode: ["WALK", "TRANSIT"]` walks to a stop, rides up to `maxRides`
vehicles and walks to the destination, for every minute of the departure
window (`departureDatetime` and the `timeWindow` minutes after it). Each
`travel_time_pXX` column is a percentile of the travel times over the
window and, for frequency-based trips, over the Monte Carlo draws:

```js
{{#include ../examples/transit-matrix.mjs:transit}}
```

```text
{{#include ../examples/output/transit-matrix.txt:1}}
```

With a `seed` the draws repeat; without one, each call draws a new seed and
the results of a frequency-based feed change from call to call:

```js
{{#include ../examples/transit-matrix.mjs:seed}}
```

```text
{{#include ../examples/output/transit-matrix.txt:2}}
```

## Accessibility

`accessibility()` sums, for each origin, the opportunities of the
destinations (the columns named in `opportunitiesColnames`), weighted by a
decay function of the travel time: `"step"` (cumulative, within each
cutoff), `"exponential"`, `"fixed_exponential"`, `"linear"` or
`"logistic"`:

```js
{{#include ../examples/accessibility.mjs:access}}
```

```text
{{#include ../examples/output/accessibility.txt:1:5}}
```

```js
{{#include ../examples/accessibility.mjs:decay}}
```

```text
{{#include ../examples/output/accessibility.txt:6}}
```

## Expanded and arrival matrices

`expandedTravelTimeMatrix()` gives one row per destination, departure
minute and draw instead of percentiles, and with `breakdown: true` the
parts of each trip. Times are minutes with one decimal:

```js
{{#include ../examples/expanded.mjs:expanded}}
```

```text
{{#include ../examples/output/expanded.txt:1:4}}
```

`arrivalTravelTimeMatrix()` searches the departures back from
`arrivalDatetime` and keeps, for each pair, the latest trip that arrives in
time:

```js
{{#include ../examples/expanded.mjs:arrival}}
```

```text
{{#include ../examples/output/expanded.txt:5:}}
```

Both need public transport; with a street mode alone they throw
`UnsupportedError`.

## Warnings and errors

```js
{{#include ../examples/errors.mjs:warnings}}
```

```js
{{#include ../examples/errors.mjs:errors}}
```

```text
{{#include ../examples/output/errors.txt}}
```

What peabiru does not support yet throws
[`UnsupportedError`](api.md#UnsupportedError): `outputDir`, and expanded and
arrival matrices by a street mode alone. Elevation is not supported
either: `buildNetwork()` takes no elevation raster. `verbose` and
`progress` are checked as r5r checks them; peabiru prints nothing.
