# Build a transport network

Builds a multimodal transport network from a directory with one
'OpenStreetMap' extract (`.pbf`), zero or more 'GTFS' feeds (`.zip`)
and, optionally, an elevation raster (`.tif`). The network lives in
memory and is reused by every routing function; nothing is written to
`data_path`. The files are recognised by their extension in lower case
only: a `map.PBF` or a `feed.ZIP` is ignored.

## Usage

``` r
build_network(
  data_path,
  verbose = FALSE,
  temp_dir = FALSE,
  elevation = "TOBLER",
  overwrite = FALSE,
  n_threads = Inf
)
```

## Arguments

- data_path:

  A string, the directory with the `.pbf` file, the 'GTFS' feeds and the
  elevation raster.

- verbose:

  A logical. Whether to report the build of the network in messages.
  Defaults to `FALSE`.

- temp_dir:

  A logical, checked but without effect: nothing is downloaded or
  cached.

- elevation:

  A string, the slope cost function for walking and cycling when
  `data_path` has a `.tif` elevation raster: `"TOBLER"` (the default;
  Tobler's hiking function), `"MINETTI"` (Minetti's energy cost of
  walking) or `"NONE"` (no slope costs).

- overwrite:

  A logical, checked but without effect: the network is always built
  anew.

- n_threads:

  The number of threads used to read the `.pbf` file. Defaults to `Inf`,
  all cores (two under `R CMD check` when it limits the cores).

## Value

A `peabiru_network` object to pass to the routing functions (printing it
summarises the network).

## Elevation

A `.tif` raster in `data_path` gives walking and cycling times slope
costs (`elevation = "TOBLER"`, the default, or `"MINETTI"`; never for
cars): the raster is sampled every 10 m along each street at the nearest
pixel, and each street's time is multiplied by the average cost of its
slopes. Points outside the raster count as 0 m high. Without a `.tif`
raster the network is built with `elevation = "NONE"`.

The raster must be a GeoTIFF with one band, north up, uncompressed or
compressed with LZW, Deflate or PackBits (what GDAL writes by default),
in WGS 84 longitude and latitude (EPSG:4326) or another coordinate
reference system given by its EPSG code (what GDAL writes) whose
transform is reproduced exactly: SIRGAS 2000 (4674), SAD69 (4618),
Corrego Alegre 1970-72 (4225), their UTM zones used in Brazil
(31965-31985, 6210, 6211, 5396; 29168-29172, 29187-29195; 22521-22525),
SIRGAS 2000 / Brazil Polyconic (5880), WGS 84 / UTM (32601-32660,
32701-32760) and WGS 84 / Pseudo-Mercator (3857). Other rasters are an
error that says what to change, and so are a `.prj` file next to the
raster, a raster whose name ends in `.tiff` and more than one raster.

Elevations are kept in decimetres in 16-bit integers: voids and values
beyond +/-3276.7 m wrap around (SRTM's -32768 voids become 0 m, -9999
becomes 3108.2 m, a `NaN` no-data value 0 m), with a warning counting
them and the samples equal to the raster's no-data value.

In a system other than EPSG:4326 the pixel read for a street point
within nanometres (5e-8 m, or 2e-13 degrees) of the boundary between two
pixels, or of the raster's edge, depends on the last bits of the sines
and cosines of the projection. peabiru computes them itself, bit for bit
the same way, so the result is the same on every machine, and warns once
with the number of such points (about one in a hundred million for 30 m
pixels; reprojecting the raster to EPSG:4326 removes the ambiguity). A
point that falls on the raster's outer edge (inside its envelope, past
the last pixel) is an error, since there is no pixel to read.

A raster also turns off the swap of origins and destinations of walking
matrices (see [`travel_time_matrix()`](travel_time_matrix.md)), and a
routing call with a scenario (`new_carspeeds`, `carspeed_scale`,
`new_lts`) runs without the slope costs, with a warning.

## Public transport

The 'GTFS' feeds are loaded in the order in which the operating system
lists the directory. With more than one feed that order depends on the
file system, so the same directory can give slightly different expanded
and arrival matrices on two machines (another stop or route for a few
trips); on one machine the results are always the same.

The network is not built when a 'GTFS' feed has errors of priority
`"HIGH"` (a reference to a stop, route, trip, service or agency that
does not exist, a number that cannot be read, tables in a subfolder of
the `.zip`, routes without any agency): the error names the feeds and
points to [`get_gtfs_errors()`](get_gtfs_errors.md), which lists the
errors. A feed that cannot be loaded at all is an error too.

## The network object

The network is held in memory by the 'Rust' engine and is always built
anew: there is no cache on disk, and a network cannot be saved with
[`saveRDS()`](https://rdrr.io/r/base/readRDS.html) or carried to another
session (a reloaded one is refused with an error; build it again). More
than one `.pbf` file in `data_path` is an error.

## See also

Other network functions:
[`check_transit_availability()`](check_transit_availability.md),
[`find_snap()`](find_snap.md),
[`get_gtfs_errors()`](get_gtfs_errors.md),
[`street_network_bbox()`](street_network_bbox.md),
[`street_network_to_sf()`](street_network_to_sf.md),
[`transit_network_to_sf()`](transit_network_to_sf.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
network
#> <peabiru_network>
#>   data path: /Users/leite/Github/peabiru-routing/.claude/scratch/sessao-1112/ns/rlib-015/peabiru/extdata/spo
#>   streets:   2,609 vertices, 5,518 edges
#>   transit:   1 feed(s) (spo_gtfs), 654 stops, 36 patterns, 6 services, frequency-based trips
#>   elevation: NONE
```
