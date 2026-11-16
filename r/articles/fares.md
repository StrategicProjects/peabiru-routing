# Fares and Pareto frontiers

peabiru computes the fare of each public transport trip from a *fare
structure*, which describes how much each ride costs and which transfers
are discounted; the routing functions take it as `fare_structure`. With
one, public transport is routed with the multi-criteria router
(McRAPTOR), which keeps at each stop the trips that are faster or
cheaper than the others, over `time_window * draws_per_minute` departure
times sampled in the window.

## The fare structure

[`setup_fare_structure()`](../reference/setup_fare_structure.md) builds
one where every route, fare type and transfer costs the same; edit it to
follow the fare rules of the city.

\
`fares`` ``<-`` `[`setup_fare_structure`](../reference/setup_fare_structure.md)`(``network``, base_fare ``=`` ``4.4``, by ``=`` ``"MODE"``)`\
`fares``$``fares_per_type`\
`#> ``# A tibble: 3 × 5`\
`#>   type   unlimited_transfers allow_same_route_transfer use_route_fare  fare`\
`#>   ``<chr>``  ``<lgl>``               ``<lgl>``                     ``<lgl>``          ``<dbl>`\
`#> ``1`` SUBWAY FALSE               FALSE                     FALSE            4.4`\
`#> ``2`` BUS    FALSE               FALSE                     FALSE            4.4`\
`#> ``3`` RAIL   FALSE               FALSE                     FALSE            4.4`\
[`head`](https://rdrr.io/r/utils/head.html)`(``fares``$``fares_per_transfer``, ``3``)`\
`#> ``# A tibble: 3 × 3`\
`#>   first_leg second_leg  fare`\
`#>   ``<chr>``     ``<chr>``      ``<dbl>`\
`#> ``1`` BUS       SUBWAY       4.4`\
`#> ``2`` SUBWAY    SUBWAY       4.4`\
`#> ``3`` BUS       BUS          4.4`

The rules: the first ride costs its route’s fare (the type’s `fare`, or
the route’s own `route_fare` when the type has `use_route_fare`); a ride
of the same type as the one before is free when the type has
`unlimited_transfers`; otherwise, while fewer than
`max_discounted_transfers` discounts were used, a ride boarded within
`transfer_time_allowance` minutes of the previous boarding, with a row
of `fares_per_transfer` for the two types (and not on the same route,
unless the type has `allow_same_route_transfer`), brings the two rides
to that row’s `fare`; any other ride costs its full fare. The total is
capped at `fare_cap`.

\
`fares``$``fares_per_transfer``$``fare`` ``<-`` ``6.6``   ``# two rides together`\
`fares``$``transfer_time_allowance`` ``<-`` ``60L``   ``# within an hour`

[`write_fare_structure()`](../reference/write_fare_structure.md) and
[`read_fare_structure()`](../reference/read_fare_structure.md) write and
read a structure as a `.zip` file of five CSV files (the settings and
the three tables), to keep it or share it.

\
`path`` ``<-`` `[`tempfile`](https://rdrr.io/r/base/tempfile.html)`(``fileext ``=`` ``".zip"``)`\
[`write_fare_structure`](../reference/write_fare_structure.md)`(``fares``, ``path``)`\
[`identical`](https://rdrr.io/r/base/identical.html)`(`[`read_fare_structure`](../reference/read_fare_structure.md)`(``path``)``, ``fares``)`\
`#> [1] TRUE`

## Routing with fares

`max_fare` keeps, for each pair, the fastest trip that costs no more
than it, in
[`travel_time_matrix()`](../reference/travel_time_matrix.md),
[`accessibility()`](../reference/accessibility.md) and
[`detailed_itineraries()`](../reference/detailed_itineraries.md):

\
`ttm`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(`\
`  ``network``, ``points``, ``points``,`\
`  mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``, departure_datetime ``=`` ``departure``,`\
`  fare_structure ``=`` ``fares``, max_fare ``=`` ``4.4``,`\
`  draws_per_minute ``=`` ``1``, seed ``=`` ``42`\
`)`\
[`head`](https://rdrr.io/r/utils/head.html)`(``ttm``)`\
`#> ``# A tibble: 6 × 3`\
`#>   from_id         to_id           travel_time_p50`\
`#>   ``<chr>``           ``<chr>``                     ``<int>`\
`#> ``1`` 89a8100c00fffff 89a8100c00fffff               0`\
`#> ``2`` 89a8100c00fffff 89a8100c007ffff               7`\
`#> ``3`` 89a8100c00fffff 89a8100c077ffff              10`\
`#> ``4`` 89a8100c00fffff 89a8100c03bffff               8`\
`#> ``5`` 89a8100c00fffff 89a8100c033ffff              13`\
`#> ``6`` 89a8100c00fffff 89a8100c0afffff              23`

[`pareto_frontier()`](../reference/pareto_frontier.md) gives, for each
pair, the travel time at each fare cutoff where it changes.
`monetary_cost` is the cutoff as the router reads it, a 32-bit float.

\
`pf`` ``<-`` `[`pareto_frontier`](../reference/pareto_frontier.md)`(`\
`  ``network``, ``points``[``1``:``3``, ``]``, ``points``,`\
`  mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``, departure_datetime ``=`` ``departure``,`\
`  fare_structure ``=`` ``fares``, fare_cutoffs ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``0``, ``4.4``, ``6.6``)``, seed ``=`` ``42`\
`)`\
[`head`](https://rdrr.io/r/utils/head.html)`(``pf``)`\
`#> ``# A tibble: 6 × 5`\
`#>   from_id         to_id           percentile travel_time monetary_cost`\
`#>   ``<chr>``           ``<chr>``                ``<int>``       ``<int>``         ``<dbl>`\
`#> ``1`` 89a8100c00fffff 89a8100c00fffff         50           0             0`\
`#> ``2`` 89a8100c00fffff 89a8100c007ffff         50           7             0`\
`#> ``3`` 89a8100c00fffff 89a8100c077ffff         50          10             0`\
`#> ``4`` 89a8100c00fffff 89a8100c03bffff         50           8             0`\
`#> ``5`` 89a8100c00fffff 89a8100c033ffff         50          13             0`\
`#> ``6`` 89a8100c00fffff 89a8100c0afffff         50          23             0`

In this sample, whose GTFS is frequency-based, the multi-criteria router
finds no trip faster than walking (it does not ride lines given by
frequency): with fares the travel times are the walking times. With
scheduled feeds, such as those of Porto Alegre in the [trip
options](trip-options.md) article, the frontier lists the faster trips
that cost more.

## Alternative itineraries

`detailed_itineraries(shortest_path = FALSE)` lists the alternatives
McRAPTOR finds, one per sequence of routes, up to `suboptimal_minutes`
slower than the best; with a fare structure, each with its fare
(`total_fare`) and the fare paid by the end of each leg
(`cumulative_fare`). Networks whose GTFS has frequency-based trips
cannot be used with public transport there, so this sample’s itineraries
are by street:

\
`trips`` ``<-`` `[`detailed_itineraries`](../reference/detailed_itineraries.md)`(`\
`  ``network``, ``points``[``1``:``2``, ``]``, ``points``[``3``:``4``, ``]``, mode ``=`` ``"WALK"``,`\
`  departure_datetime ``=`` ``departure``, fare_structure ``=`` ``fares``,`\
`  drop_geometry ``=`` ``TRUE`\
`)`\
`trips``[``, `[`c`](https://rdrr.io/r/base/c.html)`(``"from_id"``, ``"to_id"``, ``"option"``, ``"total_duration"``, ``"total_fare"``,`\
`          ``"mode"``, ``"cumulative_fare"``)``]`\
`#> ``# A tibble: 2 × 7`\
`#>   from_id         to_id   option total_duration total_fare mode  cumulative_fare`\
`#>   ``<chr>``           ``<chr>``    ``<int>``          ``<dbl>``      ``<dbl>`` ``<chr>``           ``<dbl>`\
`#> ``1`` 89a8100c00fffff 89a810…      1           10.9          0 WALK                0`\
`#> ``2`` 89a8100c007ffff 89a810…      1            9.5          0 WALK                0`

## Trip options

To see every trip worth considering between two points, faster or
cheaper, with fewer transfers or less walking, and not only the fastest
one, see [`trip_options()`](../reference/trip_options.md), peabiru’s own
multi-criteria planner: [Trip options](trip-options.md).
