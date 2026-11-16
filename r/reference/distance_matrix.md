# Straight-line distances between two sets of points

Every pair of an origin and a destination at most `max_distance` metres
apart in a straight line, with the distance. It needs no network, and
its result is a table that [`locate_facilities()`](locate_facilities.md)
and [`floating_catchment()`](floating_catchment.md) take in
`travel_times`, for the cases in which the service standard is a
distance (a water point within 1 km) or the streets of the area are not
mapped well enough to route on.

## Usage

``` r
distance_matrix(origins, destinations, max_distance)
```

## Arguments

- origins, destinations:

  The points: a `data.frame` with `id`, `lon` and `lat`, or a `POINT sf`
  with an `id` column.

- max_distance:

  The longest distance kept, in metres (the pairs with distance
  `<= max_distance`).

## Value

A table with one row per pair, ordered as the origins and, within each,
as the destinations: `from_id`, `to_id` and `distance` (metres).

Tables are tibbles by default; `options(peabiru.output = "data.frame")`
gives base `data.frame`s and `options(peabiru.output = "data.table")`
gives `data.table`s (see [peabiru-package](peabiru-package.md)).

## Details

The distance is the great-circle one on a sphere of radius 6,367,450 m,
the same that measures the streets of a network, and is the same on
every platform. A pair at distance 0 is a pair. Only the pairs within
`max_distance` are computed, so two large sets of points with a short
distance take little time and memory.

## See also

Other facility location: [`allocate_budget()`](allocate_budget.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
points <- read.csv(file.path(data_path, "spo_points.csv"))
near <- distance_matrix(points, points, max_distance = 800)
head(near)
#> # A tibble: 6 × 3
#>   from_id         to_id           distance
#>   <chr>           <chr>              <dbl>
#> 1 89a8100c00fffff 89a8100c00fffff       0 
#> 2 89a8100c00fffff 89a8100c007ffff     352.
#> 3 89a8100c00fffff 89a8100c077ffff     341.
#> 4 89a8100c00fffff 89a8100c03bffff     359.
#> 5 89a8100c00fffff 89a8100c033ffff     624.
#> 6 89a8100c00fffff 89a8100c067ffff     682.
# the two points with the most residents within 800 m of them
locate_facilities(NULL, points, points, model = "max_cover", n_facilities = 2,
                  cutoff = 800, demand_colname = "population",
                  travel_times = near)$sites
#> # A tibble: 2 × 6
#>   id                lon   lat order marginal cumulative
#>   <chr>           <dbl> <dbl> <int>    <dbl>      <dbl>
#> 1 89a8100c023ffff -46.6 -23.6     1    39445      39445
#> 2 89a8100c397ffff -46.6 -23.5     2     6828      46273
```
