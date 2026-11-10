#' Calculate travel time matrix between origin destination pairs considering a
#' departure time
#'
#' Travel times between every origin and every destination, by one street
#' mode (`"WALK"`, `"BICYCLE"` or `"CAR"`) or by public transport with walk
#' access and egress (`c("WALK", "TRANSIT")`). Arguments, defaults and the
#' result are those of `r5r::travel_time_matrix()` (r5r 2.5.0), and so are
#' the values: peabiru reproduces R5's routing exactly.
#'
#' Not supported yet, and an error when requested: transit with `"BICYCLE"`
#' or `"CAR"` access or a non-walk `mode_egress`; a subset of transit modes
#' (such as `"BUS"`); fares (`fare_structure`, `max_fare`); scenarios
#' (`new_carspeeds`, `carspeed_scale`, `new_lts`); `output_dir`.
#'
#' @param r5r_network A network built with [build_network()].
#' @param origins,destinations A `data.frame` with columns `id`, `lon` and
#'   `lat`, or a `POINT sf` with an `id` column, in WGS 84.
#' @param mode A character vector, the transport modes: one of `"WALK"`,
#'   `"BICYCLE"` or `"CAR"`, or `"TRANSIT"` (alone or with `"WALK"`).
#' @param mode_egress A string, the mode from the last stop to the
#'   destination. Only `"WALK"` is supported with transit.
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
#' @param new_carspeeds,carspeed_scale,new_lts Scenarios; not supported yet
#'   (must be `NULL`, `1` and `NULL`).
#' @param draws_per_minute An integer, the Monte Carlo draws per departure
#'   minute for frequency-based 'GTFS'.
#' @param seed An integer seed for the Monte Carlo draws, or `NULL`. With
#'   frequency-based 'GTFS', results are reproducible (and equal to r5r's)
#'   only with a seed.
#' @param n_threads The number of threads; `Inf` (the default) uses all cores.
#' @param verbose,progress Logicals, checked as in r5r; peabiru prints no
#'   progress counter.
#' @param output_dir Not supported yet; must be `NULL`.
#' @param r5r_core Deprecated: the network under the name r5r used before
#'   version 2.3.0. As in r5r, it is accepted with a warning and used as
#'   `r5r_network`.
#'
#' @return A `data.frame` with `from_id`, `to_id` and one integer column
#'   `travel_time_pXX` per percentile (minutes). Pairs that cannot be
#'   completed within `max_trip_duration` are not returned; a higher
#'   percentile beyond the limit is `NA`.
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
  mode_list <- assign_mode(mode, mode_egress)
  departure <- assign_departure(departure_datetime, mode_list)

  assert_network(r5r_network)
  if (mode_list$transit) {
    check_transit_availability_on_date(r5r_network, departure)
  }

  swapped <- swap_od(origins, destinations, mode_list, r5r_network)
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
  check_scenarios(new_carspeeds, carspeed_scale, new_lts)

  pct_names <- sprintf("travel_time_p%02d", percentiles)
  if (mode_list$transit) {
    res <- rs_transit_ttm(
      r5r_network$ptr, origins$lat, origins$lon, destinations$lat, destinations$lon,
      departure$yyyymmdd, departure$seconds, as.integer(time_window),
      percentiles, max_walk_time, max_trip_duration, as.double(walk_speed),
      as.integer(max_rides), as.integer(draws_per_minute), seed$has, seed$value,
      threads
    )
    times <- res[-(1:2)]
  } else {
    res <- rs_street_ttm(
      r5r_network$ptr, origins$lat, origins$lon, destinations$lat, destinations$lon,
      mode_list$direct_modes, as.double(walk_speed), as.double(bike_speed),
      as.integer(max_lts), max_trip_duration, threads
    )
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
