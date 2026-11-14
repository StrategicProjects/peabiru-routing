# Calculate travel time and monetary cost Pareto frontier

Fits the Pareto frontier of travel time and fare between each origin and
each destination: for every value of `fare_cutoffs`, in the order given,
the travel times of the multi-criteria router (McRAPTOR, over
`time_window` sampled departure times) when no trip may cost more than
the cutoff; a destination and percentile gets a row at each cutoff where
its travel time changes and is below `max_trip_duration`.
`monetary_cost` is the cutoff as the router reads it, a 32-bit float
(`4.8` is `4.800000190734863`). Routes given by frequencies are routed
with Monte Carlo draws: pass `seed` for reproducible results.

## Usage

``` r
pareto_frontier(
  network,
  origins,
  destinations,
  mode = c("WALK", "TRANSIT"),
  mode_egress = "WALK",
  departure_datetime = NULL,
  time_window = 10L,
  seed = NULL,
  percentiles = 50L,
  max_walk_time = Inf,
  max_bike_time = Inf,
  max_car_time = Inf,
  max_trip_duration = 120L,
  fare_structure,
  fare_cutoffs,
  walk_speed = 3.6,
  bike_speed = 12,
  max_rides = 3,
  max_lts = 2,
  n_threads = Inf,
  verbose = FALSE,
  progress = TRUE,
  output_dir = NULL
)
```

## Arguments

- network:

  A network built with [`build_network()`](build_network.md).

- origins, destinations:

  A `data.frame` with columns `id`, `lon` and `lat` (WGS 84 longitude
  and latitude), or a `POINT sf` with an `id` column, in any coordinate
  reference system (CRS): an `sf` in another CRS than WGS 84 is
  reprojected with
  [`sf::st_transform()`](https://r-spatial.github.io/sf/reference/st_transform.html),
  with a message, and one without a CRS is an error. Ids that are not
  character are converted, with a warning; points with missing
  coordinates are not routed, with a warning; duplicated ids are an
  error.

- mode:

  A character vector, the transport modes (see the section "Transport
  modes"). Defaults to `"WALK"`.

- mode_egress:

  A string, the mode from the last stop to the destination: `"WALK"`
  (the default), `"BICYCLE"` or `"CAR"`. Ignored without public
  transport.

- departure_datetime:

  A POSIXct, the departure date and time (see the section "Date and
  time"). Required with public transport.

- time_window:

  An integer, the number of departure minutes, from `departure_datetime`
  on, over which travel times are computed. Defaults to 10.

- seed:

  An integer or `NULL`: the seed of the Monte Carlo draws of
  frequency-based routes.

- percentiles:

  An integer vector of up to 5 values from 1 to 99, the percentiles of
  the travel time over the departures of the window to report. Defaults
  to 50, the median.

- max_walk_time, max_bike_time, max_car_time:

  The maximum time in minutes on foot, by bicycle and by car. Each limit
  applies to each leg on its own (15 minutes to reach the first stop and
  another 15 from the last), and to the whole trip when that mode is the
  only one. `Inf`, the default, leaves only `max_trip_duration`.

- max_trip_duration:

  An integer, the maximum trip duration in minutes. Defaults to 120.

- fare_structure:

  A fare structure (see
  [`setup_fare_structure()`](setup_fare_structure.md)); required.

- fare_cutoffs:

  A numeric vector, required: the fare cutoffs, at least 0, unique.

- walk_speed, bike_speed:

  Average speeds in km/h, 3.6 and 12 by default.

- max_rides:

  An integer, the maximum number of public transport rides in a trip.
  Defaults to 3.

- max_lts:

  An integer from 1 to 4, the highest level of traffic stress a cyclist
  rides on (see the section "Level of traffic stress"). Defaults to 2.

- n_threads:

  The number of threads; `Inf` (the default) uses all cores (two under
  `R CMD check` when it limits the cores).

- verbose, progress:

  Logicals, checked but without effect: no progress counter or routing
  log is printed.

- output_dir:

  Must be `NULL` (the default): writing the frontier to files is not
  available.

## Value

A table with `from_id`, `to_id` (character), `percentile`, `travel_time`
and `monetary_cost`.

Tables are tibbles by default; `options(peabiru.output = "data.frame")`
gives base `data.frame`s and `options(peabiru.output = "data.table")`
gives `data.table`s (see [peabiru-package](peabiru-package.md)).

## See also

Other fare structure: [`read_fare_structure()`](read_fare_structure.md),
[`setup_fare_structure()`](setup_fare_structure.md),
[`write_fare_structure()`](write_fare_structure.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
points <- read.csv(file.path(data_path, "spo_points.csv"))
fares <- setup_fare_structure(network, base_fare = 4.4)
pf <- pareto_frontier(
  network, origins = points[1:3, ], destinations = points,
  mode = c("WALK", "TRANSIT"),
  departure_datetime = as.POSIXct("2019-05-13 14:00:00",
                                  tz = "America/Sao_Paulo"),
  fare_structure = fares, fare_cutoffs = c(0, 4.4, 8.8),
  seed = 42, n_threads = 2
)
head(pf)
#> # A tibble: 6 × 5
#>   from_id         to_id           percentile travel_time monetary_cost
#>   <chr>           <chr>                <int>       <int>         <dbl>
#> 1 89a8100c00fffff 89a8100c00fffff         50           0             0
#> 2 89a8100c00fffff 89a8100c007ffff         50           7             0
#> 3 89a8100c00fffff 89a8100c077ffff         50          10             0
#> 4 89a8100c00fffff 89a8100c03bffff         50           8             0
#> 5 89a8100c00fffff 89a8100c033ffff         50          13             0
#> 6 89a8100c00fffff 89a8100c0afffff         50          23             0
```
