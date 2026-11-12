#' Trip options between origin-destination pairs
#'
#' For each pair of origin and destination, every trip worth considering:
#' the trips that no other trip beats on all the chosen criteria at once.
#' This is peabiru's own multi-criteria trip planner; r5r has no
#' counterpart (see "Not dominated" and "Compared with r5r's functions"
#' below).
#'
#' @section Not dominated:
#' A trip *dominates* another when it is no worse on every chosen
#' criterion and better on at least one. The criteria are
#' * `"TIME"`: door-to-door minutes from the departure minute, waits
#'   included;
#' * `"FARE"`: the fare by `fare_structure` (needs one);
#' * `"TRANSFERS"`: rides minus one; a trip without public transport has 0,
#'   as many as a trip with a single ride;
#' * `"WALK"`: minutes on foot, that is the access and egress legs when they
#'   are walked and the walks between stops. A bicycle or car leg counts 0,
#'   also where the cyclist pushes the bicycle.
#'
#' At each departure minute of the window (`departure_datetime`, plus one
#' minute at a time for `time_window` minutes) the options are the exact
#' set of trips not dominated, one trip per combination of values: of
#' trips with the same values on the chosen criteria, the one with the
#' least total time, then transfers, minutes on foot and fare is kept.
#' Trips are those of r5r's network semantics, so that their times agree
#' with [detailed_itineraries()]: a direct trip by the street mode, and
#' trips that reach a stop of the access search, ride and walk between
#' stops (as R5's transfer tables allow), and leave a stop of the egress
#' search; each ride boards at least a minute after its stop is reached,
#' on the vehicle that gets to the alighting stop first.
#'
#' The search is exact; what is returned is then compared at the
#' resolution the values are read at: total time to a tenth of a minute (as
#' it is reported), and minutes on foot to the whole minute (rounded,
#' `walk_resolution` seconds). Of the trips of a minute's exact set, one is
#' left out when another is no worse on every criterion in those terms and
#' better on one, or equal on all and better in exact terms (least total
#' time, then transfers, walking, fare); two trips a few seconds of walking
#' apart are not both returned. The values returned are exact. Rounding
#' cannot make a trip look better than one that beats it, so this is the
#' same as comparing every trip at that resolution.
#'
#' The minutes are then merged into options: trips with the same street
#' modes, the same routes between the same stops (whichever vehicles), and
#' the same fare, transfers and minutes on foot are one option, available
#' at `n_departures` minutes from `first_departure` to `last_departure`.
#' Its total time changes with the minute (the wait at the start does); the
#' option reports the trip of its best minute, the one with the least total
#' time (the earliest of equal ones). With `by_departure = TRUE` the result
#' also has every minute's options.
#'
#' Frequency-based routes (GTFS `frequencies.txt`) are boarded at their
#' expected times: half a headway after the boarding wait (or after the
#' start of the period), within the period; with `exact_times = 1` at the
#' first scheduled departure. There are no random draws and no `seed`, and
#' the results differ on purpose from r5r's Monte Carlo draws.
#'
#' @section Fares:
#' Fares follow `fare_structure` exactly as [pareto_frontier()] and
#' [detailed_itineraries()] compute them (r5r's calculator, in cents). A
#' fare is not the sum of its rides (discounted transfers within a time
#' allowance, unlimited transfers, a cap), so two partial trips are only
#' compared on fare when one is at least as cheap for every way both can
#' continue; the comparison makes no assumption about the structure. This
#' is what makes fare criteria slower than the others: trips waiting for a
#' discount are kept apart until they cannot get one any more.
#'
#' @section Compared with r5r's functions:
#' [pareto_frontier()] gives, for each fare cutoff, the percentiles of the
#' travel time over Monte Carlo draws of departure times (r5r's function);
#' [detailed_itineraries()] gives the fastest trip, or with
#' `shortest_path = FALSE` R5's alternatives, one per sequence of routes.
#' `trip_options()` gives the trade-offs themselves: the trips that are
#' faster, cheaper, with fewer transfers or less walking than the others,
#' each with its legs. With `criteria = "TIME"` and `time_window = 1` its
#' first option has the total time of [detailed_itineraries()]'s fastest
#' trip.
#'
#' @inheritParams detailed_itineraries
#' @param mode A character vector, the modes as in [detailed_itineraries()];
#'   defaults to `c("WALK", "TRANSIT")`. Without a public transport mode the
#'   only option is the direct street trip.
#' @param criteria A character vector, some of `"TIME"`, `"FARE"`,
#'   `"TRANSFERS"` and `"WALK"`, or `NULL` (the default): all four with a
#'   `fare_structure`, all but `"FARE"` without one.
#' @param max_options A number: at most this many options per pair, the
#'   first in the order of `option`; `Inf` (the default) for all of them.
#' @param walk_resolution A number of seconds, 60 (the default): the step
#'   to which minutes on foot are compared. By default walking is compared to
#'   the whole minute (rounded), so that of two trips a few seconds of walking
#'   apart and otherwise no better only one is returned; `1` compares the
#'   exact seconds. See "Not dominated".
#' @param itineraries A logical. Whether to build the legs of each option
#'   (`TRUE`, the default).
#' @param by_departure A logical. Whether to add the options of every
#'   departure minute (`FALSE`, the default).
#' @param drop_geometry A logical. Whether to leave the geometry of the legs
#'   out (and return them as a table without geometry).
#' @param output_dir `NULL` or a directory: write one CSV file per pair with
#'   options, `from_<origin id>_to_<destination id>.csv`, one row per leg (or
#'   per option, with `itineraries = FALSE`) with the option's and the leg's
#'   columns and the geometry as WKT, and return `output_dir`.
#'
#' @return A list with
#'   * `options`, a table: one row per option, with `from_id`,
#'     `to_id`, `option` (from 1, ordered by total time, then fare,
#'     transfers and minutes on foot), `total_time` (minutes, one decimal
#'     place), `fare` (with a `fare_structure`), `transfers`, `walk_time`
#'     (minutes), `departure_time` and `arrival_time` of the option's best
#'     minute, `n_departures`, `first_departure`, `last_departure` and
#'     `description` (the modes and route short names, as
#'     `"WALK > BUS 343 > WALK"`);
#'   * `itineraries` (with `itineraries = TRUE`): the legs of each option at
#'     its best minute, in the format of [detailed_itineraries()] and keyed
#'     by `from_id`, `to_id` and `option`: `segment`, `mode`,
#'     `cumulative_fare` (with a `fare_structure`), `segment_duration`,
#'     `wait`, `distance`, `route`, the four id columns with `osm_link_ids`,
#'     and the geometry: an `sf` of `LINESTRING`s in EPSG:4326, or a
#'     table without geometry with `drop_geometry = TRUE`;
#'   * `departures` (with `by_departure = TRUE`), a table: for every
#'     departure minute, the options on its frontier (`from_id`, `to_id`,
#'     `option`, `departure_time`, `arrival_time`, `total_time`).
#'
#'   `r doc_output_class(sf = TRUE)` With "data.table", the itineraries are
#'   an `sf` `data.table`, as those of [detailed_itineraries()].
#' @export
#' @examplesIf requireNamespace("sf", quietly = TRUE)
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' points <- read.csv(file.path(data_path, "spo_points.csv"))
#'
#' trips <- trip_options(
#'   network, points[10, ], points[12, ],
#'   departure_datetime = as.POSIXct("2019-05-13 14:00:00",
#'                                   tz = "America/Sao_Paulo"),
#'   n_threads = 2
#' )
#' trips$options
trip_options <- function(r5r_network,
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
                         output_dir = NULL) {
  kind <- output_class()
  assert_network(r5r_network)
  origins <- assign_points_input(origins, "origins", unique_ids = all_to_all)
  destinations <- assign_points_input(destinations, "destinations", unique_ids = all_to_all)
  od <- expand_od_pairs(origins, destinations, all_to_all)
  origins <- od$origins
  destinations <- od$destinations

  if (!is.null(output_dir)) {
    if (anyDuplicated(data.frame(origins$id, destinations$id)) > 0) {
      stop("With 'output_dir', origin-destination pairs must be unique, because ",
           "each pair is written to a file named 'from_<id>_to_<id>.csv'.",
           call. = FALSE)
    }
    bad_ids <- unique(grep("[/\\\\:*?\"<>|]", c(origins$id, destinations$id), value = TRUE))
    if (length(bad_ids) > 0) {
      stop("With 'output_dir', ids cannot contain characters that are not ",
           "allowed in file names (/ \\ : * ? \" < > |). Invalid id(s): ",
           paste0("\"", bad_ids, "\"", collapse = ", "), ".", call. = FALSE)
    }
  }

  mode_list <- assign_mode(mode, mode_egress)
  departure <- assign_departure(departure_datetime, mode_list)
  if (mode_list$transit) {
    check_transit_availability_on_date(r5r_network, departure)
  }
  criteria <- assign_criteria(criteria, fare_structure)

  max_walk_time <- assign_max_street_time(max_walk_time, walk_speed, max_trip_duration, "walk")
  max_bike_time <- assign_max_street_time(max_bike_time, bike_speed, max_trip_duration, "bike")
  max_car_time <- assign_max_street_time(max_car_time, NULL, max_trip_duration, "car")
  max_trip_duration <- assign_max_trip_duration(
    max_trip_duration, mode_list, max_walk_time, max_bike_time, max_car_time
  )
  assert_count(time_window, "time_window", positive = TRUE)
  check_speed(walk_speed, "walk")
  check_speed(bike_speed, "bike")
  assert_count(max_rides, "max_rides", positive = TRUE)
  check_max_lts(max_lts)
  assert_number(max_options, "max_options", lower = 1)
  if (is.finite(max_options)) assert_count(max_options, "max_options", positive = TRUE)
  assert_count(walk_resolution, "walk_resolution", positive = TRUE)
  assert_flag(itineraries, "itineraries")
  assert_flag(by_departure, "by_departure")
  assert_flag(drop_geometry, "drop_geometry")
  assert_flag(osm_link_ids, "osm_link_ids")
  if (isTRUE(drop_geometry) && isTRUE(osm_link_ids)) {
    cli_abort_text(paste("To set osm_link_ids to `TRUE`, the parameter 'drop_geometry'",
                         "must also be `FALSE`"))
  }
  threads <- assign_n_threads(n_threads)
  check_messages(verbose, progress)
  check_fares(fare_structure, max_fare, r5r_network)
  check_output_dir(output_dir)
  scenario <- scenario_spec(new_carspeeds, carspeed_scale, new_lts, r5r_network)
  needs_sf <- itineraries && !drop_geometry && is.null(output_dir)
  if (needs_sf && !requireNamespace("sf", quietly = TRUE)) {
    stop("Package 'sf' is needed for the geometry of the itineraries; ",
         "install it or use 'drop_geometry = TRUE' or 'itineraries = FALSE'.",
         call. = FALSE)
  }

  output <- if (is.null(output_dir)) NULL else {
    list(dir = path.expand(output_dir), from_ids = origins$id, to_ids = destinations$id)
  }
  res <- raise_if_stopped(rs_trip_options(
    r5r_network$ptr, origins$lat, origins$lon, destinations$lat, destinations$lon,
    mode_list$direct_modes, mode_list$transit_mode, mode_list$egress_mode,
    departure$yyyymmdd, departure$seconds, as.integer(time_window), criteria,
    max_walk_time, max_bike_time, max_car_time, max_trip_duration,
    as.double(walk_speed), as.double(bike_speed), as.integer(max_rides),
    as.integer(max_lts), threads, scenario, fare_structure, as.double(max_fare),
    if (is.finite(max_options)) as.integer(max_options) else 0L,
    itineraries, drop_geometry, osm_link_ids, output, as.integer(walk_resolution)
  ))
  if (!is.null(output_dir)) return(output_dir)

  ids <- function(tab) {
    i <- tab$pair
    tab$pair <- NULL
    new_df(c(list(from_id = origins$id[i], to_id = destinations$id[i]),
             Filter(Negate(is.null), tab)))
  }
  out <- list(options = ids(res$options))
  if (itineraries) {
    legs <- res$legs
    geometry <- legs$geometry
    legs$geometry <- NULL
    legs <- ids(legs)
    if (!drop_geometry) {
      # as detailed_itineraries() builds its sf: WKT read by sf, EPSG:4326
      legs$geometry <- if (nrow(legs) > 0) {
        sf::st_as_sfc(geometry)
      } else {
        sf::st_sfc(sf::st_linestring(), crs = 4326)[0]
      }
      legs <- sf::st_sf(legs, crs = 4326)
    }
    out$itineraries <- legs
  }
  if (by_departure) out$departures <- ids(res$departures)
  as_output_list(out, kind, c("options", "itineraries", "departures"),
                 sf_data_table = "itineraries")
}

# The criteria of trip_options(): NULL for the default, else a subset of the
# four names (upper case, unique); "FARE" needs a fare structure.
assign_criteria <- function(criteria, fare_structure) {
  choices <- c("TIME", "FARE", "TRANSFERS", "WALK")
  if (is.null(criteria)) {
    return(if (is.null(fare_structure)) setdiff(choices, "FARE") else choices)
  }
  cm_assert(check_character(criteria, min.len = 1, any.missing = FALSE, unique = TRUE),
            "criteria")
  criteria <- toupper(criteria)
  cm_assert(check_subset(criteria, choices, empty.ok = FALSE), "criteria")
  if ("FARE" %in% criteria && is.null(fare_structure)) {
    cli_abort_text("The \"FARE\" criterion needs a `fare_structure`.", c(
      i = "Create one with `setup_fare_structure()` or `read_fare_structure()`."
    ))
  }
  criteria
}
