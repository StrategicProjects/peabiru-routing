# Examples: isochrones and a map in the browser

## Isochrones by mode, as GeoJSON

[`isochrone()`](api.md#isochrone) returns a GeoJSON `FeatureCollection`: one
`MultiPolygon` per origin and cutoff, largest first, ready for a web map,
QGIS, geopandas or sf. This script computes the isochrones of the four modes
from one point of the São Paulo sample, prints their areas, and writes them
to one file. GeoJSON has no area function, so a small one is included (a
flat approximation, fine at the scale of a city):

```js
{{#include ../examples/isochrones.mjs:area}}

{{#include ../examples/isochrones.mjs:polygons}}
```

```text
{{#include ../examples/output/isochrones.txt:1:4}}
```

From this point, 15 minutes is too short for public transport to beat
walking, so the two give the same isochrones; with longer cutoffs, or on the
Porto Alegre network below, they part.

With `polygonOutput: false` the result is the street segments reached
instead, each with its travel time, street class and length:

```js
{{#include ../examples/isochrones.mjs:lines}}
```

```text
{{#include ../examples/output/isochrones.txt:5:7}}
```

The other packages give the same polygons; drawn for Porto Alegre, from the
[R example](../r/articles/isochrones.html):

<figure>
  <img src="../assets/figures/isochrones-by-mode.svg" alt="Four maps of Porto Alegre, one per mode, with the areas reached from Farroupilha Park within 10, 20 and 30 minutes: a small area walking, a few kilometres around the park cycling, most of the map driving, and long arms along the bus corridors with public transport." style="max-width: 100%; height: auto;">
</figure>

## An isochrone map in the browser

The [isochrone demo](../demo/) of this site is a single page that builds a
network in the browser and draws isochrones from wherever you click, without
a server: the routing never leaves the page. Its code,
[`site/demo/demo.mjs`](https://github.com/StrategicProjects/peabiru-routing/blob/main/site/demo/demo.mjs),
has three parts that matter.

<figure>
  <a href="../demo/"><img src="../assets/figures/demo-poa.webp" alt="The isochrone demo page: a map of Porto Alegre with three nested isochrones, 10, 20 and 30 minutes by walking and public transport, stretching along the bus corridors from the clicked point." width="1000" height="651" style="max-width: 100%; height: auto;"></a>
  <figcaption>The demo with the Porto Alegre network, walking and public transport at 14:00.</figcaption>
</figure>

**Loading a network from URLs.** The page imports the package as an ES
module (here from a copy next to the page; from a CDN or a bundler it is
`import * as peabiru from "peabiru"`), fetches the files and passes their
bytes to `buildNetwork()`:

```js
{{#include ../../demo/demo.mjs:load}}
```

**The streets, from the network itself.** The page can show a background
map (tiles of OpenFreeMap, to recognise the city), but it does not need one:
switched off, the map is every street segment a car reaches from the centre
within two hours, which on a city extract is every street, with its class.
Over the background map the same lines show, faintly, where the network
ends:

```js
{{#include ../../demo/demo.mjs:streets}}
```

**One isochrone per click.** The clicked point becomes the origin. For
public transport the feeds' date and the chosen time make the departure;
`seed` keeps the frequency-based São Paulo feed reproducible:

```js
{{#include ../../demo/demo.mjs:isochrone}}
```

The rest of the file hands the GeoJSON to [MapLibre GL JS](https://maplibre.org/)
as two layers, the streets and the isochrones, placed over the land and the
buildings of the background map and under its roads and labels, so the map
can be dragged and zoomed. Everything runs on the page's thread; for long computations,
`buildNetwork({ useWorkers: true })` moves the routing to workers (see
[Threads and workers](threads.md)).

**Cities without public transport data.** Besides the two samples, the demo
has the streets of Recife and Caruaru, in Pernambuco, built from open data
(see the [R](../r/articles/recife-caruaru.html) and
[Python](../python/example-recife-caruaru/) examples for how the extracts
are cut). No open GTFS feed was found for either, so their entries pass only
the `.pbf` file to `buildNetwork()`, and the page hides public transport and
the departure time for them. The extracts are Recife's municipality and
Caruaru's urban area, without buildings and without OpenStreetMap's metadata
(2.8 MB and 0.55 MB): peabiru reads neither, and on these cities the travel times are the
same as with the full extract of the same area (the other ways are kept,
since their nodes split the streets).
