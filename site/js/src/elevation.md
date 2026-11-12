# Elevation

With an elevation raster, walking and cycling times get slope costs: the
same numbers r5r 2.5.0 gives when its data directory holds a `.tif` file.
Pass the bytes of the GeoTIFF file as `tif` to
[`buildNetwork()`](api.md#buildNetwork), and choose the cost function with
`elevation`:

- `"TOBLER"` (the default): Tobler's hiking function;
- `"MINETTI"`: Minetti's energy cost of walking;
- `"NONE"`: the raster is read but no slope cost is applied.

Case does not matter. Without `tif` the function is `"NONE"`, whatever
`elevation` says. As in R5, each directed street edge gets one factor (the
function averaged over samples every 10 m along it), which multiplies the
time of walking or cycling along the whole edge; driving times never
change.

The rasters in these examples are synthetic hills over the São Paulo
sample, made for the tests of the package, not measured elevations.

```js
{{#include ../examples/elevation.mjs:build}}
```

```text
{{#include ../examples/output/elevation.txt:1}}
```

```js
{{#include ../examples/elevation.mjs:times}}
```

```text
{{#include ../examples/output/elevation.txt:2:3}}
```

`network.info().elevation` tells which function a network uses. A network
built with a raster also stops `travelTimeMatrix()` from routing walking
trips from the destinations when there are more origins than destinations
(with slopes, a walk takes a different time each way), and a call with a
[scenario](scenarios.md) routes without the slope costs, with a warning.

## Rasters

The raster is a GeoTIFF of one band, north up, uncompressed or compressed
with LZW, Deflate or PackBits, in one of these coordinate reference
systems, given by its EPSG code in the file:

- WGS 84 longitude and latitude (EPSG:4326);
- SIRGAS 2000 (4674), SAD69 (4618), Corrego Alegre 1970-72 (4225), and
  their UTM zones used in Brazil (31965-31985, 6210, 6211, 5396;
  29168-29172, 29187-29195; 22521-22525);
- SIRGAS 2000 / Brazil Polyconic (5880);
- WGS 84 / UTM (32601-32660, 32701-32760);
- WGS 84 / Pseudo-Mercator (3857).

These are the systems whose transforms peabiru reproduces exactly as R5's
GeoTools computes them; any other raster is an error that says why (as is
a raster that is not a GeoTIFF). Reprojecting a raster to EPSG:4326 (with
GDAL's `gdalwarp -t_srs EPSG:4326`, or `terra::project()` in R) makes any
source usable.

```js
{{#include ../examples/elevation.mjs:crs}}
```

```text
{{#include ../examples/output/elevation.txt:4}}
```

In a projected raster, a street point within nanometres (5e-8 m, or
2e-13 degrees) of the boundary between two pixels, or of the raster's
edge, falls on one side or the other depending on the last bits of a
sine or cosine. r5r's result there depends on the machine it runs on;
peabiru reads the pixel that Java's `StrictMath` gives, the same on every
machine, and warns once with the number of such points (about one in a
hundred million for 30 m pixels). A point that `StrictMath` itself puts on
the raster's outer edge (inside its envelope, past the last pixel) is an
error, since r5r fails there on every machine and there is no pixel to
read.

## Voids and large values

R5 keeps elevations as 16-bit integers of decimetres, so values beyond
+/-3276.7 m wrap around: an SRTM void of -32768 becomes 0 m, -9999 becomes
3108.2 m, and a NaN no-data value is stored as 0 m. peabiru stores them as
R5 does, so the slope costs are r5r's, and warns (r5r is silent), counting
those samples and the ones equal to the raster's no-data value. The
warning goes to `onWarning` (by default `console.warn`) and stays with the
network, in `network.buildWarnings()`:

```js
{{#include ../examples/elevation.mjs:voids}}
```

```text
{{#include ../examples/output/elevation.txt:5:}}
```

Fill the voids, or crop the raster to the area of the network, before
building it.
