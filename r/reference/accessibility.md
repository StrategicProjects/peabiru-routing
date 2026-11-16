# Calculate access to opportunities

Cumulative or decay-weighted access to the opportunities at the
destinations from each origin, by one street mode (`"WALK"`, `"BICYCLE"`
or `"CAR"`) or by public transport (see
[`travel_time_matrix()`](travel_time_matrix.md) for the modes). For each
origin, the opportunities of every destination are summed, each weighted
by the decay function of the travel time to it, for every percentile of
the travel time over the departure window and every cutoff.

## Usage

``` r
accessibility(
  network,
  origins,
  destinations,
  opportunities_colnames = "opportunities",
  mode = "WALK",
  mode_egress = "WALK",
  departure_datetime = NULL,
  time_window = 10L,
  percentiles = 50L,
  decay_function = "step",
  cutoffs = NULL,
  decay_value = NULL,
  max_walk_time = Inf,
  max_bike_time = Inf,
  max_car_time = Inf,
  max_trip_duration = 120L,
  walk_speed = 3.6,
  bike_speed = 12,
  max_rides = 3,
  max_lts = 2,
  fare_structure = NULL,
  max_fare = Inf,
  new_carspeeds = NULL,
  carspeed_scale = 1,
  new_lts = NULL,
  draws_per_minute = 5L,
  seed = NULL,
  n_threads = Inf,
  verbose = FALSE,
  progress = TRUE,
  output_dir = NULL
)
```

## Arguments

- network:

  A network built with [`build_network()`](build_network.md).

- origins:

  A `data.frame` with columns `id`, `lon` and `lat` (WGS 84), or a
  `POINT sf` with an `id` column, in any coordinate reference system
  (reprojected to WGS 84, see
  [`travel_time_matrix()`](travel_time_matrix.md)).

- destinations:

  A `data.frame` or `POINT sf` with `id`, `lon`, `lat` and the
  opportunity columns.

- opportunities_colnames:

  A character vector, the names of the numeric columns of `destinations`
  with the opportunity counts. Missing values count as 0, with a
  warning.

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

- percentiles:

  An integer vector of up to 5 values from 1 to 99, the percentiles of
  the travel time over the departures of the window to report. Defaults
  to 50, the median.

- decay_function:

  A string: `"step"`, `"exponential"`, `"fixed_exponential"`, `"linear"`
  or `"logistic"`.

- cutoffs:

  An integer vector of up to 12 distinct values from 1 to 120 (minutes).
  Must be `NULL` with `"fixed_exponential"` and given otherwise.

- decay_value:

  A number, the parameter of the decay function: the decay constant for
  `"fixed_exponential"` (between 0 and 1), the width in minutes for
  `"linear"` (a whole number from 1 to 59), the standard deviation in
  minutes for `"logistic"` (at least 1); `NULL` for `"step"` and
  `"exponential"`.

- max_walk_time, max_bike_time, max_car_time:

  The maximum time in minutes on foot, by bicycle and by car. Each limit
  applies to each leg on its own (15 minutes to reach the first stop and
  another 15 from the last), and to the whole trip when that mode is the
  only one. `Inf`, the default, leaves only `max_trip_duration`.

- max_trip_duration:

  An integer, the maximum trip duration in minutes. Defaults to 120.

- walk_speed, bike_speed:

  Average speeds in km/h, 3.6 and 12 by default.

- max_rides:

  An integer, the maximum number of public transport rides in a trip.
  Defaults to 3.

- max_lts:

  An integer from 1 to 4, the highest level of traffic stress a cyclist
  rides on (see the section "Level of traffic stress"). Defaults to 2.

- fare_structure:

  A fare structure (see
  [`setup_fare_structure()`](setup_fare_structure.md)) or `NULL` (the
  default): no fares. It must have a row for every route of the network.

- max_fare:

  A number, the highest fare of a trip (`Inf`, the default: no limit),
  in the currency of the fare structure; a finite value needs a
  `fare_structure`.

- new_carspeeds:

  A scenario of car speeds: `NULL` (the default), a data frame with
  columns `osm_id`, `max_speed` and `speed_type` (`"scale"`, a factor
  for the car speed of the way's edges, or `"km/h"`, a speed; 0 closes
  the way to cars), or an `sf` data frame of `POLYGON`s or
  `MULTIPOLYGON`s (in any CRS, reprojected to WGS 84 as `origins`) with
  columns `poly_id` (character), `scale` (numeric) and `priority`
  (integer): each street edge takes the factor of the polygon it meets
  with the highest priority (ties: the one it overlaps most). Polygons
  that are not valid are an error, and so is a tie with no winner: two
  polygons of the same priority and different scales that overlap a way
  by the same length (give one of them a higher priority).

- carspeed_scale:

  A number of at least 0, the factor for the car speeds of the edges
  `new_carspeeds` leaves out (all of them when it is `NULL`). Defaults
  to 1. Car speeds are kept as 16-bit centimetres per second: a factor
  or speed that would bring a speed to 0 or less, or past that range, is
  an error.

- new_lts:

  A scenario of levels of traffic stress: `NULL` (the default), a data
  frame with columns `osm_id` and `lts` (integer), or an `sf` data frame
  of `LINESTRING`s (in any CRS, reprojected to WGS 84 as `origins`) with
  columns `line_id`, `lts` (1 to 4) and `priority`: each street edge
  takes the level of the line closest to it among those whose bounding
  box meets its own (`priority` is not used). A scenario (of car speeds
  or of levels of traffic stress) changes the street search from the
  origins (the trip without public transport, and access by bicycle or
  car) but not the egress from the stops, and turns off slope costs,
  with a warning, on a network built with an elevation raster.

- draws_per_minute:

  An integer, the Monte Carlo draws per departure minute (see the
  section "Departures and Monte Carlo draws"). Defaults to 5.

- seed:

  An integer seed for the Monte Carlo draws, or `NULL` (the default: the
  draws differ from call to call). With a seed, the same inputs give the
  same results whatever `n_threads`.

- n_threads:

  The number of threads; `Inf` (the default) uses all cores (two under
  `R CMD check` when it limits the cores).

- verbose, progress:

  Logicals, checked but without effect: no progress counter or routing
  log is printed.

- output_dir:

  `NULL` (the default), or the path of an existing directory: instead of
  returning the result, write it there, one CSV file per origin,
  `from_<origin id>.csv` (an id that cannot name a file there, such as
  one with a path separator, is an error; an existing file of that name
  is replaced, other files are left alone), and return `output_dir`. The
  origins are routed a slice at a time and each slice is written before
  the next one is routed, so the whole result is never held in memory.
  The files have the columns of the result (with `cutoff` also under
  `"fixed_exponential"`, as the placeholder `0`), fields with `,` or `"`
  quoted, `NA` as an empty field, and doubles in the shortest form that
  reads back the same, with a decimal point (`2.0`, `1.0E-4`); an origin
  with no row has no file, except in `accessibility()`. With
  `output_dir`, `"WALK"` is routed from the origins even when there are
  fewer destinations, ids must not be missing, and trips over
  `max_trip_duration` are not blanked out as in the returned result.

## Value

A table with `id`, `opportunity`, `percentile`, `cutoff` (absent with
`"fixed_exponential"`) and `accessibility`, one row per origin,
opportunity, percentile and cutoff, in that order. With `output_dir`,
`output_dir`.

Tables are tibbles by default; `options(peabiru.output = "data.frame")`
gives base `data.frame`s and `options(peabiru.output = "data.table")`
gives `data.table`s (see [peabiru-package](peabiru-package.md)).

## Decay functions

With `t` the travel time and `c` a cutoff:

- `"step"`: weight 1 below the cutoff, 0 from it on (cumulative
  opportunities). Trips longer than the largest cutoff are not routed
  (`max_trip_duration` becomes `max(cutoffs)`).

- `"exponential"`: `2^(-t / c)`, half the weight at the cutoff; 0 from
  two hours on.

- `"fixed_exponential"`: `exp(-decay_value * t)` with `t` in seconds,
  without cutoffs; 0 from two hours on.

- `"linear"`: 1 up to `decay_value / 2` minutes before the cutoff, 0
  from `decay_value / 2` minutes after it, falling linearly in between.

- `"logistic"`: a logistic curve with its median at the cutoff and
  standard deviation `decay_value` minutes, scaled to 1 at `t = 0`.

The weights are computed with a table-based exponential (a translation
of `FastMath.exp()` of Apache Commons Math), so the sums are the same,
to the last bit, on every machine. Except with `"step"`,
`max_trip_duration` may not be shorter than the largest cutoff.

With a `fare_structure`, public transport is routed with fares as in
[`travel_time_matrix()`](travel_time_matrix.md) (see its section
"Departures and Monte Carlo draws").

## See also

Other routing functions:
[`arrival_travel_time_matrix()`](arrival_travel_time_matrix.md),
[`detailed_itineraries()`](detailed_itineraries.md),
[`expanded_travel_time_matrix()`](expanded_travel_time_matrix.md),
[`isochrone()`](isochrone.md),
[`travel_time_matrix()`](travel_time_matrix.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
points <- read.csv(file.path(data_path, "spo_points.csv"))

access <- accessibility(
  network, points, points,
  opportunities_colnames = c("jobs", "schools"),
  mode = c("WALK", "TRANSIT"),
  departure_datetime = as.POSIXct("2019-05-13 14:00:00",
                                  tz = "America/Sao_Paulo"),
  decay_function = "logistic",
  cutoffs = c(15, 30),
  decay_value = 5,
  seed = 42,
  n_threads = 2
)
head(access)
#> # A tibble: 6 × 5
#>   id              opportunity percentile cutoff accessibility
#>   <chr>           <chr>            <int>  <int>         <dbl>
#> 1 89a8100c00fffff jobs                50     15     13316.   
#> 2 89a8100c00fffff jobs                50     30     77956.   
#> 3 89a8100c00fffff schools             50     15         0.881
#> 4 89a8100c00fffff schools             50     30         4.37 
#> 5 89a8100c007ffff jobs                50     15     17187.   
#> 6 89a8100c007ffff jobs                50     30     92597.   
```
