#' Estimate isochrones from a given location
#'
#' The areas (`polygon_output = TRUE`) or the street segments
#' (`polygon_output = FALSE`) reached from each origin within each cutoff,
#' by one street mode (`"WALK"`, `"BICYCLE"` or `"CAR"`) or by public
#' transport (see [travel_time_matrix()] for the modes). Package 'sf' is
#' required.
#'
#' Polygon isochrones are traced from the travel times from each origin to
#' the centres of a regular grid of Web Mercator pixels covering the network
#' (see `zoom`): the bands from 0 to each cutoff are traced over that grid
#' by marching squares (the algorithm of `isoband::isobands()`), grouped
#' into polygons with holes (as `isoband::iso_to_sfg()` groups them) and
#' passed to `sf::st_make_valid()`. The order of the rings and the vertex
#' each one starts at follow the C++ library 'isoband' is built with on
#' each platform (libc++ on macOS, libstdc++ on Linux and Windows); between
#' platforms the polygons differ in that order and, by the system's
#' \code{sinh()} and \code{atan()}, in the last bits of some latitudes.
#'
#' Line isochrones are the travel times from each origin to the centroid of
#' every street segment (`sf::st_centroid()` of the edges of
#' [street_network_to_sf()]), each segment in the band of the smallest
#' cutoff at or above its travel time.
#'
#' `max_trip_duration` is not used: the trip limit is the largest cutoff
#' (plus 10 minutes for polygons, so that the outer band is interpolated
#' like the inner ones).
#'
#' @section Compatibility with r5r:
#' The arguments, their defaults and checks and the result are those of
#' `r5r::isochrone()` in r5r 2.5.0. The travel times are r5r's, and so are
#' the polygons before `sf::st_make_valid()`, ring for ring and vertex for
#' vertex on the same platform, so the `sf` returned is r5r's, coordinate
#' for coordinate (compared with `identical()`). The centroids of the line
#' isochrones agree with r5r's to within a few units in the last place
#' (about 1e-14 degrees), which has not changed a travel time in any
#' comparison made. The deprecated `sample_size` is accepted with r5r's
#' warning and ignored. An empty `percentiles`, a `polygon_output` of `NA`,
#' `origins` without rows and a fractional `zoom` are errors that say so
#' (r5r lets them through and then fails, or passes the fraction to R5).
#'
#' @inheritParams travel_time_matrix
#' @param origins A `data.frame` with columns `id`, `lon` and `lat`, or a
#'   `POINT sf` with an `id` column, in WGS 84.
#' @param mode A character vector, the transport modes (default
#'   `"transit"`): one of `"WALK"`, `"BICYCLE"` or `"CAR"`, and for public
#'   transport `"TRANSIT"` or some of its modes (see
#'   [travel_time_matrix()]).
#' @param cutoffs A numeric vector, the travel times in minutes that delimit
#'   the isochrones. Values are sorted and duplicates removed; 0 is added and
#'   at least one value must be greater than 0. Defaults to `c(0, 15, 30)`.
#' @param zoom A number from 9 to 12: the Web Mercator zoom level of the
#'   travel-time grid polygons are interpolated from (only used with
#'   `polygon_output = TRUE`). Defaults to 10 (cells of about 153 metres at
#'   the Equator).
#' @param polygon_output A logical. `TRUE` (the default) for polygon
#'   isochrones, `FALSE` for the street segments reached.
#' @param max_trip_duration Not used: the trip limit comes from
#'   `max(cutoffs)`.
#' @param percentiles An integer vector of up to 5 values from 1 to 99, the
#'   travel time percentiles to make polygons for (one set per percentile).
#'   `NULL` (the default) means 50. Must be `NULL` with
#'   `polygon_output = FALSE`.
#' @param sample_size Deprecated and ignored (a warning when
#'   given).
#'
#' @return A `"sf" "data.frame"` in WGS 84. With `polygon_output = TRUE`,
#'   one `POLYGON` or `MULTIPOLYGON` per origin, percentile and cutoff, with
#'   columns `id`, `isochrone` (the cutoff), `percentile` (such as `"p50"`)
#'   and `polygons`; rows by origin, percentile and cutoff (descending). Each
#'   polygon covers the whole area reached from 0 up to its cutoff. With
#'   `polygon_output = FALSE`, one `LINESTRING` per street segment reached,
#'   with columns `id`, `edge_index`, `osm_id`, `isochrone` (the smallest
#'   cutoff at or above the segment's travel time), `travel_time_p50`, the
#'   segment's attributes from [street_network_to_sf()]
#'   (`from_vertex`, `to_vertex`, `street_class`, `length`, `walk`, `car`,
#'   `car_speed`, `bicycle`, `bicycle_lts`) and `geometry`; rows by origin,
#'   then by band and travel time, both descending.
#' @family routing functions
#' @export
#' @examplesIf requireNamespace("sf", quietly = TRUE)
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' points <- read.csv(file.path(data_path, "spo_points.csv"))
#' departure <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")
#'
#' # polygons: the areas reached on foot within 10 and 20 minutes
#' iso <- isochrone(network, points[1, ], mode = "WALK",
#'                  cutoffs = c(0, 10, 20), departure_datetime = departure,
#'                  n_threads = 2)
#' iso
#'
#' # lines: the street segments reached by public transport
#' iso_lines <- isochrone(network, points[1, ], mode = "TRANSIT",
#'                        cutoffs = c(0, 10, 20), polygon_output = FALSE,
#'                        departure_datetime = departure, seed = 42,
#'                        n_threads = 2)
#' head(iso_lines)
isochrone <- function(r5r_network,
                      origins,
                      mode = "transit",
                      mode_egress = "walk",
                      cutoffs = c(0, 15, 30),
                      zoom = 10,
                      departure_datetime = NULL,
                      polygon_output = TRUE,
                      time_window = 10L,
                      max_walk_time = Inf,
                      max_bike_time = Inf,
                      max_car_time = Inf,
                      max_trip_duration = 120L,
                      walk_speed = 3.6,
                      bike_speed = 12,
                      max_rides = 3,
                      max_lts = 2,
                      draws_per_minute = 5L,
                      seed = NULL,
                      percentiles = NULL,
                      n_threads = Inf,
                      verbose = FALSE,
                      progress = TRUE,
                      sample_size = deprecated(),
                      r5r_core = deprecated()) {
  # mirrors r5r R/isochrone.R
  if (is_present(r5r_core)) {
    warn_r5r_core()
    r5r_network <- r5r_core
  }
  if (is_present(sample_size)) {
    warning("The `sample_size` argument is no longer used and has no effect.",
            call. = FALSE)
  }
  assert_network(r5r_network)
  if (!requireNamespace("sf", quietly = TRUE)) {
    stop("Package 'sf' is needed for isochrone(); install it with ",
         "install.packages(\"sf\").", call. = FALSE)
  }

  cm_assert(check_numeric(cutoffs, lower = 0, finite = TRUE, any.missing = FALSE,
                          min.len = 1), "cutoffs")
  cm_assert(check_logical(polygon_output), "polygon_output")
  # r5r takes anything but TRUE as FALSE and then fails on an undefined
  # object; an error of peabiru's own here
  assert_flag(polygon_output, "polygon_output")

  # the largest cutoff is the trip limit; the user's max_trip_duration is
  # ignored
  max_trip_duration <- as.integer(max(cutoffs))
  cutoffs <- sort(unique(c(0, cutoffs)))
  if (length(cutoffs) < 2) {
    cli_abort_text("`cutoffs` must contain at least one value greater than 0.")
  }

  if (polygon_output) {
    if (is.null(percentiles)) percentiles <- 50L
    cm_assert(check_numeric(zoom, lower = 9, upper = 12, len = 1), "zoom")
    # r5r lets a fractional or missing zoom through to R5
    assert_int(zoom, "zoom")
    isochrone_polygons(
      r5r_network, origins, mode, mode_egress, cutoffs, zoom,
      departure_datetime, time_window, max_walk_time, max_bike_time,
      max_car_time,
      # route past max(cutoffs) so the outer band is interpolated like the
      # inner ones
      max_trip_duration + 10L,
      walk_speed, bike_speed, max_rides, max_lts, draws_per_minute, seed,
      percentiles, n_threads, verbose, progress
    )
  } else {
    if (!is.null(percentiles)) {
      stop("Percentiles not supported for line-based isochrones", call. = FALSE)
    }
    isochrone_lines(
      r5r_network, origins, mode, mode_egress, cutoffs, departure_datetime,
      time_window, max_walk_time, max_bike_time, max_car_time,
      max_trip_duration, walk_speed, bike_speed, max_rides, max_lts,
      draws_per_minute, seed, n_threads, verbose, progress
    )
  }
}

# r5r's travel_time_surface() and surface_isochrone(): the argument handling
# of travel_time_surface(), then the core's surfaces and polygons, and
# sf::st_make_valid() as in r5r.
isochrone_polygons <- function(r5r_network, origins, mode, mode_egress, cutoffs,
                               zoom, departure_datetime, time_window,
                               max_walk_time, max_bike_time, max_car_time,
                               max_trip_duration, walk_speed, bike_speed,
                               max_rides, max_lts, draws_per_minute, seed,
                               percentiles, n_threads, verbose, progress) {
  origins <- assign_points_input(origins, "origins")
  ids <- origins$id
  mode_list <- assign_mode(mode, mode_egress)
  departure <- assign_departure(departure_datetime, mode_list)

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

  res <- raise_if_stopped(rs_isochrone_polygons(
    r5r_network$ptr, origins$lat, origins$lon,
    mode_list$direct_modes, mode_list$transit_mode, mode_list$egress_mode,
    as.double(cutoffs), as.integer(zoom), percentiles,
    departure$yyyymmdd, departure$seconds, as.integer(time_window),
    max_walk_time, max_bike_time, max_car_time, max_trip_duration, as.double(walk_speed),
    as.double(bike_speed), as.integer(max_lts), as.integer(max_rides),
    as.integer(draws_per_minute), seed$has, seed$value, threads
  ))

  # r5r's surface_isochrone() makes one sfc per origin and percentile and
  # validates it as a whole: st_make_valid() gives a common type within it
  # (a POLYGON becomes a MULTIPOLYGON next to one), and combining the
  # groups then keeps each group's type.
  geoms <- multipolygons(res)
  group <- (res$origin - 1L) * length(percentiles) + match(res$percentile, percentiles)
  groups <- split(geoms, factor(group, levels = unique(group)))
  polygons <- lapply(unname(groups), function(g) sf::st_make_valid(sf::st_sfc(g, crs = 4326)))
  polygons <- do.call(c, polygons)
  out <- new_df(list(
    id = ids[res$origin],
    isochrone = res$isochrone,
    percentile = paste0("p", res$percentile)
  ))
  out$polygons <- polygons
  sf::st_as_sf(out, sf_column_name = "polygons")
}

# The MULTIPOLYGON sfg of each row of rs_isochrone_polygons(), built as
# isoband's separate_polygons() builds them: lists of polygons, each a list
# of two-column matrices.
multipolygons <- function(res) {
  ring_of_point <- rep.int(seq_along(res$n_points), res$n_points)
  xs <- split(res$x, factor(ring_of_point, levels = seq_along(res$n_points)))
  ys <- split(res$y, factor(ring_of_point, levels = seq_along(res$n_points)))
  rings <- .mapply(function(x, y) matrix(c(x, y), ncol = 2L), list(xs, ys), NULL)
  poly_of_ring <- rep.int(seq_along(res$n_rings), res$n_rings)
  polys <- unname(split(rings, factor(poly_of_ring, levels = seq_along(res$n_rings))))
  row_of_poly <- rep.int(seq_along(res$n_polygons), res$n_polygons)
  rows <- unname(split(polys, factor(row_of_poly, levels = seq_along(res$n_polygons))))
  lapply(rows, function(p) structure(p, class = c("XY", "MULTIPOLYGON", "sfg")))
}

# r5r's line-based isochrones: travel_time_matrix() to the centroids of the
# street edges, banded by the core.
isochrone_lines <- function(r5r_network, origins, mode, mode_egress, cutoffs,
                            departure_datetime, time_window, max_walk_time,
                            max_bike_time, max_car_time, max_trip_duration,
                            walk_speed, bike_speed, max_rides, max_lts,
                            draws_per_minute, seed, n_threads, verbose,
                            progress) {
  edges <- rs_street_edges(r5r_network$ptr)
  destinations <- new_df(list(
    id = as.character(edges$edge_index), lon = edges$lon, lat = edges$lat
  ))
  ttm <- travel_time_matrix(
    r5r_network = r5r_network,
    origins = origins,
    destinations = destinations,
    mode = mode,
    mode_egress = mode_egress,
    departure_datetime = departure_datetime,
    time_window = time_window,
    max_walk_time = max_walk_time,
    max_bike_time = max_bike_time,
    max_car_time = max_car_time,
    max_trip_duration = max_trip_duration,
    walk_speed = walk_speed,
    bike_speed = bike_speed,
    max_rides = max_rides,
    max_lts = max_lts,
    draws_per_minute = draws_per_minute,
    seed = seed,
    n_threads = n_threads,
    verbose = verbose,
    progress = progress
  )

  ids <- unique(origins$id)
  n_o <- length(ids)
  n_e <- length(edges$edge_index)
  o <- match(ttm$from_id, as.character(ids))
  e <- match(ttm$to_id, destinations$id)
  minutes <- rep(NA_integer_, n_o * n_e)
  minutes[(o - 1L) * n_e + e] <- ttm$travel_time_p50
  res <- rs_isochrone_lines(n_o, n_e, minutes, as.double(cutoffs))

  k <- res$edge
  out <- new_df(list(
    id = as.character(ids)[res$origin],
    edge_index = as.character(edges$edge_index[k]),
    osm_id = edges$osm_id[k],
    isochrone = res$isochrone,
    travel_time_p50 = res$travel_time,
    from_vertex = edges$from_vertex[k],
    to_vertex = edges$to_vertex[k],
    street_class = edges$street_class[k],
    length = edges$length[k],
    walk = edges$walk[k],
    car = edges$car[k],
    car_speed = edges$car_speed[k],
    bicycle = edges$bicycle[k],
    bicycle_lts = edges$bicycle_lts[k]
  ))
  ends <- cumsum(edges$n_points)
  starts <- ends - edges$n_points
  lines <- lapply(k, function(i) {
    at <- (starts[i] + 1L):ends[i]
    structure(matrix(c(edges$x[at], edges$y[at]), ncol = 2L),
              class = c("XY", "LINESTRING", "sfg"))
  })
  out$geometry <- sf::st_sfc(lines, crs = 4326)
  out <- sf::st_sf(out)
  class(out) <- c("sf", "data.frame")
  out
}
