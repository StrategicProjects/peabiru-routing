# Elevation

With an elevation raster, walking and cycling times depend on the slope of
each street.
Driving times do not change.

`BuildOptions::elevation` picks the slope cost function:

- `Elevation::None` (the default here; the packages' default is `"TOBLER"`
  when the data directory has a raster);
- `Elevation::Tobler`: Tobler's hiking function;
- `Elevation::Minetti`: Minetti's energy cost of walking.

`build_from_dir` reads the `.tif` (or `.tiff`) raster of the directory
when a model is set; `build_from_bytes_with_elevation` takes the raster's
bytes. `network.street().elevation()` tells which model a network was built
with.

## What is computed

When the network is built, every vertex is sampled at its coordinates and
every street segment of at least 10 m every 10 m along its line, at the
nearest pixel (no interpolation);
points outside the raster get 0 m. Each sample is kept in whole decimetres
in a 16-bit integer. Each directed edge gets a factor: the
slope cost averaged over its segments, weighted by their length, minus 1.
Walking or cycling along a whole edge takes `t + round(t * factor)` seconds
(at least 1); the partial edges at the origin and the destination keep their
plain times.

## Rasters

The raster is a GeoTIFF of one band of 8, 16 or 32-bit integers or 32 or
64-bit floats, uncompressed or compressed with LZW, Deflate or PackBits,
north up, georeferenced by a tie point and a pixel scale (what GDAL
writes). Its coordinate system can be WGS 84 longitude and latitude
(EPSG:4326) or one of those whose GeoTools transform peabiru
reproduces: SIRGAS 2000 (4674), SAD69 (4618), Córrego Alegre 1970-72
(4225), their UTM zones used in Brazil, SIRGAS 2000 / Brazil Polyconic
(5880), WGS 84 / UTM (326xx, 327xx) and WGS 84 / Pseudo-Mercator (3857).
Any other raster is an error that says what to change (reproject it with
GDAL, for example).

## Warnings

`StreetNetwork::elevation_warnings()` (and `Network::build_warnings()`, with the
warnings about GTFS feeds) lists what looks wrong in the raster.
The core never prints them; the R, Python and JavaScript packages turn them
into warnings.

- **Voids and values out of range.** A sample beyond +/-3276.7 m does not fit
  in 16-bit decimetres and is stored wrapped around: an SRTM void of
  -32768 becomes 0 m, a no-data value of -9999 becomes 3108.2 m. peabiru
  says how many
  samples were affected; samples equal to the raster's declared no-data
  value are counted too. Fill the voids or crop the raster.
- **Samples on a pixel boundary.** In a raster that is not in EPSG:4326,
  the point is projected before the pixel is found. For a sample within
  nanometres of the boundary between two pixels of different values (or of
  the raster's edge), the last bits of the projection's sines and cosines
  decide which pixel is read, and those can differ from one mathematical
  library to another. peabiru computes them with its own fdlibm functions
  and counts such samples in the
  warnings, so its results do not depend on the machine. A sample that
  falls on the raster's outer edge (inside its envelope,
  past the last pixel) is an error.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/elevation.rs}}
```

With `spo_hills.tif`, synthetic hills between 743 and 834 m made for the
tests (not measured data):

```text
NONE      Walk  648 min  Bicycle  354 min  Car  179 min   (sum of the times from the first point)
TOBLER    Walk  669 min  Bicycle  365 min  Car  179 min   (sum of the times from the first point)
MINETTI   Walk  651 min  Bicycle  359 min  Car  179 min   (sum of the times from the first point)
```

The same hills in SIRGAS 2000 / UTM zone 23S
(`crates/peabiru-core/tests/data/spo_hills_31983.tif`, as the second
argument) give nearly the same times:

```text
NONE      Walk  648 min  Bicycle  354 min  Car  179 min   (sum of the times from the first point)
TOBLER    Walk  668 min  Bicycle  364 min  Car  179 min   (sum of the times from the first point)
MINETTI   Walk  652 min  Bicycle  359 min  Car  179 min   (sum of the times from the first point)
```

And a raster with SRTM voids (`python/tests/data/spo_voids.tif`) gives the
warning:

```text
NONE      Walk  648 min  Bicycle  354 min  Car  179 min   (sum of the times from the first point)
warning: of the 21899 elevation samples on the network, 6298 with a value beyond +/-3276.7 m, which is stored wrapped around as 16-bit decimetres (an SRTM void of -32768 becomes 0 m, -9999 becomes 3108.2 m, 3500 m becomes -3053.6 m); the slope costs follow from these values, and are probably not what you want there: fill the voids or crop the raster
TOBLER    Walk  664 min  Bicycle  361 min  Car  179 min   (sum of the times from the first point)
warning: of the 21899 elevation samples on the network, 6298 with a value beyond +/-3276.7 m, which is stored wrapped around as 16-bit decimetres (an SRTM void of -32768 becomes 0 m, -9999 becomes 3108.2 m, 3500 m becomes -3053.6 m); the slope costs follow from these values, and are probably not what you want there: fill the voids or crop the raster
MINETTI   Walk  656 min  Bicycle  364 min  Car  179 min   (sum of the times from the first point)
```

With `Elevation::None` the raster is not read, so there is no warning.
