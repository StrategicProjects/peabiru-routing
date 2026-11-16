#' Detailed itineraries between origin-destination pairs
#'
#' The itinerary of each trip between pairs of origins and destinations:
#' its legs (walking, cycling, driving and each ride), with their duration,
#' waiting time, distance, route and geometry: the direct trip by the street
#' mode and, with public transport, the fastest trip over the departure
#' window (`shortest_path = TRUE`) or, with `shortest_path = FALSE`, the
#' alternatives the multi-criteria router (McRAPTOR) finds over
#' `time_window` sampled departure times, one per sequence of routes (the
#' fastest of each), up to `suboptimal_minutes` slower than the best. A
#' pair's options are ordered by the trip by the street mode alone
#' first, then by the number of legs, then by duration, so the first option
#' is not the fastest when a trip with fewer legs (or the street trip) is
#' slower: on Porto Alegre with `suboptimal_minutes = 10`, a pair had 102
#' options, the first of 60.4 minutes and the fastest of 53.3. With a
#' `fare_structure` the trips are routed with fares (and only those within
#' `max_fare` kept), and the result gets `total_fare` and `cumulative_fare`
#' columns.
#'
#' Networks whose 'GTFS' has frequency-based trips cannot be used with
#' public transport (the itineraries of a Monte Carlo draw are not trips
#' that run).
#'
#' @section Egress by car and turn restrictions:
#' With `mode_egress = "CAR"`, the egress is searched backwards from the
#' destination, and a turn restriction with two or more via ways can be
#' read in either order of those ways. peabiru fixes the order: every pair
#' gets the itinerary of a freshly built network routing that pair alone,
#' whatever the other pairs, the threads or the earlier calls, and a
#' warning, once per call, names the pairs whose itineraries the other
#' order of those via ways would change (on Porto Alegre, about 1 pair in
#' 200).
#'
#' @inheritParams travel_time_matrix
#' @param suboptimal_minutes An integer, 0 (the default): with
#'   `shortest_path = FALSE`, how many minutes slower than the best an
#'   alternative may be. It must be 0 with `shortest_path = TRUE` and with a
#'   `fare_structure`.
#' @param shortest_path A logical. Whether to return only the fastest
#'   itinerary (`TRUE`, the default).
#' @param all_to_all A logical. Whether to route from every origin to
#'   every destination (`TRUE`) or pair them row by row (`FALSE`, the
#'   default; a single origin or destination is repeated to match the
#'   other).
#' @param drop_geometry A logical. Whether to leave the geometry out and
#'   return a table without it.
#' @param osm_link_ids A logical. Whether to add the columns
#'   `osm_id_list` (the OpenStreetMap way ids of a street leg's edges),
#'   `edge_id_list` (the indices of the street network's edges, as
#'   [street_network_to_sf()] numbers them), both as `"[id1, id2, ...]"` and
#'   `"[]"` for rides, and `board_stop_id`, `alight_stop_id` (a ride's stops,
#'   `"<feed>:<stop_id>"`; empty for street legs). Consecutive
#'   edges on a roundabout give only their first ids, and connectors none.
#'   Needs `drop_geometry = FALSE`. Defaults to `FALSE`.
#'
#' @return With `drop_geometry = FALSE`, an `sf` of `LINESTRING`s in
#'   EPSG:4326; otherwise a table without geometry. One row per leg (`segment`) of
#'   each itinerary (`option`), with `from_id`, `from_lat`, `from_lon`,
#'   `to_id`, `to_lat`, `to_lon`, `option`, `departure_time`,
#'   `total_duration` (minutes), `total_distance` (metres), `segment`,
#'   `mode`, `segment_duration`, `wait` (minutes), `distance` (metres),
#'   `route` and, with `osm_link_ids`, the four id columns; with a
#'   `fare_structure`, also `total_fare` (after `total_distance`) and
#'   `cumulative_fare` (after `mode`), the fare paid up to the end of the
#'   leg. Pairs without an itinerary have no rows. With `output_dir`,
#'   one CSV file per pair with itineraries is written,
#'   `from_<origin id>_to_<destination id>.csv` (the geometry as WKT text), and `output_dir` is returned.
#'
#'   `r doc_output_class(sf = TRUE)` With "data.table", both are
#'   `data.table`s.
#' @family routing functions
#' @export
#' @examplesIf requireNamespace("sf", quietly = TRUE)
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' points <- read.csv(file.path(data_path, "spo_points.csv"))
#'
#' trips <- detailed_itineraries(
#'   network, points[1:3, ], points[4:6, ],
#'   mode = "WALK",
#'   departure_datetime = as.POSIXct("2019-05-13 14:00:00",
#'                                   tz = "America/Sao_Paulo"),
#'   n_threads = 2
#' )
#' head(trips)
detailed_itineraries <- function(network,
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
                                 output_dir = NULL) {
  kind <- output_class()
  assert_network(network)
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
    check_transit_availability_on_date(network, departure)
    if (isTRUE(rs_network_info(network$ptr)$frequencies)) {
      stop("Assertion on 'network' failed: None of the GTFS feeds used to create ",
           "the transit network can contain a 'frequencies' table. Try using ",
           "gtfstools::frequencies_to_stop_times() to create a suitable feed.",
           call. = FALSE)
    }
  }

  max_walk_time <- assign_max_street_time(max_walk_time, walk_speed, max_trip_duration, "walk")
  max_bike_time <- assign_max_street_time(max_bike_time, bike_speed, max_trip_duration, "bike")
  max_car_time <- assign_max_street_time(max_car_time, NULL, max_trip_duration, "car")
  max_trip_duration <- assign_max_trip_duration(
    max_trip_duration, mode_list, max_walk_time, max_bike_time, max_car_time
  )
  assert_flag(shortest_path, "shortest_path")
  assert_flag(drop_geometry, "drop_geometry")
  assert_flag(osm_link_ids, "osm_link_ids")
  if (isTRUE(drop_geometry) && isTRUE(osm_link_ids)) {
    cli_abort_text(paste("To set osm_link_ids to `TRUE`, the parameter 'drop_geometry'",
                         "must also be `FALSE`"))
  }
  assert_count(time_window, "time_window", positive = TRUE)
  check_draws(1L, time_window,
              mcraptor = mode_list$transit && !(shortest_path && is.null(fare_structure)))
  check_speed(walk_speed, "walk")
  check_speed(bike_speed, "bike")
  assert_count(max_rides, "max_rides", positive = TRUE)
  check_max_lts(max_lts)
  threads <- assign_n_threads(n_threads)
  check_messages(verbose, progress)
  check_fares(fare_structure, max_fare, network)
  check_output_dir(output_dir)
  assert_count(suboptimal_minutes, "suboptimal_minutes")
  if (!is.null(fare_structure) && suboptimal_minutes > 0) {
    stop("Assertion on 'suboptimal_minutes' failed: Must be 0 when calculating ",
         "fares with detailed_itineraries().", call. = FALSE)
  }
  if (shortest_path && suboptimal_minutes > 0) {
    stop("Assertion on 'suboptimal_minutes' failed: Must be 0 when ",
         "'shortest_path' is TRUE.", call. = FALSE)
  }
  scenario <- scenario_spec(new_carspeeds, carspeed_scale, new_lts, network)
  if (is.null(output_dir) && !drop_geometry && !requireNamespace("sf", quietly = TRUE)) {
    stop("Package 'sf' is needed for the geometry of detailed itineraries; ",
         "install it or use 'drop_geometry = TRUE'.", call. = FALSE)
  }

  output <- if (is.null(output_dir)) NULL else {
    list(dir = path.expand(output_dir), from_ids = origins$id, to_ids = destinations$id)
  }
  res <- raise_if_stopped(rs_detailed_itineraries(
    network$ptr, origins$lat, origins$lon, destinations$lat, destinations$lon,
    mode_list$direct_modes, mode_list$transit_mode, mode_list$egress_mode,
    departure$yyyymmdd, departure$seconds, as.integer(time_window),
    max_walk_time, max_bike_time, max_car_time, max_trip_duration,
    as.double(walk_speed), as.double(bike_speed), as.integer(max_rides),
    as.integer(max_lts), shortest_path, threads, drop_geometry, scenario, osm_link_ids, output,
    as.integer(suboptimal_minutes), fare_structure, as.double(max_fare),
    # for tests: make the first router fail, so that the fallback runs
    isTRUE(getOption("peabiru.fail_fast_raptor"))
  ))
  for (w in res$warnings) warning(w, call. = FALSE)
  if (!is.null(output_dir)) return(output_dir)

  i <- res$pair
  # the fare columns are NULL without a fare structure, and left out
  out <- new_df(Filter(Negate(is.null), list(
    from_id = origins$id[i], from_lat = origins$lat[i], from_lon = origins$lon[i],
    to_id = destinations$id[i], to_lat = destinations$lat[i], to_lon = destinations$lon[i],
    option = res$option, departure_time = res$departure_time,
    total_duration = res$total_duration, total_distance = res$total_distance,
    total_fare = res$total_fare,
    segment = res$segment, mode = res$mode, cumulative_fare = res$cumulative_fare,
    segment_duration = res$segment_duration,
    wait = res$wait, distance = res$distance, route = res$route
  )))
  if (osm_link_ids) {
    out$osm_id_list <- res$osm_id_list
    out$edge_id_list <- res$edge_id_list
    out$board_stop_id <- res$board_stop_id
    out$alight_stop_id <- res$alight_stop_id
  }
  if (drop_geometry) return(as_output(out, kind))
  # WKT read by sf, then an sf in EPSG:4326
  out$geometry <- if (nrow(out) > 0) {
    sf::st_as_sfc(res$geometry)
  } else {
    sf::st_sfc(sf::st_linestring(), crs = 4326)[0]
  }
  as_output_sf(sf::st_sf(out, crs = 4326), kind, data_table = TRUE)
}

# The pairs to route: every origin with every destination, or row by row.
expand_od_pairs <- function(origins, destinations, all_to_all) {
  assert_flag(all_to_all, "all_to_all")
  n_origs <- nrow(origins)
  n_dests <- nrow(destinations)
  if (all_to_all || n_origs == 1 || n_dests == 1) {
    origins <- origins[rep(seq_len(n_origs), each = n_dests), ]
    destinations <- destinations[rep(seq_len(n_dests), times = n_origs), ]
    if (!all_to_all && (n_origs > 1 || n_dests > 1)) {
      if (n_origs == 1) {
        message("'origins' was expanded to match the number of destinations.")
      } else {
        message("'destinations' was expanded to match the number of origins.")
      }
    }
  } else if (n_origs != n_dests) {
    stop("When 'all_to_all' is FALSE, 'origins' and 'destinations' must either ",
         "have the same number of rows or one of them must have only one row.",
         call. = FALSE)
  }
  list(origins = origins, destinations = destinations)
}
