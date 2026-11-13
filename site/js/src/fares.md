# Fares

A *fare structure* describes how much each ride costs and which transfers
are discounted; the routing functions take it as
`fareStructure`. With one, public transport is
routed with the multi-criteria router (McRAPTOR), which keeps at each stop
the trips that are faster or cheaper than the others, over
`timeWindow * drawsPerMinute` departure times sampled in the window.

| Function | What it does |
|---|---|
| [`setupFareStructure()`](api.md#setupFareStructure) | a structure where every route, type and transfer costs the same |
| [`readFareStructure()`](api.md#readFareStructure) | a structure from the bytes of a `.zip` file |
| [`writeFareStructure()`](api.md#writeFareStructure) | the bytes of a `.zip` file, the same format in the R and Python packages |
| [`paretoFrontier()`](api.md#paretoFrontier) | the travel time at each fare cutoff where it changes |

The structure is an object with its tables as objects of columns:
`fares_per_type` (one row per fare type), `fares_per_transfer` (the fare of
two rides together, for the pairs of types with a discount) and
`fares_per_route` (the fare type of each GTFS route, matched by `agency_id`
and `route_id`; a route missing from it is an error).

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

In this sample, whose GTFS is frequency-based, the fare router finds no
trip faster than walking (the results are the walking times), so the
frontier has one row per pair. With scheduled feeds, such as the Porto
Alegre sample, the frontier lists the faster trips that
cost more.

```js
{{#include ../examples/fares.mjs:files}}
```

`monetary_cost` is the cutoff as the router reads it, a 32-bit float (4.4 is
4.400000095367432).

## Trip options

To see every trip worth considering between two points, faster or cheaper, with fewer transfers or less walking, and not only the fastest one, see `tripOptions()`, peabiru's own multi-criteria planner: [Trip options](trip-options.md).
