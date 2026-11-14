# Trip options between origin-destination pairs

For each pair of origin and destination, every trip worth considering:
the trips that no other trip beats on all the chosen criteria at once.
This is a multi-criteria trip planner (see "Not dominated" and "Compared
with the other functions" below).

## Usage

``` r
trip_options(
  network,
  origins,
  destinations,
  mode = c("WALK", "TRANSIT"),
  mode_egress = "WALK",
  departure_datetime = NULL,
  time_window = 10L,
  criteria = NULL,
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
  max_options = Inf,
  walk_resolution = 60L,
  all_to_all = FALSE,
  itineraries = TRUE,
  by_departure = FALSE,
  new_carspeeds = NULL,
  carspeed_scale = 1,
  new_lts = NULL,
  n_threads = Inf,
  verbose = FALSE,
  progress = TRUE,
  drop_geometry = FALSE,
  osm_link_ids = FALSE,
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

  A character vector, the modes as in
  [`detailed_itineraries()`](detailed_itineraries.md); defaults to
  `c("WALK", "TRANSIT")`. Without a public transport mode the only
  option is the direct street trip.

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

- criteria:

  A character vector, some of `"TIME"`, `"FARE"`, `"TRANSFERS"` and
  `"WALK"`, or `NULL` (the default): all four with a `fare_structure`,
  all but `"FARE"` without one.

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

- max_options:

  A number: at most this many options per pair, the first in the order
  of `option`; `Inf` (the default) for all of them.

- walk_resolution:

  A number of seconds, 60 (the default): the step to which minutes on
  foot are compared. By default walking is compared to the whole minute
  (rounded), so that of two trips a few seconds of walking apart and
  otherwise no better only one is returned; `1` compares the exact
  seconds. See "Not dominated".

- all_to_all:

  A logical. Whether to route from every origin to every destination
  (`TRUE`) or pair them row by row (`FALSE`, the default; a single
  origin or destination is repeated to match the other).

- itineraries:

  A logical. Whether to build the legs of each option (`TRUE`, the
  default).

- by_departure:

  A logical. Whether to add the options of every departure minute
  (`FALSE`, the default).

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

- n_threads:

  The number of threads; `Inf` (the default) uses all cores (two under
  `R CMD check` when it limits the cores).

- verbose, progress:

  Logicals, checked but without effect: no progress counter or routing
  log is printed.

- drop_geometry:

  A logical. Whether to leave the geometry of the legs out (and return
  them as a table without geometry).

- osm_link_ids:

  A logical. Whether to add the columns `osm_id_list` (the OpenStreetMap
  way ids of a street leg's edges), `edge_id_list` (the indices of the
  street network's edges, as
  [`street_network_to_sf()`](street_network_to_sf.md) numbers them),
  both as `"[id1, id2, ...]"` and `"[]"` for rides, and `board_stop_id`,
  `alight_stop_id` (a ride's stops, `"<feed>:<stop_id>"`; empty for
  street legs). Consecutive edges on a roundabout give only their first
  ids, and connectors none. Needs `drop_geometry = FALSE`. Defaults to
  `FALSE`.

- output_dir:

  `NULL` or a directory: write one CSV file per pair with options,
  `from_<origin id>_to_<destination id>.csv`, one row per leg (or per
  option, with `itineraries = FALSE`) with the option's and the leg's
  columns and the geometry as WKT, and return `output_dir`.

## Value

A list with

- `options`, a table: one row per option, with `from_id`, `to_id`,
  `option` (from 1, ordered by total time, then fare, transfers and
  minutes on foot), `total_time` (minutes, one decimal place), `fare`
  (with a `fare_structure`), `transfers`, `walk_time` (minutes),
  `departure_time` and `arrival_time` of the option's best minute,
  `n_departures`, `first_departure`, `last_departure` and `description`
  (the modes and route short names, as `"WALK > BUS 343 > WALK"`);

- `itineraries` (with `itineraries = TRUE`): the legs of each option at
  its best minute, in the format of
  [`detailed_itineraries()`](detailed_itineraries.md) and keyed by
  `from_id`, `to_id` and `option`: `segment`, `mode`, `cumulative_fare`
  (with a `fare_structure`), `segment_duration`, `wait`, `distance`,
  `route`, the four id columns with `osm_link_ids`, and the geometry: an
  `sf` of `LINESTRING`s in EPSG:4326, or a table without geometry with
  `drop_geometry = TRUE`;

- `departures` (with `by_departure = TRUE`), a table: for every
  departure minute, the options on its frontier (`from_id`, `to_id`,
  `option`, `departure_time`, `arrival_time`, `total_time`).

Tables are tibbles by default; `options(peabiru.output = "data.frame")`
gives base `data.frame`s and `options(peabiru.output = "data.table")`
gives `data.table`s (see [peabiru-package](peabiru-package.md)). An `sf`
result keeps its geometry column and EPSG:4326 and takes the same data
frame class: an `sf` tibble, a plain `sf` data frame, or, with
"data.table", an `sf` `data.table` where the section "Class of the
results" of [peabiru-package](peabiru-package.md) says so. With
"data.table", the itineraries are an `sf` `data.table`, as those of
[`detailed_itineraries()`](detailed_itineraries.md).

## Not dominated

A trip *dominates* another when it is no worse on every chosen criterion
and better on at least one. The criteria are

- `"TIME"`: door-to-door minutes from the departure minute, waits
  included;

- `"FARE"`: the fare by `fare_structure` (needs one);

- `"TRANSFERS"`: rides minus one; a trip without public transport has 0,
  as many as a trip with a single ride;

- `"WALK"`: minutes on foot, that is the access and egress legs when
  they are walked and the walks between stops. A bicycle or car leg
  counts 0, also where the cyclist pushes the bicycle.

At each departure minute of the window (`departure_datetime`, plus one
minute at a time for `time_window` minutes) the options are the exact
set of trips not dominated, one trip per combination of values: of trips
with the same values on the chosen criteria, the one with the least
total time, then transfers, minutes on foot and fare is kept. Trips are
built as in the other routing functions, so that their times agree with
[`detailed_itineraries()`](detailed_itineraries.md): a direct trip by
the street mode, and trips that reach a stop of the access search, ride
and walk between stops (the transfers of the other functions), and leave
a stop of the egress search; each ride boards at least a minute after
its stop is reached, on the vehicle that gets to the alighting stop
first.

The search is exact; what is returned is then compared at the resolution
the values are read at: total time to a tenth of a minute (as it is
reported), and minutes on foot to the whole minute (rounded,
`walk_resolution` seconds). Of the trips of a minute's exact set, one is
left out when another is no worse on every criterion in those terms and
better on one, or equal on all and better in exact terms (least total
time, then transfers, walking, fare); two trips a few seconds of walking
apart are not both returned. The values returned are exact. Rounding
cannot make a trip look better than one that beats it, so this is the
same as comparing every trip at that resolution.

The minutes are then merged into options: trips with the same street
modes, the same routes between the same stops (whichever vehicles), and
the same fare, transfers and minutes on foot are one option, available
at `n_departures` minutes from `first_departure` to `last_departure`.
Its total time changes with the minute (the wait at the start does); the
option reports the trip of its best minute, the one with the least total
time (the earliest of equal ones). With `by_departure = TRUE` the result
also has every minute's options.

Frequency-based routes (GTFS `frequencies.txt`) are boarded at their
expected times: half a headway after the boarding wait (or after the
start of the period), within the period; with `exact_times = 1` at the
first scheduled departure. There are no random draws and no `seed`, and
the results differ on purpose from the Monte Carlo draws of the other
routing functions.

## Fares

Fares follow `fare_structure` exactly as
[`pareto_frontier()`](pareto_frontier.md) and
[`detailed_itineraries()`](detailed_itineraries.md) compute them (in
cents). A fare is not the sum of its rides (discounted transfers within
a time allowance, unlimited transfers, a cap), so two partial trips are
only compared on fare when one is at least as cheap for every way both
can continue; the comparison makes no assumption about the structure.
This is what makes fare criteria slower than the others: trips waiting
for a discount are kept apart until they cannot get one any more.

## Compared with the other functions

[`pareto_frontier()`](pareto_frontier.md) gives, for each fare cutoff,
the percentiles of the travel time over Monte Carlo draws of departure
times; [`detailed_itineraries()`](detailed_itineraries.md) gives the
fastest trip, or with `shortest_path = FALSE` the alternatives, one per
sequence of routes. `trip_options()` gives the trade-offs themselves:
the trips that are faster, cheaper, with fewer transfers or less walking
than the others, each with its legs. With `criteria = "TIME"` and
`time_window = 1` its first option has the total time of
[`detailed_itineraries()`](detailed_itineraries.md)'s fastest trip.

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
points <- read.csv(file.path(data_path, "spo_points.csv"))

trips <- trip_options(
  network, points[10, ], points[12, ],
  departure_datetime = as.POSIXct("2019-05-13 14:00:00",
                                  tz = "America/Sao_Paulo"),
  n_threads = 2
)
trips$options
#> # A tibble: 4 × 12
#>   from_id         to_id     option total_time transfers walk_time departure_time
#>   <chr>           <chr>      <int>      <dbl>     <int>     <dbl> <chr>         
#> 1 89a8100c037ffff 89a8100c…      1       31.8         0      26.1 14:00:00      
#> 2 89a8100c037ffff 89a8100c…      2       34.3         1      22.4 14:00:00      
#> 3 89a8100c037ffff 89a8100c…      3       34.7         0      24.2 14:00:00      
#> 4 89a8100c037ffff 89a8100c…      4       34.9         0      19.7 14:00:00      
#> # ℹ 5 more variables: arrival_time <chr>, n_departures <int>,
#> #   first_departure <chr>, last_departure <chr>, description <chr>
```
