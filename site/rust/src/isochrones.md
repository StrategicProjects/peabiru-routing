# Isochrones

<img src="../assets/figures/isochrones-by-mode.svg" alt="Four maps of Porto Alegre, one per mode, with the areas reached from Farroupilha Park within 10, 20 and 30 minutes: a small area walking, a few kilometres around the park cycling, most of the map driving, and long arms along the bus corridors with public transport." width="576" height="662">

An isochrone is made in two steps, as in r5r's `isochrone()`. First a
**travel-time surface**: the time from each origin to every cell of a Web
Mercator grid laid over the network (zoom 9 to 12; at zoom 12 a cell is
about 38 m wide at the equator), by street or by public transport, at each
percentile. Then **polygons**: for each cutoff, the area of the cells within
it, traced with the same algorithm as the R package isoband.

```rust,ignore
pub fn street_travel_time_surface(
    network: &Network,
    origins: &[(f64, f64)],
    mode: Mode,
    params: &RouteParams,
    percentiles: &[i32],
    zoom: i32,
) -> Result<TravelTimeSurface, RouteError>

pub fn transit_travel_time_surface(
    network: &Network,
    origins: &[(f64, f64)],
    p: &TransitParams,
    zoom: i32,
) -> Result<TravelTimeSurface, RouteError>

pub fn isochrone_polygons(
    surface: &TravelTimeSurface,
    cutoffs: &[f64],
) -> Result<Vec<IsochronePolygon>, RouteError>
```

Each `IsochronePolygon` holds the index of its origin, its percentile, its
cutoff (`isochrone`, minutes) and its `polygons`: each one a shell followed
by its holes, every ring closed, with `[lon, lat]` coordinates. They come by
origin, percentile ascending and cutoff descending; a cutoff that reaches
nothing has no polygons. The R and Python packages make an `sf` or
`GeoDataFrame` of them; the JavaScript package, GeoJSON. The order of the
rings and the vertex each ring starts at are those isoband gives on the
platform the crate is compiled for (isoband walks a C++ hash table: libc++
order on Apple targets and WebAssembly, libstdc++ order elsewhere); see
[Parity](parity.md).

r5r routes polygon isochrones up to the largest cutoff plus 10 minutes,
whatever `max_trip_duration` says; `isochrone_trip_limit(cutoffs)` gives the
largest cutoff (minutes, truncated) and the packages add the 10.

For the street segments reached instead of areas, `street_edges` lists the
segments of the network and `isochrone_lines`, given the travel time from
each origin to each segment, keeps the segments within the cutoffs and
gives each one the band of the smallest cutoff at or above its time (the R,
Python and JavaScript packages do it with `polygon_output = FALSE`).

## Example

Isochrones of 5, 10 and 15 minutes from one point of the São Paulo sample,
by each mode, with their areas:

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/isochrone.rs}}
```

```text
Walk     5 min 0.09 · 10 min 0.60 · 15 min 1.28 km²
Bicycle  5 min 0.36 · 10 min 1.40 · 15 min 3.25 km²
Car      5 min 0.79 · 10 min 5.20 · 15 min 8.52 km²
Transit  5 min 0.09 · 10 min 0.60 · 15 min 1.28 km²
```

From this point, a quarter of an hour is too short for the buses and the
metro to beat walking; the map above, of Porto Alegre with cutoffs up to 30
minutes, shows where they part. The same numbers come from the
[JavaScript example](../js/examples.html), and the R and Python packages
give the same polygons.
