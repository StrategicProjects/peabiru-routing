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
a server and without map tiles. Its code,
[`site/demo/demo.mjs`](https://github.com/StrategicProjects/peabiru-routing/blob/main/site/demo/demo.mjs),
has three parts that matter.

<figure>
  <a href="../demo/"><img src="../assets/figures/demo-poa.webp" alt="The isochrone demo page: a map of the streets of Porto Alegre with three nested isochrones, 10, 20 and 30 minutes by walking and public transport, stretching along the bus corridors from the clicked point." width="1000" height="615" style="max-width: 100%; height: auto;"></a>
  <figcaption>The demo with the Porto Alegre network, walking and public transport at 14:00.</figcaption>
</figure>

**Loading a network from URLs.** The page imports the package as an ES
module (here from a copy next to the page; from a CDN or a bundler it is
`import * as peabiru from "peabiru"`), fetches the files and passes their
bytes to `buildNetwork()`:

```js
{{#include ../../demo/demo.mjs:load}}
```

**The streets, from the network itself.** Instead of map tiles, the
background is every street segment a car reaches from the centre within two
hours, which on a city extract is every street, with its class:

```js
{{#include ../../demo/demo.mjs:streets}}
```

**One isochrone per click.** The clicked point becomes the origin. For
public transport the feeds' date and the chosen time make the departure;
`seed` keeps the frequency-based São Paulo feed reproducible:

```js
{{#include ../../demo/demo.mjs:isochrone}}
```

The rest of the file projects longitude and latitude to Web Mercator and
draws the lines and the polygons (with the even-odd rule, for the holes) on
a `<canvas>`. Everything runs on the page's thread; for long computations,
`buildNetwork({ useWorkers: true })` moves the routing to workers (see
[Threads and workers](threads.md)).
