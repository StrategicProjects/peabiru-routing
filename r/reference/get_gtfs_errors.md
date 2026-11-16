# Get GTFS errors encountered in network building

The errors found in the network's 'GTFS' feeds, as the validator
'gtfs-lib' (Conveyal) reports them.
[`build_network()`](build_network.md) refuses to build a network when
any error has priority `"HIGH"`; this lists them, also for a directory a
network could not be built from.

## Usage

``` r
get_gtfs_errors(network)
```

## Arguments

- network:

  A network built with [`build_network()`](build_network.md), or the
  path of a directory with 'GTFS' feeds (`.zip`), for instance one a
  network could not be built from.

## Value

A table with columns `file` (the table, without `.txt`), `line` (the
record, the header being line 1; 0 for a whole table), `type`
(gtfs-lib's error class), `field`, `id` (the key concerned, for
duplicate keys and suspect stop locations) and `priority` (`"HIGH"`,
`"MEDIUM"`, `"LOW"` or `"UNKNOWN"`).

Tables are tibbles by default; `options(peabiru.output = "data.frame")`
gives base `data.frame`s and `options(peabiru.output = "data.table")`
gives `data.table`s (see [peabiru-package](peabiru-package.md)).

## Details

The feeds are read again (the files the network was built from, or the
`.zip` files of the directory given) with gtfs-lib's checks: every check
of its table loaders (empty required fields, numbers, times and URLs
that do not parse, values out of range, duplicate and missing keys,
references to stops, trips, routes, services, agencies and fares that do
not exist, tables missing, empty or in a subfolder of the `.zip`) and of
its post-load validation (services, parent stations, stops in places
with nearly no population, GTFS-Flex trips), with the `file`, `line`,
`type`, `field`, `id` and `priority` gtfs-lib gives. The rows are listed
feed by feed, in the order gtfs-lib finds them. A feed on which gtfs-lib
fails instead of listing errors (a date that is not `YYYYMMDD`, a
frequency of a trip that does not exist, a service or fare without an
id) is an error naming the cause, as is a `locations.geojson` (of
GTFS-Flex zones) that is not in UTF-8. GTFS-Flex zones and location
groups are read as gtfs-lib reads them, for its errors; on-demand trips
are not routed.

Nothing is written to the data folder. The types of the columns are
those
[`data.table::fread()`](https://rdrr.io/pkg/data.table/man/fread.html)
would give a file of these rows: a column with no value at all (often
`field` or `id`) is logical, `line` is an integer.

## See also

Other network functions: [`build_network()`](build_network.md),
[`check_transit_availability()`](check_transit_availability.md),
[`find_snap()`](find_snap.md),
[`street_network_bbox()`](street_network_bbox.md),
[`street_network_to_sf()`](street_network_to_sf.md),
[`transit_network_to_sf()`](transit_network_to_sf.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
get_gtfs_errors(network)
#> # A tibble: 7 × 6
#>   file      line type              field      id    priority
#>   <chr>    <int> <chr>             <chr>      <chr> <chr>   
#> 1 agency       3 DuplicateKeyError agency_id  1     MEDIUM  
#> 2 calendar     8 DuplicateKeyError service_id USD   MEDIUM  
#> 3 calendar     9 DuplicateKeyError service_id U__   MEDIUM  
#> 4 calendar    10 DuplicateKeyError service_id US_   MEDIUM  
#> 5 calendar    11 DuplicateKeyError service_id _SD   MEDIUM  
#> 6 calendar    12 DuplicateKeyError service_id __D   MEDIUM  
#> 7 calendar    13 DuplicateKeyError service_id _S_   MEDIUM  
```
