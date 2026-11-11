#' Calculate travel time matrix between origin destination pairs considering a
#' departure time
#'
#' Travel times between every origin and every destination, by one street
#' mode (`"WALK"`, `"BICYCLE"` or `"CAR"`) or by public transport (all of it
#' or some of its modes), reached by walking, bicycle or car and left by
#' any of them (`mode_egress`). Arguments, defaults and the result are those
#' of `r5r::travel_time_matrix()` (r5r 2.5.0), and so are the values:
#' peabiru reproduces R5's routing exactly.
#'
#' As in R5, a bicycle or car trip to public transport goes on on foot from
#' wherever the vehicle got, within the same `max_bike_time` or
#' `max_car_time` (so stops in pedestrian-only areas are reached), and the
#' trip without public transport keeps those walking ends too. Egress by
#' bicycle or car is searched from every stop with R5's defaults (up to
#' 5 km by bicycle at any level of traffic stress, up to 30 minutes by
#' car) and must stay within `max_bike_time` or `max_car_time`.
#'
#' Not supported yet, and an error when requested: fares (`fare_structure`,
#' `max_fare`).
#'
#' @param r5r_network A network built with [build_network()].
#' @param origins,destinations A `data.frame` with columns `id`, `lon` and
#'   `lat`, or a `POINT sf` with an `id` column, in WGS 84.
#' @param mode A character vector, the transport modes: one of `"WALK"`,
#'   `"BICYCLE"` or `"CAR"`, and for public transport `"TRANSIT"` (every
#'   transit mode) or any of `"TRAM"`, `"SUBWAY"`, `"RAIL"`, `"BUS"`,
#'   `"FERRY"`, `"CABLE_CAR"`, `"GONDOLA"` and `"FUNICULAR"`; the street
#'   mode, `"WALK"` by default with transit, is the access mode.
#' @param mode_egress A string, the mode from the last stop to the
#'   destination: `"WALK"`, `"BICYCLE"` or `"CAR"`.
#' @param departure_datetime A POSIXct, the departure date and time. Its
#'   clock time and date are taken in its own time zone. Required with
#'   transit.
#' @param time_window An integer, the window of departure minutes, starting
#'   at `departure_datetime`, over which travel times are computed.
#' @param percentiles An integer vector of up to 5 values from 1 to 99, the
#'   travel time percentiles over the time window to report.
#' @param max_walk_time,max_bike_time,max_car_time The maximum time in
#'   minutes for each street mode (`Inf`, the default, means the trip limit).
#' @param max_trip_duration An integer, the maximum trip duration in minutes.
#' @param walk_speed,bike_speed Speeds in km/h.
#' @param max_rides An integer, the maximum number of public transport rides.
#' @param max_lts An integer from 1 to 4, the highest level of traffic stress
#'   a cyclist will ride on.
#' @param fare_structure,max_fare Fares; not supported yet (`fare_structure`
#'   must be `NULL` and `max_fare` `Inf`).
#' @param new_carspeeds A scenario of car speeds, as in r5r: `NULL`, a data
#'   frame with columns `osm_id`, `max_speed` and `speed_type` (`"scale"`, a
#'   factor for the car speed of the way's edges, or `"km/h"`, a speed; 0
#'   closes the way to cars), or an `sf` data frame of `POLYGON`s or
#'   `MULTIPOLYGON`s in WGS 84 with columns `poly_id` (character), `scale`
#'   (numeric) and `priority` (integer): each street edge takes the factor
#'   of the polygon it meets with the highest priority (ties: the one it
#'   overlaps most).
#' @param carspeed_scale A number of at least 0, the factor for the car
#'   speeds of the edges `new_carspeeds` leaves out (all of them when it is
#'   `NULL`).
#' @param new_lts A scenario of levels of traffic stress, as in r5r: `NULL`,
#'   a data frame with columns `osm_id` and `lts` (integer), or an `sf` data
#'   frame of `LINESTRING`s in WGS 84 with columns `line_id`, `lts` (1 to 4)
#'   and `priority`: each street edge takes the level of the line closest to
#'   it among those whose bounding box meets its own. As in r5r, the
#'   scenario changes the street search from the origins (the trip without
#'   public transport, and access by bicycle or car) but not the egress
#'   from the stops, and `priority` is not used. A car speed that R5's
#'   16-bit cm/s would turn into 0 or less (a factor too small or too
#'   large) is an error, as are polygons that are not valid and ties
#'   between polygons that peabiru cannot settle exactly as R5 does.
#' @param draws_per_minute An integer, the Monte Carlo draws per departure
#'   minute for frequency-based 'GTFS'.
#' @param seed An integer seed for the Monte Carlo draws, or `NULL`. With
#'   frequency-based 'GTFS', results are reproducible (and equal to r5r's)
#'   only with a seed.
#' @param n_threads The number of threads; `Inf` (the default) uses all cores.
#' @param verbose,progress Logicals, checked as in r5r; peabiru prints no
#'   progress counter.
#' @param output_dir `NULL`, or the path of an existing directory: instead of
#'   returning the result, write it there as r5r does, one CSV file per
#'   origin, `from_<origin id>.csv` (the id is not sanitized; an existing
#'   file of that name is replaced, other files are left alone), and return
#'   `output_dir`. The origins are routed a slice at a time and each slice is
#'   written before the next one is routed, so the whole result is never held
#'   in memory. The files are r5r's byte for byte: the columns of the result
#'   (with `cutoff` also under `"fixed_exponential"`, as the placeholder
#'   `0`), fields with `,` or `"` quoted, `NA` as an empty field, doubles
#'   as Java writes them (`2.0`, `1.0E-4`); an origin with no row has no
#'   file, except in [accessibility()]. As in r5r, `"WALK"` is then routed
#'   from the origins even when there are fewer destinations, ids must not
#'   be missing, and the rule that blanks out trips over
#'   `max_trip_duration` in the returned result is not applied to the
#'   files.
#' @param r5r_core Deprecated: the network under the name r5r used before
#'   version 2.3.0. As in r5r, it is accepted with a warning and used as
#'   `r5r_network`.
#'
#' @return A `data.frame` with `from_id`, `to_id` and one integer column
#'   `travel_time_pXX` per percentile (minutes). Pairs that cannot be
#'   completed within `max_trip_duration` are not returned; a higher
#'   percentile beyond the limit is `NA`. With `output_dir`, `output_dir`.
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
                               r5r_core) {
  # mirrors r5r R/travel_time_matrix.R
  if (!missing(r5r_core)) {
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
  check_draws(draws_per_minute)
  seed <- seed_args(seed)
  check_speed(walk_speed, "walk")
  check_speed(bike_speed, "bike")
  assert_count(max_rides, "max_rides", positive = TRUE)
  check_max_lts(max_lts)
  threads <- assign_n_threads(n_threads)
  check_messages(verbose, progress)
  check_fares(fare_structure, max_fare)
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
      threads, scenario, output
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
  travel_times
}
