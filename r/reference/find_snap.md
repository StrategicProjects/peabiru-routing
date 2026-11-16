# Find snapped locations of input points on street network

Finds where points snap to the street network: snapping is how routing
places origins and destinations on the network. The nearest point of a
street the mode may use is looked for first within 300 metres of the
point and then, if there is none, within `radius`. A point that does not
snap is not routed. The street a point snaps to depends on the mode,
since some streets cannot be used by some modes (a pedestrian street
cannot be used to snap car trips).

## Usage

``` r
find_snap(network, points, radius = 1600, mode = "WALK")
```

## Arguments

- network:

  A network built with [`build_network()`](build_network.md).

- points:

  Either a `POINT sf` object with an `id` column, in any coordinate
  reference system (reprojected to WGS 84, see
  [`travel_time_matrix()`](travel_time_matrix.md)), or a `data.frame`
  containing the columns `id`, `lon` and `lat` (WGS 84).

- radius:

  Numeric. The maximum radius in meters within which to snap. Defaults
  to 1600m.

- mode:

  A string. Which mode to consider when trying to snap the points to the
  network. Defaults to `WALK`, also allows `BICYCLE` and `CAR`.

## Value

A table with the original points (`point_id`, `lat`, `lon`), their
snapped coordinates on the street network (`snap_lat`, `snap_lon`), the
distance in meters between the point and its snapped location
(`distance`, haversine on a sphere of radius 6,367,450 m) and whether
the point snapped (`found`). Points that could not be snapped have `NA`
coordinates and distance and `found = FALSE`.

Tables are tibbles by default; `options(peabiru.output = "data.frame")`
gives base `data.frame`s and `options(peabiru.output = "data.table")`
gives `data.table`s (see [peabiru-package](peabiru-package.md)). A
`data.table` result has an index on `found` (when
`options(datatable.auto.index)` is on, as by default).

## See also

Other network functions: [`build_network()`](build_network.md),
[`check_transit_availability()`](check_transit_availability.md),
[`get_gtfs_errors()`](get_gtfs_errors.md),
[`street_network_bbox()`](street_network_bbox.md),
[`street_network_to_sf()`](street_network_to_sf.md),
[`transit_network_to_sf()`](transit_network_to_sf.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
points <- read.csv(file.path(data_path, "spo_points.csv"))
snaps <- find_snap(network, points, radius = 2000, mode = "WALK")
head(snaps)
#> # A tibble: 6 × 7
#>   point_id          lat   lon snap_lat snap_lon distance found
#>   <chr>           <dbl> <dbl>    <dbl>    <dbl>    <dbl> <lgl>
#> 1 89a8100c00fffff -23.6 -46.6    -23.6    -46.6   13.7   TRUE 
#> 2 89a8100c007ffff -23.6 -46.6    -23.6    -46.6    9.48  TRUE 
#> 3 89a8100c077ffff -23.6 -46.6    -23.6    -46.6    0.768 TRUE 
#> 4 89a8100c03bffff -23.6 -46.6    -23.6    -46.6    1.38  TRUE 
#> 5 89a8100c033ffff -23.6 -46.6    -23.6    -46.6   14.9   TRUE 
#> 6 89a8100c0afffff -23.6 -46.6    -23.6    -46.6    7.72  TRUE 
```
