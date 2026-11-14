# Calculate travel time matrix between origin destination pairs considering a time of arrival

For each origin and destination, the trip by public transport that
leaves latest and still arrives by `arrival_datetime` (see
[`travel_time_matrix()`](travel_time_matrix.md) for the modes).
Departures are searched minute by minute from
`arrival_datetime - max_trip_duration`, which may fall on the day
before, and each pair keeps the latest departure that arrives in time,
with its routes and, with `breakdown`, its components.

## Usage

``` r
arrival_travel_time_matrix(
  network,
  origins,
  destinations,
  mode = "WALK",
  mode_egress = "WALK",
  arrival_datetime,
  breakdown = FALSE,
  max_walk_time = Inf,
  max_bike_time = Inf,
  max_car_time = Inf,
  max_trip_duration = 120L,
  walk_speed = 3.6,
  bike_speed = 12,
  max_rides = 3,
  max_lts = 2,
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

- arrival_datetime:

  A POSIXct (or a POSIXlt, which is converted), the time by which trips
  must arrive (see the section "Date and time" of
  [`travel_time_matrix()`](travel_time_matrix.md)).

- breakdown:

  A logical. Whether to break each trip down into access, waiting,
  in-vehicle, transfer and egress times, and the number of rides.

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
  with no row has no file, except in
  [`accessibility()`](accessibility.md). With `output_dir`, `"WALK"` is
  routed from the origins even when there are fewer destinations, ids
  must not be missing, and trips over `max_trip_duration` are not
  blanked out as in the returned result.

## Value

A table with the columns of
[`expanded_travel_time_matrix()`](expanded_travel_time_matrix.md): one
row per origin and destination reached in time. With `output_dir`,
`output_dir`.

Tables are tibbles by default; `options(peabiru.output = "data.frame")`
gives base `data.frame`s and `options(peabiru.output = "data.table")`
gives `data.table`s (see [peabiru-package](peabiru-package.md)).

## Details

As in [`expanded_travel_time_matrix()`](expanded_travel_time_matrix.md),
a street mode alone is an error and at most 5,000 destinations are
accepted.

## See also

Other routing functions: [`accessibility()`](accessibility.md),
[`detailed_itineraries()`](detailed_itineraries.md),
[`expanded_travel_time_matrix()`](expanded_travel_time_matrix.md),
[`isochrone()`](isochrone.md),
[`travel_time_matrix()`](travel_time_matrix.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
points <- read.csv(file.path(data_path, "spo_points.csv"))

attm <- arrival_travel_time_matrix(
  network, points[1:2, ], points,
  mode = c("WALK", "TRANSIT"),
  arrival_datetime = as.POSIXct("2019-05-13 15:00:00",
                                tz = "America/Sao_Paulo"),
  max_trip_duration = 30,
  seed = 42,
  n_threads = 2
)
head(attm)
#> # A tibble: 6 × 6
#>   from_id         to_id           departure_time draw_number routes total_time
#>   <chr>           <chr>           <chr>                <int> <chr>       <dbl>
#> 1 89a8100c00fffff 89a8100c00fffff 14:59:00                 1 [WALK]        0.4
#> 2 89a8100c00fffff 89a8100c007ffff 14:52:00                 1 [WALK]        7.3
#> 3 89a8100c00fffff 89a8100c077ffff 14:49:00                 1 [WALK]       10.9
#> 4 89a8100c00fffff 89a8100c03bffff 14:51:00                 1 [WALK]        8.8
#> 5 89a8100c00fffff 89a8100c033ffff 14:46:00                 1 [WALK]       13.6
#> 6 89a8100c00fffff 89a8100c0afffff 14:36:00                 1 [WALK]       23.1
```
