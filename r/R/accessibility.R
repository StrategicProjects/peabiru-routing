#' Calculate access to opportunities
#'
#' Cumulative or decay-weighted access to the opportunities at the
#' destinations from each origin, by one street mode (`"WALK"`, `"BICYCLE"`
#' or `"CAR"`) or by public transport (see [travel_time_matrix()] for the
#' modes). For each origin, the opportunities of every destination are
#' summed, each weighted by the decay function of the travel time to it,
#' for every percentile of the travel time over the departure window and
#' every cutoff.
#'
#' @section Decay functions:
#' With `t` the travel time and `c` a cutoff:
#' - `"step"`: weight 1 below the cutoff, 0 from it on (cumulative
#'   opportunities). Trips longer than the largest cutoff are not routed
#'   (`max_trip_duration` becomes `max(cutoffs)`).
#' - `"exponential"`: `2^(-t / c)`, half the weight at the cutoff; 0 from
#'   two hours on.
#' - `"fixed_exponential"`: `exp(-decay_value * t)` with `t` in seconds,
#'   without cutoffs; 0 from two hours on.
#' - `"linear"`: 1 up to `decay_value / 2` minutes before the cutoff, 0 from
#'   `decay_value / 2` minutes after it, falling linearly in between.
#' - `"logistic"`: a logistic curve with its median at the cutoff and
#'   standard deviation `decay_value` minutes, scaled to 1 at `t = 0`.
#'
#' The weights are those of R5, computed with the exponential of Apache
#' Commons Math (`FastMath.exp()`) that R5 calls, so the sums are R5's to the
#' last bit. Except with `"step"`, `max_trip_duration` may not be shorter
#' than the largest cutoff.
#'
#' With a `fare_structure`, public transport is routed with fares as in
#' [travel_time_matrix()] (see its section "Departures and Monte Carlo
#' draws").
#'
#' @section Compatibility with r5r:
#' The arguments, their defaults and checks, the columns and the rows of the
#' result are those of `r5r::accessibility()` in r5r 2.5.0, and the values
#' are equal to the last bit (with frequencies, for the same `seed`); with `options(peabiru.output = "data.table")`
#' the result is identical to r5r's, its class included. An empty
#' `percentiles` is an error that says so (r5r fails on it in Java).
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
#' @return A table with `id`, `opportunity`, `percentile`, `cutoff`
#'   (absent with `"fixed_exponential"`) and `accessibility`, one row per
#'   origin, opportunity, percentile and cutoff, in that order. With
#'   `output_dir`, `output_dir`.
#'
#'   `r doc_output_class()`
#' @family routing functions
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
                          r5r_core = deprecated()) {
  # mirrors r5r R/accessibility.R
  kind <- output_class()
  if (is_present(r5r_core)) {
    warn_r5r_core()
    r5r_network <- r5r_core
  }
  assert_network(r5r_network)
  origins <- assign_points_input(origins, "origins")
  destinations <- assign_points_input(destinations, "destinations")
  # r5r's Java side fails on a missing destination id (NullPointerException)
  if (anyNA(destinations$id)) {
    stop("'destinations$id' cannot be missing (r5r 2.5.0 fails on a missing ",
         "destination id in Java).", call. = FALSE)
  }
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

  values <- raise_if_stopped(rs_accessibility(
    r5r_network$ptr, origins$lat, origins$lon, destinations$lat, destinations$lon,
    opportunities,
    mode_list$direct_modes, mode_list$transit_mode, mode_list$egress_mode,
    decay_list$fun, decay_list$value, cutoffs_r5, percentiles,
    departure$yyyymmdd, departure$seconds, as.integer(time_window),
    max_walk_time, max_bike_time, max_car_time, max_trip_duration,
    as.double(walk_speed), as.double(bike_speed),
    as.integer(max_lts), as.integer(max_rides), as.integer(draws_per_minute),
    seed$has, seed$value, threads, scenario,
    csv_output(output_dir, origins, destinations,
               opportunities = opportunities_colnames),
    fare_structure, as.double(max_fare)
  ))
  if (!is.null(output_dir)) return(output_dir)

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
  as_output(new_df(cols), kind)
}
