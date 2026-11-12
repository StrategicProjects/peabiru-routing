#' Calculate travel time matrix between origin destination pairs considering a
#' departure time
#'
#' Travel times between every origin and every destination, by one street
#' mode (`"WALK"`, `"BICYCLE"` or `"CAR"`) or by public transport (all of it
#' or some of its modes), reached by walking, bicycle or car and left by
#' any of them (`mode_egress`), for departures every minute of a time
#' window. For trips that must arrive by a given time, see
#' [arrival_travel_time_matrix()]; for each departure of the window on its
#' own, [expanded_travel_time_matrix()].
#'
#' A bicycle or car trip to public transport goes on on foot from wherever
#' the vehicle got, within the same `max_bike_time` or `max_car_time` (so
#' stops in pedestrian-only areas are reached), and the trip without public
#' transport keeps those walking ends too. Egress by bicycle or car is
#' searched from every stop up to 5 km by bicycle at any level of traffic
#' stress and up to 30 minutes by car, and must stay within
#' `max_bike_time` or `max_car_time`.
#'
#' @section Transport modes:
#' Street modes: `"WALK"`, `"BICYCLE"`, `"CAR"`. Public transport:
#' `"TRANSIT"` (every mode) or any of `"TRAM"`, `"SUBWAY"`, `"RAIL"`,
#' `"BUS"`, `"FERRY"`, `"CABLE_CAR"`, `"GONDOLA"` and `"FUNICULAR"`, which
#' keep only the routes of those 'GTFS' route types. With public transport,
#' the street mode in `mode` (`"WALK"` when none is given) is the access
#' mode, used to reach the first stop and, on foot, between stops.
#'
#' @section Level of traffic stress:
#' Each street is given a level of traffic stress (LTS) for cyclists from
#' its 'OpenStreetMap' tags, as R5 assigns it: 1 is tolerable for
#' children (quiet streets, separated cycle tracks), 2 for most adults, 3
#' for confident cyclists, 4 only for the strong and fearless. A cyclist
#' rides only on streets up to `max_lts` and walks the bicycle elsewhere
#' (also where the street is not rideable, such as stairs). A way tagged
#' `lts` in the `.pbf` file keeps that level.
#'
#' @section Date and time:
#' `departure_datetime` (and `arrival_datetime` in
#' [arrival_travel_time_matrix()]) is read as the date and clock time it
#' shows in its own time zone, and that clock time is taken as the local
#' time of the 'GTFS' feeds: `as.POSIXct("2019-05-13 14:00:00", tz =
#' "Europe/Paris")` means 14:00 in the study area, wherever it is. It must
#' fall on a date with service (see [check_transit_availability()]).
#'
#' @section Departures and Monte Carlo draws:
#' Without fares, a trip is routed for every departure minute of
#' `time_window` and the travel times are summarised by `percentiles`
#' over those departures. Routes given by frequencies (`frequencies.txt`)
#' have no timetable: for each minute `draws_per_minute` random timetables
#' consistent with the frequencies are drawn, so the results are a Monte
#' Carlo sample. Give `seed` to make them reproducible (`set.seed()` has no
#' effect on these draws); with timetables only, the draws change nothing.
#'
#' With a `fare_structure` (see [setup_fare_structure()]) public transport
#' is routed with R5's multi-criteria router (McRAPTOR), over
#' `time_window * draws_per_minute` departure times sampled at random in
#' the window (the same for an origin in every call with the same `seed`),
#' keeping at each stop the fastest trip whose fare is within `max_fare`.
#' R5's sampler cannot draw exactly 200 departure times, nor many per minute
#' over a long window (such as 8 per minute in a 10-minute window, or 5 in a
#' window of 150 minutes or more): those combinations are an error.
#'
#' @section Compatibility with r5r:
#' The arguments, their defaults and checks, the columns and the rows of the
#' result are those of `r5r::travel_time_matrix()` in r5r 2.5.0, and the
#' travel times are equal pair by pair (with frequencies, for the same
#' `seed`): the engine is a port of R5 7.6, the engine r5r drives. With
#' `options(peabiru.output = "data.table")` the result is identical to
#' r5r's, its class included. For a walking trip with more origins than
#' destinations the search runs from the destinations, as r5r runs it, and
#' the columns are swapped back. An `sf` of points (or of scenario
#' polygons or lines) in another CRS than WGS 84 is reprojected, where r5r
#' stops with an error. A
#' missing destination id is an error (r5r fails on it in Java). See
#' the article "Coming from r5r" on the package website for every difference.
#'
#' @param r5r_network A network built with [build_network()].
#' @param origins,destinations A `data.frame` with columns `id`, `lon` and
#'   `lat` (WGS 84 longitude and latitude), or a `POINT sf` with an `id`
#'   column, in any coordinate reference system (CRS): an `sf` in another
#'   CRS than WGS 84 is reprojected with `sf::st_transform()`, with a
#'   message, and one without a CRS is an error. Ids that are not
#'   character are converted, with a warning; points with missing
#'   coordinates are not routed, with a warning; duplicated ids are an
#'   error.
#' @param mode A character vector, the transport modes (see the section
#'   "Transport modes"). Defaults to `"WALK"`.
#' @param mode_egress A string, the mode from the last stop to the
#'   destination: `"WALK"` (the default), `"BICYCLE"` or `"CAR"`. Ignored
#'   without public transport.
#' @param departure_datetime A POSIXct, the departure date and time (see
#'   the section "Date and time"). Required with public transport.
#' @param time_window An integer, the number of departure minutes, from
#'   `departure_datetime` on, over which travel times are computed.
#'   Defaults to 10.
#' @param percentiles An integer vector of up to 5 values from 1 to 99, the
#'   percentiles of the travel time over the departures of the window to
#'   report. Defaults to 50, the median.
#' @param max_walk_time,max_bike_time,max_car_time The maximum time in
#'   minutes on foot, by bicycle and by car. Each limit applies to each leg
#'   on its own (15 minutes to reach the first stop and another 15 from the
#'   last), and to the whole trip when that mode is the only one. `Inf`, the
#'   default, leaves only `max_trip_duration`.
#' @param max_trip_duration An integer, the maximum trip duration in
#'   minutes. Defaults to 120.
#' @param walk_speed,bike_speed Average speeds in km/h, 3.6 and 12 by
#'   default.
#' @param max_rides An integer, the maximum number of public transport rides
#'   in a trip. Defaults to 3.
#' @param max_lts An integer from 1 to 4, the highest level of traffic stress
#'   a cyclist rides on (see the section "Level of traffic stress").
#'   Defaults to 2.
#' @param fare_structure A fare structure (see [setup_fare_structure()]) or
#'   `NULL` (the default): no fares. It must have a row for every route of
#'   the network.
#' @param max_fare A number, the highest fare of a trip (`Inf`, the default:
#'   no limit), in the currency of the fare structure; a finite value needs
#'   a `fare_structure`.
#' @param new_carspeeds A scenario of car speeds: `NULL` (the default), a
#'   data frame with columns `osm_id`, `max_speed` and `speed_type`
#'   (`"scale"`, a factor for the car speed of the way's edges, or `"km/h"`,
#'   a speed; 0 closes the way to cars), or an `sf` data frame of `POLYGON`s
#'   or `MULTIPOLYGON`s (in any CRS, reprojected to WGS 84 as `origins`)
#'   with columns `poly_id` (character), `scale`
#'   (numeric) and `priority` (integer): each street edge takes the factor
#'   of the polygon it meets with the highest priority (ties: the one it
#'   overlaps most). Polygons that are not valid are an error, and so are
#'   ties that cannot be settled exactly as R5 settles them.
#' @param carspeed_scale A number of at least 0, the factor for the car
#'   speeds of the edges `new_carspeeds` leaves out (all of them when it is
#'   `NULL`). Defaults to 1. R5 keeps car speeds as 16-bit centimetres per
#'   second: a factor or speed that would bring a speed to 0 or less, or
#'   past that range, is an error.
#' @param new_lts A scenario of levels of traffic stress: `NULL` (the
#'   default), a data frame with columns `osm_id` and `lts` (integer), or an
#'   `sf` data frame of `LINESTRING`s (in any CRS, reprojected to WGS 84 as
#'   `origins`) with columns `line_id`, `lts`
#'   (1 to 4) and `priority`: each street edge takes the level of the line
#'   closest to it among those whose bounding box meets its own (`priority`
#'   is not used). A scenario (of car speeds or of levels of traffic stress)
#'   changes the street search from the origins (the trip without public
#'   transport, and access by bicycle or car) but not the egress from the
#'   stops, and turns off slope costs, with a warning, on a network built
#'   with an elevation raster.
#' @param draws_per_minute An integer, the Monte Carlo draws per departure
#'   minute (see the section "Departures and Monte Carlo draws"). Defaults
#'   to 5.
#' @param seed An integer seed for the Monte Carlo draws, or `NULL` (the
#'   default: the draws differ from call to call). With a seed, the same
#'   inputs give the same results whatever `n_threads`.
#' @param n_threads The number of threads; `Inf` (the default) uses all cores
#'   (two under `R CMD check` when it limits the cores).
#' @param verbose,progress Logicals, checked but without effect: no progress
#'   counter or routing log is printed.
#' @param output_dir `NULL` (the default), or the path of an existing
#'   directory: instead of returning the result, write it there, one CSV
#'   file per origin, `from_<origin id>.csv` (an id that cannot name a file there, such as one with a path
#'   separator, is an error; an
#'   existing file of that name is replaced, other files are left alone),
#'   and return `output_dir`. The origins are routed a slice at a time and
#'   each slice is written before the next one is routed, so the whole
#'   result is never held in memory. The files have the columns of the
#'   result (with `cutoff` also under `"fixed_exponential"`, as the
#'   placeholder `0`), fields with `,` or `"` quoted, `NA` as an empty
#'   field, and doubles as Java writes them (`2.0`, `1.0E-4`), byte for byte
#'   the files r5r writes; an origin with no row has no file, except in
#'   [accessibility()]. With `output_dir`, `"WALK"` is routed from the
#'   origins even when there are fewer destinations, ids must not be
#'   missing, and trips over `max_trip_duration` are not blanked out as in
#'   the returned result.
#' @param r5r_core Deprecated: the network under its name before r5r 2.3.0,
#'   accepted with a warning and used as `r5r_network`.
#'
#' @return A table with `from_id`, `to_id` (character) and one
#'   integer column `travel_time_pXX` per percentile (minutes). Pairs that
#'   cannot be completed within `max_trip_duration`, or whose points are
#'   too far from the street network, are not returned; a higher percentile
#'   beyond the limit is `NA`. With `output_dir`, `output_dir`.
#'
#'   `r doc_output_class()`
#' @family routing functions
#' @export
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' points <- read.csv(file.path(data_path, "spo_points.csv"))
#'
#' # walking
#' ttm <- travel_time_matrix(network, points, points, mode = "WALK",
#'                           n_threads = 2)
#' head(ttm)
#'
#' # public transport
#' ttm <- travel_time_matrix(
#'   network, points, points,
#'   mode = c("WALK", "TRANSIT"),
#'   departure_datetime = as.POSIXct("2019-05-13 14:00:00",
#'                                   tz = "America/Sao_Paulo"),
#'   percentiles = c(25, 50, 75),
#'   seed = 42,
#'   n_threads = 2
#' )
#' head(ttm)
travel_time_matrix <- function(r5r_network,
                               origins,
                               destinations,
                               mode = "WALK",
                               mode_egress = "WALK",
                               departure_datetime = NULL,
                               time_window = 10L,
                               percentiles = 50L,
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
                               output_dir = NULL,
                               r5r_core = deprecated()) {
  # mirrors r5r R/travel_time_matrix.R
  kind <- output_class()
  if (is_present(r5r_core)) {
    warn_r5r_core()
    r5r_network <- r5r_core
  }
  origins <- assign_points_input(origins, "origins")
  destinations <- assign_points_input(destinations, "destinations")
  # r5r's Java side fails on a missing destination id (NullPointerException)
  if (anyNA(destinations$id)) {
    stop("'destinations$id' cannot be missing (r5r 2.5.0 fails on a missing ",
         "destination id in Java).", call. = FALSE)
  }
  mode_list <- assign_mode(mode, mode_egress)
  departure <- assign_departure(departure_datetime, mode_list)

  assert_network(r5r_network)
  if (mode_list$transit) {
    check_transit_availability_on_date(r5r_network, departure)
  }

  # as in r5r, not with output_dir: the files are named after the origins
  # routed from
  swapped <- is.null(output_dir) && swap_od(origins, destinations, mode_list, r5r_network)
  if (swapped) {
    temp <- origins
    origins <- destinations
    destinations <- temp
  }

  max_walk_time <- assign_max_street_time(max_walk_time, walk_speed, max_trip_duration, "walk")
  max_bike_time <- assign_max_street_time(max_bike_time, bike_speed, max_trip_duration, "bike")
  max_car_time <- assign_max_street_time(max_car_time, NULL, max_trip_duration, "car")
  max_trip_duration <- assign_max_trip_duration(
    max_trip_duration, mode_list, max_walk_time, max_bike_time, max_car_time
  )

  assert_count(time_window, "time_window", positive = TRUE)
  percentiles <- assign_percentiles(percentiles)
  check_draws(draws_per_minute, time_window, mcraptor = !is.null(fare_structure))
  seed <- seed_args(seed)
  check_speed(walk_speed, "walk")
  check_speed(bike_speed, "bike")
  assert_count(max_rides, "max_rides", positive = TRUE)
  check_max_lts(max_lts)
  threads <- assign_n_threads(n_threads)
  check_messages(verbose, progress)
  check_fares(fare_structure, max_fare, r5r_network)
  check_output_dir(output_dir)
  scenario <- scenario_spec(new_carspeeds, carspeed_scale, new_lts, r5r_network)

  pct_names <- sprintf("travel_time_p%02d", percentiles)
  output <- csv_output(output_dir, origins, destinations,
                       percentiles = as.integer(percentiles))
  if (mode_list$transit) {
    res <- raise_if_stopped(rs_transit_ttm(
      r5r_network$ptr, origins$lat, origins$lon, destinations$lat, destinations$lon,
      mode_list$transit_mode, mode_list$access_mode, mode_list$egress_mode,
      as.double(bike_speed),
      as.integer(max_lts), max_bike_time, max_car_time,
      departure$yyyymmdd, departure$seconds, as.integer(time_window),
      percentiles, max_walk_time, max_trip_duration, as.double(walk_speed),
      as.integer(max_rides), as.integer(draws_per_minute), seed$has, seed$value,
      threads, scenario, output, fare_structure, as.double(max_fare)
    ))
    if (!is.null(output_dir)) return(output_dir)
    times <- res[-(1:2)]
  } else {
    res <- raise_if_stopped(rs_street_ttm(
      r5r_network$ptr, origins$lat, origins$lon, destinations$lat, destinations$lon,
      mode_list$direct_modes, as.double(walk_speed), as.double(bike_speed),
      as.integer(max_lts), max_trip_duration, threads, scenario, output
    ))
    if (!is.null(output_dir)) return(output_dir)
    # a street mode has one travel time, reported for every percentile
    times <- rep(list(res[[3]]), length(percentiles))
  }
  names(times) <- pct_names

  if (swapped) {
    # r5r reverse_back_if_direct_mode(): the routed destinations are the
    # caller's origins
    ids <- list(from_id = destinations$id[res[[2]]], to_id = origins$id[res[[1]]])
  } else {
    ids <- list(from_id = origins$id[res[[1]]], to_id = destinations$id[res[[2]]])
  }
  travel_times <- new_df(c(ids, times))

  # r5r: travel times of non-viable trips become NA, in every percentile
  # column
  for (j in pct_names) {
    over <- which(travel_times[[j]] > max_trip_duration)
    travel_times[[j]][over] <- NA_integer_
  }
  as_output(travel_times, kind)
}
