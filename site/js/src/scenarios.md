# Scenarios

Three options change the street network for one call, without building it
again: car speeds by way or by area (`newCarspeeds`), a factor for every
other car speed (`carspeedScale`), and the cyclists' levels of traffic
stress (`newLts`). They are taken by `travelTimeMatrix()`,
`accessibility()`, `expandedTravelTimeMatrix()`,
`arrivalTravelTimeMatrix()` and `detailedItineraries()`.

The examples compare the mean travel time between the 27 points of the São
Paulo sample under each scenario:

```js
const network = await peabiru.buildNetwork({ pbf: await readFile("spo_osm.pbf") });
```

## Car speeds

`carspeedScale` multiplies the car speed of every edge that `newCarspeeds`
leaves out (all of them when there is no `newCarspeeds`); it is 1 by
default:

```js
{{#include ../examples/scenarios.mjs:scale}}
```

```text
{{#include ../examples/output/scenarios.txt:1:2}}
```

`newCarspeeds` as a table (an object of columns, or an array of row
objects) changes the speed of the edges of the OpenStreetMap ways it
lists, with columns `osm_id`, `max_speed` and `speed_type`, the same for
every row: `"km/h"` (the speed itself; 0 closes the way to cars) or
`"scale"` (a factor for the way's car speed). The way ids can come from
the network itself, with [`streetNetworkToSf()`](network.md):

```js
{{#include ../examples/scenarios.mjs:ways}}
```

```text
{{#include ../examples/output/scenarios.txt:3:5}}
```

Closing ways leaves some pairs unreachable, which leave the matrix (here 651
of 729 pairs remain).

`newCarspeeds` can also be a GeoJSON `FeatureCollection` of `Polygon`s or
`MultiPolygon`s (WGS 84) with properties `poly_id` (a string), `scale` (a
number) and `priority` (an integer): each street edge takes the `scale` of
the polygon it meets with the highest priority (on a tie, the one it
overlaps most):

```js
{{#include ../examples/scenarios.mjs:polygon}}
```

```text
{{#include ../examples/output/scenarios.txt:6}}
```

R5 stores car speeds as 16-bit integers of centimetres per second, so a
factor or speed that would come out as 0 or less there (a negative factor
or speed, a polygon `scale` of `Infinity` or 0, a `carspeedScale` far above
1) is an error, as are polygons that are not valid and ties between
polygons that peabiru cannot settle exactly as R5 does.

## Levels of traffic stress

Cyclists ride on streets up to `maxLts` (2 by default) of the four levels
of traffic stress R5 gives every street. `newLts` changes those levels: as
a table with columns `osm_id` and `lts` (an integer from 1 to 4), for the
edges of the ways it lists; or as a GeoJSON `FeatureCollection` of
`LineString`s with properties `line_id`, `lts` (1 to 4) and `priority`,
where each street edge takes the level of the line closest to it among
those whose bounding box meets its own (`priority` is not used).

```js
{{#include ../examples/scenarios.mjs:lts}}
```

```text
{{#include ../examples/output/scenarios.txt:7:}}
```

## What a scenario changes

A scenario changes the street search from the origins: the trip without
public transport, and the way to the stops by bicycle or car. It does not
change the trip from the last stop to the destination. On a network built
with an [elevation raster](elevation.md), a call with a scenario routes
without the slope costs (R5's copy of the network for the scenario has
none), with a warning that says so.
