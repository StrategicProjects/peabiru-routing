# Extract OpenStreetMap network in sf format

The street network in `sf` format: the vertices at the ends of street
edges and every directed street edge (the connectors of transit stops
and park-and-rides left out), in the order of the network's edge index.

## Usage

``` r
street_network_to_sf(network)
```

## Arguments

- network:

  A network built with [`build_network()`](build_network.md).

## Value

A list with two `sf` data frames in WGS 84: `vertices` (`POINT`), with
columns `index`, `park_and_ride`, `bike_sharing` and `geometry`, and
`edges` (`LINESTRING`), with columns `edge_index`, `osm_id`,
`from_vertex`, `to_vertex`, `street_class`, `length` (meters), `walk`,
`car`, `car_speed` (km/h), `bicycle`, `bicycle_lts` and `geometry`.
Package 'sf' is needed. With `options(peabiru.output = "data.table")`,
`edges` is an `sf` `data.table` and `vertices` a plain `sf` data frame
(see [peabiru-package](peabiru-package.md)).

## See also

Other network functions: [`build_network()`](build_network.md),
[`check_transit_availability()`](check_transit_availability.md),
[`find_snap()`](find_snap.md),
[`get_gtfs_errors()`](get_gtfs_errors.md),
[`street_network_bbox()`](street_network_bbox.md),
[`transit_network_to_sf()`](transit_network_to_sf.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
street_net <- street_network_to_sf(network)
head(street_net$edges)
#> Simple feature collection with 6 features and 11 fields
#> Geometry type: LINESTRING
#> Dimension:     XY
#> Bounding box:  xmin: -46.63534 ymin: -23.55117 xmax: -46.63387 ymax: -23.542
#> Geodetic CRS:  WGS 84
#> # A tibble: 6 × 12
#>   edge_index  osm_id from_vertex to_vertex street_class length walk  car  
#>        <int>   <dbl>       <int>     <int> <chr>         <dbl> <lgl> <lgl>
#> 1          0 4331470           0         1 OTHER         179.  FALSE TRUE 
#> 2          1 4331470           1         0 OTHER         179.  FALSE FALSE
#> 3          2 8103459           2         3 OTHER          73.6 TRUE  TRUE 
#> 4          3 8103459           3         2 OTHER          73.6 TRUE  TRUE 
#> 5          4 8103459           3         4 OTHER          27.0 TRUE  TRUE 
#> 6          5 8103459           4         3 OTHER          27.0 TRUE  TRUE 
#> # ℹ 4 more variables: car_speed <dbl>, bicycle <lgl>, bicycle_lts <int>,
#> #   geometry <LINESTRING [°]>
```
