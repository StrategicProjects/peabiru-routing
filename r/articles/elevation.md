# Elevation

Walking and cycling uphill take longer. When the data directory has a
GeoTIFF elevation raster (`.tif`),
[`build_network()`](../reference/build_network.md) samples it every 10 m
along each street, at the nearest pixel, and multiplies the street’s
walking and cycling times by the average cost of its slopes. Car times
do not change.

This page uses the Porto Alegre sample of r5r 2.5.0, which has a raster
of the city (`poa_elevation.tif`, SRTM data at about 90 m).

\
[`library`](https://rdrr.io/r/base/library.html)`(`[`peabiru`](https://strategicprojects.github.io/peabiru-routing/r/)`)`\
`poa`` ``<-`` ``poa_sample``(`[`c`](https://rdrr.io/r/base/c.html)`(``"poa_osm.pbf"``, ``"poa_elevation.tif"``, ``"poa_hexgrid.csv"``)``,`\
`                  dir ``=`` `[`file.path`](https://rdrr.io/r/base/file.path.html)`(`[`tempdir`](https://rdrr.io/r/base/tempfile.html)`(``)``, ``"poa_elevation"``)``)`\
`hexes`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_hexgrid.csv"``)``)`\
`pts`` ``<-`` ``hexes``[`[`seq`](https://rdrr.io/r/base/seq.html)`(``1``, `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``hexes``)``, by ``=`` ``40``)``, ``]`\
[`nrow`](https://rdrr.io/r/base/nrow.html)`(``pts``)`\
`#> [1] 31`

## Tobler and Minetti

`elevation` picks the cost function: `"TOBLER"` (the default, Tobler’s
hiking function) or `"MINETTI"` (Minetti’s energy cost of walking and
cycling); `"NONE"` ignores the raster.

\
`flat`` ``<-`` `[`build_network`](../reference/build_network.md)`(``poa``, elevation ``=`` ``"NONE"``, n_threads ``=`` ``2``)`\
`tobler`` ``<-`` `[`build_network`](../reference/build_network.md)`(``poa``, n_threads ``=`` ``2``)`\
`minetti`` ``<-`` `[`build_network`](../reference/build_network.md)`(``poa``, elevation ``=`` ``"MINETTI"``, n_threads ``=`` ``2``)`\
`tobler`\
`#> <peabiru_network>`\
`#>   data path: /private/var/folders/j9/7g_srh2x0d71c5q0pbj5mxh40000gn/T/Rtmp2LGmcN/poa_elevation`\
`#>   streets:   20,832 vertices, 56,916 edges`\
`#>   transit:   none (no GTFS feed)`\
`#>   elevation: TOBLER`\
\
`walk`` ``<-`` ``function``(``net``)`` ``{`\
`  `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``net``, ``pts``, ``pts``, mode ``=`` ``"WALK"``, max_trip_duration ``=`` ``60``,`\
`                     n_threads ``=`` ``2``)`\
`}`\
`times`` ``<-`` `[`Reduce`](https://rdrr.io/r/base/funprog.html)`(``function``(``a``, ``b``)`` `[`merge`](https://rdrr.io/r/base/merge.html)`(``a``, ``b``, by ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"from_id"``, ``"to_id"``)``)``,`\
`                `[`list`](https://rdrr.io/r/base/list.html)`(``walk``(``flat``)``, ``walk``(``tobler``)``, ``walk``(``minetti``)``)``)`\
[`names`](https://rdrr.io/r/base/names.html)`(``times``)``[``3``:``5``]`` ``<-`` `[`c`](https://rdrr.io/r/base/c.html)`(``"none"``, ``"tobler"``, ``"minetti"``)`\
[`summary`](https://rdrr.io/r/base/summary.html)`(``times``[``3``:``5``]``)`\
`#>       none           tobler         minetti  `\
`#>  Min.   : 0.00   Min.   : 0.00   Min.   : 0  `\
`#>  1st Qu.:19.00   1st Qu.:20.00   1st Qu.:22  `\
`#>  Median :30.00   Median :33.00   Median :36  `\
`#>  Mean   :28.01   Mean   :30.29   Mean   :33  `\
`#>  3rd Qu.:40.00   3rd Qu.:44.00   3rd Qu.:48  `\
`#>  Max.   :57.00   Max.   :59.00   Max.   :60`

Pairs reached within the hour on flat ground can take longer than that
with the slopes, so the number of pairs within `max_trip_duration`
changes too (here, of the pairs reached in all three).

A raster also turns off one shortcut: without one, a walking matrix with
more origins than destinations is searched from the destinations; with a
raster, slopes make the trip one way differ from the trip back, and the
search runs from the origins.

## Rasters in other coordinate systems

The raster can be in WGS 84 longitude and latitude (EPSG:4326) or in one
of the projected and geographic systems used for Brazil and worldwide
whose transform is reproduced exactly: SIRGAS 2000 (EPSG:4674), SAD69,
Córrego Alegre and their UTM zones, SIRGAS 2000 / Brazil Polyconic, WGS
84 / UTM and WGS 84 / Pseudo-Mercator (the list is in
[`?build_network`](../reference/build_network.md)). Here the raster is
warped with GDAL (through ‘sf’) to SIRGAS 2000 / UTM zone 22S
(EPSG:31982):

\
`utm`` ``<-`` `[`file.path`](https://rdrr.io/r/base/file.path.html)`(`[`tempdir`](https://rdrr.io/r/base/tempfile.html)`(``)``, ``"poa_utm"``)`\
[`dir.create`](https://rdrr.io/r/base/files2.html)`(``utm``, showWarnings ``=`` ``FALSE``)`\
[`invisible`](https://rdrr.io/r/base/invisible.html)`(`[`file.copy`](https://rdrr.io/r/base/files.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_osm.pbf"``)``, ``utm``)``)`\
`sf``::`[`gdal_utils`](https://r-spatial.github.io/sf/reference/gdal_utils.html)`(``"warp"``, `[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_elevation.tif"``)``,`\
`               `[`file.path`](https://rdrr.io/r/base/file.path.html)`(``utm``, ``"poa_elevation_utm.tif"``)``,`\
`               options ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"-t_srs"``, ``"EPSG:31982"``, ``"-r"``, ``"near"``)``)`\
`tobler_utm`` ``<-`` `[`build_network`](../reference/build_network.md)`(``utm``, n_threads ``=`` ``2``)`\
`both`` ``<-`` `[`merge`](https://rdrr.io/r/base/merge.html)`(``walk``(``tobler``)``, ``walk``(``tobler_utm``)``, by ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"from_id"``, ``"to_id"``)``)`\
[`summary`](https://rdrr.io/r/base/summary.html)`(``both``$``travel_time_p50.y`` ``-`` ``both``$``travel_time_p50.x``)`\
`#>     Min.  1st Qu.   Median     Mean  3rd Qu.     Max. `\
`#> -1.00000  0.00000  0.00000 -0.05854  0.00000  1.00000`

The times change a little, because the pixels of the warped raster are
not those of the original.

In a projected system, which pixel a street point falls in is decided by
sines and cosines of the projection. For a point within nanometres of
the boundary between two pixels, the last bits of those functions
decide, and they differ between mathematical libraries and processors.
peabiru computes them itself, bit for bit the same way, so a network
gives the same results on every machine, and warns with the number of
such points when there are any (about one in a hundred million for 30 m
pixels). Reprojecting the raster to EPSG:4326 removes the question.

## Voids and no-data values

Elevations are kept in decimetres in 16-bit integers, so values beyond
+/-3276.7 m wrap around: SRTM’s voids (-32768) become 0 m, a no-data
value of -9999 becomes 3108.2 m, and a `NaN` no-data value 0 m. peabiru
stores them that way and warns, counting them. The raster below is a
small synthetic one over the São Paulo sample, with a few voids:

\
`voids`` ``<-`` `[`file.path`](https://rdrr.io/r/base/file.path.html)`(`[`tempdir`](https://rdrr.io/r/base/tempfile.html)`(``)``, ``"spo_voids"``)`\
[`dir.create`](https://rdrr.io/r/base/files2.html)`(``voids``, showWarnings ``=`` ``FALSE``)`\
[`invisible`](https://rdrr.io/r/base/invisible.html)`(`[`file.copy`](https://rdrr.io/r/base/files.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_path``, `[`c`](https://rdrr.io/r/base/c.html)`(``"spo_osm.pbf"``, ``"spo_gtfs.zip"``)``)``, ``voids``)``)`\
[`invisible`](https://rdrr.io/r/base/invisible.html)`(`[`file.copy`](https://rdrr.io/r/base/files.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``".."``, ``".."``, ``"tests"``, ``"testthat"``, ``"fixtures"``, ``"spo_voids.tif"``)``, ``voids``)``)`\
`spo_voids`` ``<-`` `[`build_network`](../reference/build_network.md)`(``voids``, n_threads ``=`` ``2``)`\
`#> Warning: of the 21899 elevation samples on the network, 6298 with a value`\
`#> beyond +/-3276.7 m, which is stored wrapped around as 16-bit decimetres (an`\
`#> SRTM void of -32768 becomes 0 m, -9999 becomes 3108.2 m, 3500 m becomes -3053.6`\
`#> m); the slope costs follow from these values, and are probably not what you`\
`#> want there: fill the voids or crop the raster`

Fill the voids (for instance with `gdal_fillnodata`) before building the
network to avoid them.

## Scenarios

A routing call with a scenario of car speeds or traffic stress
(`new_carspeeds`, `carspeed_scale`, `new_lts`; see
[Scenarios](scenarios.md)) routes without the slope costs, and says so:

\
`x`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``tobler``, ``pts``[``1``:``3``, ``]``, ``pts``[``1``:``3``, ``]``, mode ``=`` ``"WALK"``,`\
`                        carspeed_scale ``=`` ``0.8``, n_threads ``=`` ``2``)`\
`#> Warning: A scenario was used for this calculation: WALK and BICYCLE are routed`\
`#> without the slope costs of the elevation raster.`
