#' Calculate minute-by-minute travel times between origin destination pairs
#'
#' The travel time of every trip between each origin and destination, for
#' every departure minute in the time window (and every Monte Carlo draw on
#' routes given by frequencies), by public transport, reached and left by
#' any street mode (see [travel_time_matrix()] for the modes). Each row is
#' one trip: when it departs, which routes it rides and, with `breakdown`,
#' how its time splits into access, waiting, riding, transfers and egress.
#' A departure whose fastest trip uses no public transport has the street
#' mode as `routes` (`"[WALK]"`, `"[BICYCLE]"` or `"[CAR]"`).
#'
#' A street mode alone is an error: use [travel_time_matrix()], whose
#' travel times do not depend on the departure minute. At most 5,000
#' destinations are accepted (R5's limit).
#'
#' @section Compatibility with r5r:
#' The arguments, their defaults and checks, the columns, the rows and their
#' order are those of `r5r::expanded_travel_time_matrix()` in r5r 2.5.0,
#' with the same values (with frequencies, for the same `seed`); with `options(peabiru.output = "data.table")`
#' the result is identical to r5r's, its class included. With a street mode alone
#' r5r reports the same trip for every minute of a departure time taken
#' from the computer's clock (`Sys.time()`) rather than from
#' `departure_datetime`; peabiru stops with an error instead. With more
#' than one 'GTFS' feed, a few trips can differ between operating systems
#' (the feeds are loaded in the order the file system lists them), as
#' r5r's do.
#'
#' @inheritParams travel_time_matrix
#' @param breakdown A logical. Whether to break each trip down into access,
#'   waiting, in-vehicle, transfer and egress times, and the number of rides.
#'
#' @return A table with `from_id`, `to_id`, `departure_time`
#'   (`"HH:MM:SS"`), `draw_number`, with `breakdown = TRUE` `access_time`,
#'   `wait_time`, `ride_time`, `transfer_time`, `egress_time`, then `routes`
#'   (route ids joined by `"|"`, or `"[WALK]"`), with `breakdown = TRUE`
#'   `n_rides`, and `total_time`. Times are in minutes, with one decimal.
#'   Trips longer than `max_trip_duration` have `NA` times and routes. With
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
#' ettm <- expanded_travel_time_matrix(
#'   network, points[1:2, ], points,
#'   mode = c("WALK", "TRANSIT"),
#'   departure_datetime = as.POSIXct("2019-05-13 14:00:00",
#'                                   tz = "America/Sao_Paulo"),
#'   time_window = 2,
#'   breakdown = TRUE,
#'   seed = 42,
#'   n_threads = 2
#' )
#' head(ettm)
expanded_travel_time_matrix <- function(r5r_network,
                                        origins,
                                        destinations,
                                        mode = "WALK",
                                        mode_egress = "WALK",
                                        departure_datetime = NULL,
                                        time_window = 10L,
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
                                        output_dir = NULL,
                                        r5r_core = deprecated()) {
  # mirrors r5r R/expanded_travel_time_matrix.R
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
  check_expanded_mode(mode_list)
  departure <- assign_departure(departure_datetime, mode_list)

  assert_network(r5r_network)
  check_transit_availability_on_date(r5r_network, departure)

  max_walk_time <- assign_max_street_time(max_walk_time, walk_speed, max_trip_duration, "walk")
  max_bike_time <- assign_max_street_time(max_bike_time, bike_speed, max_trip_duration, "bike")
  max_car_time <- assign_max_street_time(max_car_time, NULL, max_trip_duration, "car")
  max_trip_duration <- assign_max_trip_duration(
    max_trip_duration, mode_list, max_walk_time, max_bike_time, max_car_time
  )

  assert_count(time_window, "time_window", positive = TRUE)
  check_draws(draws_per_minute)
  seed <- seed_args(seed)
  check_speed(walk_speed, "walk")
  check_speed(bike_speed, "bike")
  assert_count(max_rides, "max_rides", positive = TRUE)
  check_max_lts(max_lts)
  threads <- assign_n_threads(n_threads)
  check_messages(verbose, progress)
  check_output_dir(output_dir)
  assert_flag(breakdown, "breakdown")
  scenario <- scenario_spec(new_carspeeds, carspeed_scale, new_lts, r5r_network)

  res <- raise_if_stopped(rs_expanded(
    r5r_network$ptr, origins$lat, origins$lon, destinations$lat, destinations$lon,
    mode_list$transit_mode, mode_list$access_mode, mode_list$egress_mode,
    as.double(bike_speed),
    as.integer(max_lts), max_bike_time, max_car_time,
    FALSE, departure$yyyymmdd, departure$seconds, as.integer(time_window),
    max_walk_time, max_trip_duration, as.double(walk_speed),
    as.integer(max_rides), as.integer(draws_per_minute), seed$has, seed$value,
    threads, scenario,
    csv_output(output_dir, origins, destinations, breakdown = breakdown)
  ))
  if (!is.null(output_dir)) return(output_dir)
  as_output(expanded_result(res, origins, destinations, breakdown, max_trip_duration), kind)
}

#' Calculate travel time matrix between origin destination pairs considering a
#' time of arrival
#'
#' For each origin and destination, the trip by public transport that
#' leaves latest and still arrives by `arrival_datetime` (see
#' [travel_time_matrix()] for the modes). Departures are searched minute by
#' minute from `arrival_datetime - max_trip_duration`, which may fall on
#' the day before, and each pair keeps the latest departure that arrives in
#' time, with its routes and, with `breakdown`, its components.
#'
#' As in [expanded_travel_time_matrix()], a street mode alone is an error
#' and at most 5,000 destinations are accepted.
#'
#' @section Compatibility with r5r:
#' The arguments, their defaults and checks, the columns, the rows and their
#' order are those of `r5r::arrival_travel_time_matrix()` in r5r 2.5.0,
#' with the same values (with frequencies, for the same `seed`); with `options(peabiru.output = "data.table")`
#' the result is identical to r5r's, its class included. An `arrival_datetime` that
#' is not a date-time is an error that says so (r5r fails on it with an
#' internal R error); a street mode alone is an error, as in
#' [expanded_travel_time_matrix()].
#'
#' @inheritParams expanded_travel_time_matrix
#' @param arrival_datetime A POSIXct (or a POSIXlt, which is converted), the
#'   time by which trips must arrive (see the section "Date and time" of
#'   [travel_time_matrix()]).
#'
#' @return A table with the columns of
#'   [expanded_travel_time_matrix()]: one row per origin and destination
#'   reached in time. With `output_dir`, `output_dir`.
#'
#'   `r doc_output_class()`
#' @family routing functions
#' @export
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' points <- read.csv(file.path(data_path, "spo_points.csv"))
#'
#' attm <- arrival_travel_time_matrix(
#'   network, points[1:2, ], points,
#'   mode = c("WALK", "TRANSIT"),
#'   arrival_datetime = as.POSIXct("2019-05-13 15:00:00",
#'                                 tz = "America/Sao_Paulo"),
#'   max_trip_duration = 30,
#'   seed = 42,
#'   n_threads = 2
#' )
#' head(attm)
arrival_travel_time_matrix <- function(r5r_network,
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
                                       output_dir = NULL,
                                       r5r_core = deprecated()) {
  # mirrors r5r R/arrival_travel_time_matrix.R
  kind <- output_class()
  if (is_present(r5r_core)) {
    warn_r5r_core()
    r5r_network <- r5r_core
  }
  if (missing(arrival_datetime) || is.null(arrival_datetime)) {
    cli_abort_text(paste(
      "`arrival_datetime` is required. Supply a POSIXct value specifying when",
      "trips must arrive. For trips that do not depend on time, use",
      "`travel_time_matrix()`."
    ))
  }
  assert_number(n_threads, "n_threads", lower = 1)
  assert_network(r5r_network)

  origins <- assign_points_input(origins, "origins")
  destinations <- assign_points_input(destinations, "destinations")
  # r5r's Java side fails on a missing destination id (NullPointerException)
  if (anyNA(destinations$id)) {
    stop("'destinations$id' cannot be missing (r5r 2.5.0 fails on a missing ",
         "destination id in Java).", call. = FALSE)
  }
  mode_list <- assign_mode(mode, mode_egress)
  check_expanded_mode(mode_list)

  # set before the departure datetime, which depends on the final
  # max_trip_duration
  max_walk_time <- assign_max_street_time(max_walk_time, walk_speed, max_trip_duration, "walk")
  max_bike_time <- assign_max_street_time(max_bike_time, bike_speed, max_trip_duration, "bike")
  max_car_time <- assign_max_street_time(max_car_time, NULL, max_trip_duration, "car")
  max_trip_duration <- assign_max_trip_duration(
    max_trip_duration, mode_list, max_walk_time, max_bike_time, max_car_time
  )

  # calculate departure datetime, exactly as r5r does: the subtraction turns
  # a POSIXlt into a POSIXct, then the date and clock time are taken in its
  # time zone, and r5r checks the result (a number gives a difftime, which
  # its message names, under 'departure_datetime'). A value the subtraction
  # fails on, where r5r stops with an internal R error, is an error of
  # peabiru's own.
  if (!(is.numeric(arrival_datetime) || is.logical(arrival_datetime) ||
        inherits(arrival_datetime, c("POSIXt", "Date")))) {
    cm_assert(paste(cm_type_msg(arrival_datetime, "POSIXct"),
                    "(r5r 2.5.0 fails on it with an internal R error)"), "arrival_datetime")
  }
  departure_datetime <- arrival_datetime - as.difftime(max_trip_duration, units = "mins")
  departure <- assign_departure(departure_datetime, mode_list)
  check_transit_availability_on_date(r5r_network, departure)

  # the time window is max_trip_duration
  check_draws(draws_per_minute)
  seed <- seed_args(seed)
  check_speed(walk_speed, "walk")
  check_speed(bike_speed, "bike")
  assert_count(max_rides, "max_rides", positive = TRUE)
  check_max_lts(max_lts)
  threads <- assign_n_threads(n_threads)
  check_messages(verbose, progress)
  check_output_dir(output_dir)
  assert_flag(breakdown, "breakdown")
  scenario <- scenario_spec(new_carspeeds, carspeed_scale, new_lts, r5r_network)

  res <- raise_if_stopped(rs_expanded(
    r5r_network$ptr, origins$lat, origins$lon, destinations$lat, destinations$lon,
    mode_list$transit_mode, mode_list$access_mode, mode_list$egress_mode,
    as.double(bike_speed),
    as.integer(max_lts), max_bike_time, max_car_time,
    TRUE, departure$yyyymmdd, departure$seconds, max_trip_duration,
    max_walk_time, max_trip_duration, as.double(walk_speed),
    as.integer(max_rides), as.integer(draws_per_minute), seed$has, seed$value,
    threads, scenario,
    csv_output(output_dir, origins, destinations, breakdown = breakdown)
  ))
  if (!is.null(output_dir)) return(output_dir)
  as_output(expanded_result(res, origins, destinations, breakdown, max_trip_duration), kind)
}

check_expanded_mode <- function(mode_list) {
  if (!mode_list$transit) {
    unsupported(paste0(
      "An expanded or arrival travel time matrix by ", mode_list$direct_modes,
      " alone (use travel_time_matrix() for street modes)"
    ))
  }
  invisible(TRUE)
}

# Column assembly of r5r's expanded and arrival matrices.
expanded_result <- function(res, origins, destinations, breakdown, max_trip_duration) {
  cols <- list(
    from_id = origins$id[res$from],
    to_id = destinations$id[res$to],
    departure_time = seconds_to_hhmmss(res$departure),
    draw_number = res$draw
  )
  if (breakdown) {
    cols <- c(cols, res[c("access_time", "wait_time", "ride_time",
                          "transfer_time", "egress_time", "routes",
                          "n_rides", "total_time")])
  } else {
    cols <- c(cols, res[c("routes", "total_time")])
  }
  travel_times <- new_df(cols)

  # r5r: replace travel times of non-viable trips with NAs
  over <- which(travel_times$total_time > max_trip_duration)
  if (length(over) > 0) {
    na_cols <- if (breakdown) {
      c("access_time", "wait_time", "ride_time", "transfer_time",
        "egress_time", "routes", "n_rides", "total_time")
    } else {
      c("routes", "total_time")
    }
    for (j in na_cols) travel_times[[j]][over] <- NA
  }
  travel_times
}
