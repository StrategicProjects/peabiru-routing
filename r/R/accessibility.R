#' Calculate access to opportunities
#'
#' Cumulative or decay-weighted access to the opportunities at the
#' destinations from each origin, by one street mode (`"WALK"`, `"BICYCLE"`
#' or `"CAR"`) or by public transport with walk access and egress
#' (`c("WALK", "TRANSIT")`). Arguments, defaults and the result are those of
#' `r5r::accessibility()` (r5r 2.5.0); the values are bit-identical.
#'
#' As in r5r, with the `"step"` function trips longer than the largest
#' cutoff are not considered (`max_trip_duration` becomes `max(cutoffs)`),
#' while the other functions still weight them; and `max_trip_duration` may
#' not be shorter than the largest cutoff.
#'
#' Not supported yet, and an error when requested: transit with `"BICYCLE"`
#' or `"CAR"` access or a non-walk `mode_egress`; a subset of transit modes;
#' fares; scenarios; `output_dir`.
#'
#' @inheritParams travel_time_matrix
#' @param origins A `data.frame` with columns `id`, `lon` and `lat`, or a
#'   `POINT sf` with an `id` column, in WGS 84.
#' @param destinations A `data.frame` or `POINT sf` with `id`, `lon`, `lat`
#'   and the opportunity columns.
#' @param opportunities_colnames A character vector, the names of the
#'   numeric columns of `destinations` with the opportunity counts. Missing
#'   values count as 0, with a warning.
#' @param decay_function A string: `"step"`, `"exponential"`,
#'   `"fixed_exponential"`, `"linear"` or `"logistic"`.
#' @param cutoffs An integer vector of up to 12 distinct values from 1 to 120
#'   (minutes). Must be `NULL` with `"fixed_exponential"` and given otherwise.
#' @param decay_value A number, the parameter of the decay function: the
#'   decay constant for `"fixed_exponential"` (between 0 and 1), the width in
#'   minutes for `"linear"` (a whole number from 1 to 59), the standard
#'   deviation in minutes for `"logistic"` (at least 1); `NULL` for `"step"`
#'   and `"exponential"`.
#'
#' @return A `data.frame` with `id`, `opportunity`, `percentile`, `cutoff`
#'   (absent with `"fixed_exponential"`) and `accessibility`, one row per
#'   origin, opportunity, percentile and cutoff, in that order.
#' @export
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' points <- read.csv(file.path(data_path, "spo_points.csv"))
#'
#' access <- accessibility(
#'   network, points, points,
#'   opportunities_colnames = c("jobs", "schools"),
#'   mode = c("WALK", "TRANSIT"),
#'   departure_datetime = as.POSIXct("2019-05-13 14:00:00",
#'                                   tz = "America/Sao_Paulo"),
#'   decay_function = "logistic",
#'   cutoffs = c(15, 30),
#'   decay_value = 5,
#'   seed = 42,
#'   n_threads = 2
#' )
#' head(access)
accessibility <- function(r5r_network,
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
                          output_dir = NULL,
                          r5r_core) {
  # mirrors r5r R/accessibility.R
  if (!missing(r5r_core)) {
    warn_r5r_core()
    r5r_network <- r5r_core
  }
  assert_network(r5r_network)
  origins <- assign_points_input(origins, "origins")
  destinations <- assign_points_input(destinations, "destinations")
  opportunities <- assign_opportunities(destinations, opportunities_colnames)
  mode_list <- assign_mode(mode, mode_egress)
  departure <- assign_departure(departure_datetime, mode_list)
  if (mode_list$transit) {
    check_transit_availability_on_date(r5r_network, departure)
  }

  decay_list <- assign_decay_function(decay_function, decay_value)

  # cap trip duration with cutoffs
  cutoffs_r5 <- assign_cutoffs(cutoffs, decay_function)
  assert_number(max_trip_duration, "max_trip_duration", lower = 1, finite = TRUE)
  if (!is.null(cutoffs)) {
    if (max_trip_duration < max(cutoffs)) {
      stop("'max_trip_duration' cannot be shorter than 'max(cutoffs)'", call. = FALSE)
    }
    # only the step function ignores trips longer than the cutoffs; the
    # other decay functions still weight them
    if (decay_function == "step") max_trip_duration <- max(cutoffs)
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

  values <- rs_accessibility(
    r5r_network$ptr, origins$lat, origins$lon, destinations$lat, destinations$lon,
    opportunities,
    if (mode_list$transit) "TRANSIT" else mode_list$direct_modes,
    decay_list$fun, decay_list$value, cutoffs_r5, percentiles,
    departure$yyyymmdd, departure$seconds, as.integer(time_window),
    max_walk_time, max_trip_duration, as.double(walk_speed), as.double(bike_speed),
    as.integer(max_lts), as.integer(max_rides), as.integer(draws_per_minute),
    seed$has, seed$value, threads
  )

  # one row per origin, opportunity, percentile and cutoff, in that order
  n_o <- nrow(origins)
  n_k <- length(opportunities_colnames)
  n_p <- length(percentiles)
  n_c <- length(cutoffs_r5)
  cols <- list(
    id = rep(origins$id, each = n_k * n_p * n_c),
    opportunity = rep(rep(opportunities_colnames, each = n_p * n_c), times = n_o),
    percentile = rep(rep(percentiles, each = n_c), times = n_o * n_k),
    cutoff = rep(cutoffs_r5, times = n_o * n_k * n_p),
    accessibility = values
  )
  if (decay_function == "fixed_exponential") cols$cutoff <- NULL
  new_df(cols)
}
