# Inspecting a network

These functions read a network without routing on it: where points snap to
the streets, the area the streets cover, the street and public transport
networks as GeoJSON, and the dates with service. Each returns a Promise.
They give the results of r5r 2.5.0's functions of the same names in
snake_case (`find_snap()`, `street_network_bbox()`, ...), with GeoJSON
where r5r gives `sf` tables.

```js
{{#include ../examples/inspect.mjs:setup}}
```

## Sizes and versions

`network.info()` gives the sizes of a network (synchronously):

```js
{{#include ../examples/network-info.mjs:info}}
```

```text
{{#include ../examples/output/network-info.txt}}
```

`vertices` and `edges` count every vertex and directed edge of the street
network, including the connectors of transit stops; `elevation` is the
slope cost function (see [Elevation](elevation.md)); `feeds` the GTFS feed
ids, in loading order; `stops`, `patterns` (trip patterns) and `services`
the sizes of the public transport network; `frequencies` whether any trip
is frequency-based. `network.r5rWarnings()` lists the warnings of the
build (about the elevation raster), and `network.workers` the number of
[workers](threads.md).

## Where points snap

[`findSnap()`](api.md#findSnap) gives, for each point, the nearest point of
a street that the mode (`"WALK"` by default, `"BICYCLE"` or `"CAR"`) may
use, first within 300 metres and then within `radius` (1600 metres by
default). A point that does not snap is not routed.

```js
{{#include ../examples/inspect.mjs:snap}}
```

```text
{{#include ../examples/output/inspect.txt:1:3}}
```

The columns are `point_id`, `lat`, `lon`, `snap_lat`, `snap_lon`,
`distance` (metres, haversine on R5's sphere of radius 6,367,450 m) and
`found`; `snap_lat`, `snap_lon` and `distance` are `NaN` where a point does
not snap.

## The bounding box

[`streetNetworkBbox()`](api.md#streetNetworkBbox) gives the envelope of the
street network, which covers every OpenStreetMap node of the ways the
streets were made from: as a GeoJSON `Polygon` (`"polygon"`, the default),
a GeoJSON bbox `[xmin, ymin, xmax, ymax]` (`"bbox"`) or an object
(`"vector"`).

```js
{{#include ../examples/inspect.mjs:bbox}}
```

```text
{{#include ../examples/output/inspect.txt:4:11}}
```

## The street network

[`streetNetworkToSf()`](api.md#streetNetworkToSf) gives two GeoJSON
`FeatureCollection`s: `vertices`, the `Point`s at the ends of street edges
(with `index`, `park_and_ride` and `bike_sharing`, always false), and
`edges`, one `LineString` per directed street edge, the connectors of
transit stops and park-and-rides left out:

```js
{{#include ../examples/inspect.mjs:streets}}
```

```text
{{#include ../examples/output/inspect.txt:12:25}}
```

`length` is in metres and `car_speed` in km/h; `walk`, `car` and `bicycle`
say which modes may use the edge, and `bicycle_lts` is its level of traffic
stress (see [Scenarios](scenarios.md)). `osm_id` is the OpenStreetMap way
the edge comes from.

## The public transport network

[`transitNetworkToSf()`](api.md#transitNetworkToSf) gives `stops` (a
`Point` per stop, with `stop_index`, `stop_id` prefixed by the feed id,
`stop_name` and `linked_to_street`; a stop not linked to the streets has a
`null` geometry) and `routes`, one feature per trip pattern, with
`agency_id`, `agency_name`, `route_id`, `long_name`, `short_name` and
`mode`. A route's line is the GTFS shape of the trip that made the pattern
or, without one, the positions of its linked stops; patterns without a line
are left out, and a line whose points are all the same is a `Point`.

```js
{{#include ../examples/inspect.mjs:transit}}
```

```text
{{#include ../examples/output/inspect.txt:26:40}}
```

## Dates with service

[`checkTransitAvailability()`](api.md#checkTransitAvailability) counts the
services active on each date: given as `dates` (`"YYYY-MM-DD"` strings or
`Date`s, read in local time), as a range from `startDate` to `endDate`, or,
with neither, the dates on which some trip runs, found from the network.
`total_services` counts every service of the feeds, used by trips or not.
The routing functions make the same check on the departure date: none
active is an error, fewer than 20% a warning.

```js
{{#include ../examples/inspect.mjs:availability}}
```

```text
{{#include ../examples/output/inspect.txt:41:}}
```

`await network.transitAvailability(date)` gives the same two counts for one
date.

## GTFS errors

R5 reads GTFS feeds with gtfs-lib, which reports what it finds wrong in
them. [`getGtfsErrors()`](api.md#getGtfsErrors) reads the feeds again and
returns those errors, one row per error, feed by feed in the order
gtfs-lib finds them: `file` (the table, without `.txt`), `line` (the
record, the header being line 1; 0 for a whole table), `type` (gtfs-lib's
class of error), `field`, `id` (the key concerned, for duplicate keys and
suspect stop locations) and `priority` (`"HIGH"`, `"MEDIUM"`, `"LOW"` or
`"UNKNOWN"`). A `Network` does not keep its feeds, so pass them as
`buildNetwork()` takes them:

```js
{{#include ../examples/gtfs-errors.mjs:errors}}
```

```text
{{#include ../examples/output/gtfs-errors.txt:1:5}}
```

`buildNetwork()` refuses feeds with errors of priority `HIGH`, as r5r's
`build_network()` does: it rejects with an `Error` that names each feed and
its number of such errors, and `getGtfsErrors()` on the same feeds tells
which they are. Errors of lower priority do not stop the build.

```js
{{#include ../examples/gtfs-errors.mjs:refused}}
```

```text
{{#include ../examples/output/gtfs-errors.txt:6:}}
```

When gtfs-lib fails on a feed instead of listing errors (r5r's
`build_network()` fails then too), `getGtfsErrors()` rejects with an
`Error` naming the cause. GTFS-Flex zones (`locations.geojson`) and
location groups are read as gtfs-lib reads them, for its errors; on-demand
trips are not routed, as in r5r.
