# Fares

A *fare structure* describes how much each ride costs and which transfers
are discounted, as r5r 2.5.0's does; the routing functions take it as
`fareStructure`, and the results are r5r's. With one, public transport is
routed with R5's multi-criteria router (McRAPTOR), which keeps at each stop
the trips that are faster or cheaper than the others, over
`timeWindow * drawsPerMinute` departure times sampled in the window.

| Function | What it does |
|---|---|
| [`setupFareStructure()`](api.md#setupFareStructure) | a structure where every route, type and transfer costs the same |
| [`readFareStructure()`](api.md#readFareStructure) | a structure from the bytes of a `.zip` file (r5r's or peabiru's) |
| [`writeFareStructure()`](api.md#writeFareStructure) | the bytes of a `.zip` file that r5r reads too |
| [`paretoFrontier()`](api.md#paretoFrontier) | the travel time at each fare cutoff where it changes |

The structure is r5r's object, with its tables as objects of columns:
`fares_per_type` (one row per fare type), `fares_per_transfer` (the fare of
two rides together, for the pairs of types with a discount) and
`fares_per_route` (the fare type of each GTFS route, matched by `agency_id`
and `route_id`; a route missing from it is an error, as in r5r).

```js
{{#include ../examples/fares.mjs:structure}}
```

`fareStructure` and `maxFare` (the highest fare of a trip) go to
`travelTimeMatrix()`, `accessibility()` and `detailedItineraries()`, whose
rows then get `total_fare` and `cumulative_fare`:

```js
{{#include ../examples/fares.mjs:routing}}
```

```text
{{#include ../examples/output/fares.txt}}
```

In this sample, whose GTFS is frequency-based, r5r's fare router finds no
trip faster than walking (its results are the walking times, and peabiru's
are the same), so the frontier has one row per pair. With scheduled feeds,
such as r5r's Porto Alegre sample, the frontier lists the faster trips that
cost more.

```js
{{#include ../examples/fares.mjs:files}}
```

`monetary_cost` is the cutoff as R5 reads it, a Java `float` (4.4 is
4.400000095367432), as r5r reports it.

## Trip options

To see every trip worth considering between two points, faster or cheaper, with fewer transfers or less walking, and not only the fastest one, see `tripOptions()`, peabiru's own multi-criteria planner: [Trip options](trip-options.md).
