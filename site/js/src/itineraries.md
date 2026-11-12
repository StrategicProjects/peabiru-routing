# Detailed itineraries

[`detailedItineraries()`](api.md#detailedItineraries) gives the legs of
each trip between pairs of origins and destinations: each walk, ride by
bicycle or car, and each public transport ride, with its duration, waiting
time, distance, route and line.

The examples on this page use the sample data of Porto Alegre from the r5r
project, whose GTFS feeds have timetables (itineraries by public transport
need them: a network with frequency-based trips is an error) and only have
service in the afternoon, hence the 14:00 departure. The files are
downloaded once into the temporary directory (about 2 MB) by a small
helper:

```js
{{#include ../examples/sample-poa.mjs:poaFile}}
```

```js
{{#include ../examples/itineraries.mjs:network}}
```

## The fastest trip

Origins and destinations are paired row by row (a single origin or
destination is repeated to match the other, with a warning). Each pair gets
its fastest itinerary (`shortestPath: true`, the default): the trip by the
street mode alone, or the fastest public transport trip over the departure
window (`departureDatetime` and the `timeWindow` minutes after it),
whichever is faster. Pairs without an itinerary have no rows.

```js
{{#include ../examples/itineraries.mjs:fastest}}
```

```text
{{#include ../examples/output/itineraries.txt:1:10}}
```

The result is a GeoJSON `FeatureCollection` with one `LineString` feature
per leg (coordinates `[lon, lat]`, WGS 84); the properties are the row's
columns. With `dropGeometry: true` it is an array of the rows themselves.
The columns:

| Column | |
|---|---|
| `from_id`, `from_lat`, `from_lon`, `to_id`, `to_lat`, `to_lon` | the pair |
| `option` | the itinerary, from 1 |
| `departure_time` | `"HH:MM:SS"`, when the itinerary leaves the origin |
| `total_duration`, `total_distance` | of the itinerary: minutes (one decimal), metres |
| `total_fare` | with a `fareStructure` only |
| `segment` | the leg, from 1 |
| `mode` | `"WALK"`, `"BICYCLE"`, `"CAR"`, or the ride's mode (`"BUS"`, `"RAIL"`, ...) |
| `cumulative_fare` | with a `fareStructure` only: what was paid up to the end of the leg |
| `segment_duration`, `wait`, `distance` | of the leg: minutes, minutes, metres |
| `route` | the GTFS `route_id` of a ride; empty for a street leg |
| `osm_id_list`, `edge_id_list`, `board_stop_id`, `alight_stop_id` | with `osmLinkIds: true` only (see below) |

The lines are drawn as R5 draws them:

- a street leg's line holds the whole of its first and last street
  segments, not only the parts walked or driven; the leg from the last stop
  ends at the destination itself;
- a ride follows the GTFS shape of its trip pattern, or straight lines
  between its stops when the feed has no shapes;
- a walk between stops is routed again on the streets for its line and
  distance; its duration is the one the public transport search used.

## Alternatives

With `shortestPath: false` the result lists the itineraries R5's
multi-criteria router (McRAPTOR) keeps over the departure window, one per
sequence of routes, with the trip by the street mode alone first:

```js
{{#include ../examples/itineraries.mjs:alternatives}}
```

```text
{{#include ../examples/output/itineraries.txt:11:18}}
```

`suboptimalMinutes` widens the search: at every stop, McRAPTOR also keeps
the states that arrive up to that many minutes after the best one there, so
more sequences of routes survive. It does not cap an alternative's
duration at the best plus `suboptimalMinutes`, since each departure minute
of the window is searched on its own. It must be 0 with
`shortestPath: true` and with a `fareStructure`.

The options are numbered in this order: the trip by the street mode
alone first, then by the number of legs, then by duration. The first option
is then not the fastest when a trip with fewer legs is slower (on Porto
Alegre with `suboptimalMinutes: 10`, a pair had 102 options, the first of
60.4 minutes and the fastest of 53.3).

## Every origin to every destination

`allToAll: true` routes every origin to every destination instead of
pairing them:

```js
{{#include ../examples/itineraries.mjs:allToAll}}
```

```text
{{#include ../examples/output/itineraries.txt:19:25}}
```

## The OpenStreetMap ways of a leg

`osmLinkIds: true` (which needs the geometry, so not with `dropGeometry`)
adds four properties: `osm_id_list`, the OpenStreetMap way ids of a street
leg's edges, and `edge_id_list`, the indices of those edges in the network
(the `edge_index` of [`streetNetworkToSf()`](network.md)), both as text
`"[id1, id2, ...]"` (`"[]"` for a ride; consecutive edges on a roundabout
give only their first ids, and the connectors to stops none); and, for a
ride, `board_stop_id` and `alight_stop_id`, `"<feed id>:<stop_id>"` (empty
for street legs).

```js
{{#include ../examples/itineraries.mjs:osmLinkIds}}
```

```text
{{#include ../examples/output/itineraries.txt:26:27}}
```

## Fares

With a [fare structure](fares.md), trips are routed with fares (only those
up to `maxFare`, `Infinity` by default) and the rows get `total_fare` and
`cumulative_fare`:

```js
{{#include ../examples/itineraries.mjs:fares}}
```

```text
{{#include ../examples/output/itineraries.txt:28:}}
```

## Car egress and turn restrictions

With `modeEgress: "CAR"`, R5 reverses the via edges of turn restrictions
with two or more via ways in place each time its backward egress search
checks one, so what R5 returns for a pair whose car searches cross such a
restriction can depend on the searches that ran before it (in the same
call, on other threads, or in earlier calls). peabiru returns, for every
pair, the itineraries of a freshly built network for that pair alone with
one thread, and warns once per call (`onWarning`), naming the pairs whose
itineraries some other order of those via edges would change (on Porto
Alegre, about 1 pair in 200). Rides along GTFS shapes cut by
`shape_dist_traveled` follow R5's cut.

The trips for which R5's fastest-path search cannot rebuild a path are
routed again with McRAPTOR.

## Trip options

To see every trip worth considering between two points, faster or cheaper, with fewer transfers or less walking, and not only the fastest one, see `tripOptions()`, peabiru's own multi-criteria planner: [Trip options](trip-options.md).
