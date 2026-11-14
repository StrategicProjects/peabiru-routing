# Detailed itineraries between origin-destination pairs

The itinerary of each trip between pairs of origins and destinations:
its legs (walking, cycling, driving and each ride), with their duration,
waiting time, distance, route and geometry: the direct trip by the
street mode and, with public transport, the fastest trip over the
departure window (`shortest_path = TRUE`) or, with
`shortest_path = FALSE`, the alternatives the multi-criteria router
(McRAPTOR) finds over `time_window` sampled departure times, one per
sequence of routes (the fastest of each), up to `suboptimal_minutes`
slower than the best. A pair's options are ordered by the trip by the
street mode alone first, then by the number of legs, then by duration,
so the first option is not the fastest when a trip with fewer legs (or
the street trip) is slower: on Porto Alegre with
`suboptimal_minutes = 10`, a pair had 102 options, the first of 60.4
minutes and the fastest of 53.3. With a `fare_structure` the trips are
routed with fares (and only those within `max_fare` kept), and the
result gets `total_fare` and `cumulative_fare` columns.

## Usage

``` r
detailed_itineraries(
  network,
  origins,
  destinations,
  mode = "WALK",
  mode_egress = "WALK",
  departure_datetime = NULL,
  time_window = 10L,
  suboptimal_minutes = 0L,
  max_walk_time = Inf,
  max_bike_time = Inf,
  max_car_time = Inf,
  max_trip_duration = 120L,
  walk_speed = 3.6,
  bike_speed = 12,
  max_rides = 3,
  max_lts = 2,
  shortest_path = TRUE,
  all_to_all = FALSE,
  fare_structure = NULL,
  max_fare = Inf,
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

- suboptimal_minutes:

  An integer, 0 (the default): with `shortest_path = FALSE`, how many
  minutes slower than the best an alternative may be. It must be 0 with
  `shortest_path = TRUE` and with a `fare_structure`.

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

- shortest_path:

  A logical. Whether to return only the fastest itinerary (`TRUE`, the
  default).

- all_to_all:

  A logical. Whether to route from every origin to every destination
  (`TRUE`) or pair them row by row (`FALSE`, the default; a single
  origin or destination is repeated to match the other).

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

- n_threads:

  The number of threads; `Inf` (the default) uses all cores (two under
  `R CMD check` when it limits the cores).

- verbose, progress:

  Logicals, checked but without effect: no progress counter or routing
  log is printed.

- drop_geometry:

  A logical. Whether to leave the geometry out and return a table
  without it.

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

With `drop_geometry = FALSE`, an `sf` of `LINESTRING`s in EPSG:4326;
otherwise a table without geometry. One row per leg (`segment`) of each
itinerary (`option`), with `from_id`, `from_lat`, `from_lon`, `to_id`,
`to_lat`, `to_lon`, `option`, `departure_time`, `total_duration`
(minutes), `total_distance` (metres), `segment`, `mode`,
`segment_duration`, `wait` (minutes), `distance` (metres), `route` and,
with `osm_link_ids`, the four id columns; with a `fare_structure`, also
`total_fare` (after `total_distance`) and `cumulative_fare` (after
`mode`), the fare paid up to the end of the leg. Pairs without an
itinerary have no rows. With `output_dir`, one CSV file per pair with
itineraries is written, `from_<origin id>_to_<destination id>.csv` (the
geometry as WKT text), and `output_dir` is returned.

Tables are tibbles by default; `options(peabiru.output = "data.frame")`
gives base `data.frame`s and `options(peabiru.output = "data.table")`
gives `data.table`s (see [peabiru-package](peabiru-package.md)). An `sf`
result keeps its geometry column and EPSG:4326 and takes the same data
frame class: an `sf` tibble, a plain `sf` data frame, or, with
"data.table", an `sf` `data.table` where the section "Class of the
results" of [peabiru-package](peabiru-package.md) says so. With
"data.table", both are `data.table`s.

## Details

Networks whose 'GTFS' has frequency-based trips cannot be used with
public transport (the itineraries of a Monte Carlo draw are not trips
that run).

## Egress by car and turn restrictions

With `mode_egress = "CAR"`, the egress is searched backwards from the
destination, and a turn restriction with two or more via ways can be
read in either order of those ways. peabiru fixes the order: every pair
gets the itinerary of a freshly built network routing that pair alone,
whatever the other pairs, the threads or the earlier calls, and a
warning, once per call, names the pairs whose itineraries the other
order of those via ways would change (on Porto Alegre, about 1 pair in
200).

## See also

Other routing functions: [`accessibility()`](accessibility.md),
[`arrival_travel_time_matrix()`](arrival_travel_time_matrix.md),
[`expanded_travel_time_matrix()`](expanded_travel_time_matrix.md),
[`isochrone()`](isochrone.md),
[`travel_time_matrix()`](travel_time_matrix.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
points <- read.csv(file.path(data_path, "spo_points.csv"))

trips <- detailed_itineraries(
  network, points[1:3, ], points[4:6, ],
  mode = "WALK",
  departure_datetime = as.POSIXct("2019-05-13 14:00:00",
                                  tz = "America/Sao_Paulo"),
  n_threads = 2
)
head(trips)
#> Simple feature collection with 3 features and 16 fields
#> Geometry type: LINESTRING
#> Dimension:     XY
#> Bounding box:  xmin: -46.63879 ymin: -23.55818 xmax: -46.62815 ymax: -23.55311
#> Geodetic CRS:  WGS 84
#> # A tibble: 3 × 17
#>   from_id         from_lat from_lon to_id    to_lat to_lon option departure_time
#>   <chr>              <dbl>    <dbl> <chr>     <dbl>  <dbl>  <int> <chr>         
#> 1 89a8100c00fffff    -23.6    -46.6 89a8100…  -23.6  -46.6      1 14:00:00      
#> 2 89a8100c007ffff    -23.6    -46.6 89a8100…  -23.6  -46.6      1 14:00:00      
#> 3 89a8100c077ffff    -23.6    -46.6 89a8100…  -23.6  -46.6      1 14:00:00      
#> # ℹ 9 more variables: total_duration <dbl>, total_distance <int>,
#> #   segment <int>, mode <chr>, segment_duration <dbl>, wait <dbl>,
#> #   distance <int>, route <chr>, geometry <LINESTRING [°]>
```
