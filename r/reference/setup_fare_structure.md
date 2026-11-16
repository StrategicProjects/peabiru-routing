# Setup a fare structure to calculate the monetary costs of trips

Creates a basic fare structure that describes how transit fares should
be calculated in [`travel_time_matrix()`](travel_time_matrix.md),
[`accessibility()`](accessibility.md),
[`detailed_itineraries()`](detailed_itineraries.md) and
[`pareto_frontier()`](pareto_frontier.md): every route, fare type and
transfer costs `base_fare` (read as a 32-bit float, the type fares are
kept in). The structure can then be edited to follow the fare rules of
the study area.

## Usage

``` r
setup_fare_structure(
  network,
  base_fare,
  by = "MODE",
  debug_path = NULL,
  debug_info = NULL
)
```

## Arguments

- network:

  A network built with [`build_network()`](build_network.md).

- base_fare:

  A number, at least 0: the fare of every route, type and transfer.

- by:

  A string: one fare type per mode (`"MODE"`, the default), per agency
  (`"AGENCY_ID"`, or `"AGENCY_NAME"`), or one type for all routes
  (`"GENERIC"`).

- debug_path, debug_info:

  A `.csv` path (or `NULL`) and one of `"MODE"`, `"ROUTE"` and
  `"MODE_ROUTE"` (or `NULL`), stored in the structure's
  `debug_settings`. No debug file is written whatever they say.

## Value

A fare structure (a list, see Details).

Tables are tibbles by default; `options(peabiru.output = "data.frame")`
gives base `data.frame`s and `options(peabiru.output = "data.table")`
gives `data.table`s (see [peabiru-package](peabiru-package.md)).

## Details

The structure is a list:

- `max_discounted_transfers`: most transfers with a discount (default
  1).

- `transfer_time_allowance`: minutes between the boardings of a
  discounted transfer (default 120).

- `fare_cap`: highest fare of a trip (`Inf`: no cap).

- `fares_per_type`: one row per fare type (`type`,
  `unlimited_transfers`, `allow_same_route_transfer`, `use_route_fare`,
  `fare`).

- `fares_per_transfer`: the fare of two rides together (`first_leg`,
  `second_leg`, `fare`), for the pairs of types with a discount.

- `fares_per_route`: the fare type of each GTFS route (`agency_id`,
  `agency_name`, `route_id`, `route_short_name`, `route_long_name`,
  `mode`, `route_fare`, `fare_type`), matched by `agency_id` and
  `route_id`.

- `debug_settings`: the debug settings (`output_file`, `trip_info`).

How the fare of a trip follows from it: the first ride costs its route's
fare (the type's `fare`, or the route's `route_fare` when the type has
`use_route_fare`); each later ride of the same type as the previous one
is free when the type has `unlimited_transfers`; otherwise, while fewer
than `max_discounted_transfers` discounts were used, a ride boarded
within `transfer_time_allowance` minutes of the previous boarding, with
a row of `fares_per_transfer` for the two types (and not on the same
route, unless the type has `allow_same_route_transfer`) brings the two
rides to that row's `fare`; any other ride costs its full fare. The
total is capped at `fare_cap`. Fare rules of this kind are the ones
supported: a structure that names a fare calculator instead (an element
`type`) is an error in the routing functions.

## See also

Other fare structure: [`pareto_frontier()`](pareto_frontier.md),
[`read_fare_structure()`](read_fare_structure.md),
[`write_fare_structure()`](write_fare_structure.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
fare_structure <- setup_fare_structure(network, base_fare = 4.4)
fare_structure$fares_per_type
#> # A tibble: 3 × 5
#>   type   unlimited_transfers allow_same_route_transfer use_route_fare  fare
#>   <chr>  <lgl>               <lgl>                     <lgl>          <dbl>
#> 1 SUBWAY FALSE               FALSE                     FALSE            4.4
#> 2 BUS    FALSE               FALSE                     FALSE            4.4
#> 3 RAIL   FALSE               FALSE                     FALSE            4.4
```
