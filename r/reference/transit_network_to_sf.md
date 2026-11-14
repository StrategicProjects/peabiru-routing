# Extract transit network in sf format

The public transport network in `sf` format: the stops of the 'GTFS'
feeds, and the line of every trip pattern of each route.

## Usage

``` r
transit_network_to_sf(network)
```

## Arguments

- network:

  A network built with [`build_network()`](build_network.md).

## Value

A list with two `sf` data frames in WGS 84: `stops` (`POINT`), with
columns `stop_index`, `stop_id` (prefixed by the feed id), `stop_name`,
`linked_to_street` and `geometry`, and `routes` (`LINESTRING`), with
columns `agency_id`, `agency_name`, `route_id`, `long_name`,
`short_name`, `mode` and `geometry`. There is one route row per trip
pattern (the trips of a route with the same stops), so the same route
can appear more than once; its line is the GTFS shape of the first trip
of the pattern or, without one, the positions of its stops. A stop not
linked to the street network (outside the area of the `.pbf`, for
instance) has an empty geometry (`POINT EMPTY`), and a pattern with no
line is left out. Positions of stops are those of their vertices in the
street network (the coordinates rounded down to 1e-7 degrees). Package
'sf' is needed. With `options(peabiru.output = "data.table")`, `routes`
is an `sf` `data.table` and `stops` a plain `sf` data frame (see
[peabiru-package](peabiru-package.md)).

## See also

Other network functions: [`build_network()`](build_network.md),
[`check_transit_availability()`](check_transit_availability.md),
[`find_snap()`](find_snap.md),
[`get_gtfs_errors()`](get_gtfs_errors.md),
[`street_network_bbox()`](street_network_bbox.md),
[`street_network_to_sf()`](street_network_to_sf.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
transit_net <- transit_network_to_sf(network)
head(transit_net$stops)
#> Simple feature collection with 6 features and 4 fields
#> Geometry type: POINT
#> Dimension:     XY
#> Bounding box:  xmin: -46.61977 ymin: -23.54202 xmax: -46.6146 ymax: -23.53415
#> Geodetic CRS:  WGS 84
#> # A tibble: 6 × 5
#>   stop_index stop_id        stop_name linked_to_street              geometry
#>        <int> <chr>          <chr>     <lgl>                      <POINT [°]>
#> 1          0 spo_gtfs:1000… R. Silva… TRUE             (-46.61467 -23.53415)
#> 2          1 spo_gtfs:1000… R. Silva… TRUE             (-46.61595 -23.53545)
#> 3          2 spo_gtfs:1000… R. João … TRUE             (-46.61977 -23.53527)
#> 4          3 spo_gtfs:1000… Lgo. Da … TRUE             (-46.61736 -23.54202)
#> 5          4 spo_gtfs:1000… R. Orien… TRUE              (-46.6146 -23.53777)
#> 6          5 spo_gtfs:1000… R. Orien… TRUE              (-46.61672 -23.5379)
```
