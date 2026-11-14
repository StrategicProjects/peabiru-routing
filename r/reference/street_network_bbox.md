# Extract the geographic bounding box of the transport network

The bounding box of the street network: the envelope of the street
layer, which covers every 'OpenStreetMap' node of the ways streets were
made from (it can be a little larger than the box of the edges of
[`street_network_to_sf()`](street_network_to_sf.md)).

## Usage

``` r
street_network_bbox(network, output = c("polygon", "bbox", "vector"))
```

## Arguments

- network:

  A network built with [`build_network()`](build_network.md).

- output:

  A character string specifying the desired output format. One of
  `"polygon"` (the default), `"bbox"`, or `"vector"`.

## Value

By default (`output = "polygon"`), an `sfc` with a single `POLYGON`
geometry. If `output = "bbox"`, an `sf` `bbox` object. If
`output = "vector"`, a named numeric vector with `xmin`, `ymin`, `xmax`,
`ymax` coordinates. All outputs are in WGS 84 (EPSG 4326). Package 'sf'
is needed for the first two.

## See also

Other network functions: [`build_network()`](build_network.md),
[`check_transit_availability()`](check_transit_availability.md),
[`find_snap()`](find_snap.md),
[`get_gtfs_errors()`](get_gtfs_errors.md),
[`street_network_to_sf()`](street_network_to_sf.md),
[`transit_network_to_sf()`](transit_network_to_sf.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
street_network_bbox(network, output = "vector")
#>      xmin      ymin      xmax      ymax 
#> -46.64771 -23.56534 -46.61130 -23.53711 
```
