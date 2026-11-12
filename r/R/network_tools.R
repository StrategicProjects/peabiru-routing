# r5r's network functions: find_snap(), street_network_bbox(),
# street_network_to_sf(), transit_network_to_sf(),
# check_transit_availability() and get_gtfs_errors().

needs_sf <- function(fun) {
  if (!requireNamespace("sf", quietly = TRUE)) {
    stop("Package 'sf' is needed for ", fun, "(); install it with ",
         "install.packages(\"sf\").", call. = FALSE)
  }
}

#' Find snapped locations of input points on street network
#'
#' Finds where points snap to the street network: snapping is how routing
#' places origins and destinations on the network. The nearest point of a
#' street the mode may use is looked for first within 300 metres of the
#' point and then, if there is none, within `radius`. A point that does not
#' snap is not routed. The street a point snaps to depends on the mode,
#' since some streets cannot be used by some modes (a pedestrian street
#' cannot be used to snap car trips).
#'
#' @section Compatibility with r5r:
#' The arguments, their defaults and checks, the columns and the values are
#' those of `r5r::find_snap()` in r5r 2.5.0; with
#' `options(peabiru.output = "data.table")` the result is identical to r5r's,
#' with the index on `found` that r5r's update of the points not found leaves
#' (when `options(datatable.auto.index)` is on, as by default). A `radius`
#' of `NA` or of length 0 is an error
#' that says so (r5r lets it through and fails in Java).
#'
#' @param r5r_network A network built with [build_network()].
#' @param points Either a `POINT sf` object with WGS84 CRS, or a
#'   `data.frame` containing the columns `id`, `lon` and `lat`.
#' @param radius Numeric. The maximum radius in meters within which to
#'   snap. Defaults to 1600m.
#' @param mode A string. Which mode to consider when trying to snap the
#'   points to the network. Defaults to `WALK`, also allows `BICYCLE` and
#'   `CAR`.
#' @param r5r_core Deprecated: the network under its name before r5r 2.3.0,
#'   accepted with a warning.
#'
#' @return A table with the original points (`point_id`, `lat`,
#'   `lon`), their snapped coordinates on the street network (`snap_lat`,
#'   `snap_lon`), the distance in meters between the point and its snapped
#'   location (`distance`, haversine on a sphere of radius 6,367,450 m, as
#'   R5 measures it) and whether the point snapped (`found`). Points that
#'   could not be snapped have `NA` coordinates and distance and
#'   `found = FALSE`.
#'
#'   `r doc_output_class()`
#' @family network functions
#' @export
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' points <- read.csv(file.path(data_path, "spo_points.csv"))
#' snaps <- find_snap(network, points, radius = 2000, mode = "WALK")
#' head(snaps)
find_snap <- function(r5r_network,
                      points,
                      radius = 1600,
                      mode = "WALK",
                      r5r_core = deprecated()) {
  kind <- output_class()
  if (is_present(r5r_core)) {
    warn_r5r_core()
    r5r_network <- r5r_core
  }
  assert_network(r5r_network)
  cm_assert(check_numeric(radius, lower = 0, finite = TRUE, max.len = 1), "radius")
  mode_options <- c("WALK", "BICYCLE", "CAR")
  cm_assert_and("mode", check_string(mode), check_names(mode, subset.of = mode_options))

  points <- assign_points_input(points, "points")
  if (length(radius) == 0L) {
    # r5r hands Java a zero-length radius; it fails there
    stop("'radius' must be a single number.", call. = FALSE)
  }
  # checkmate lets NA through: a logical NA has no Java method in r5r (it
  # fails), a numeric one snaps nothing in R5 (a radius of 0)
  if (is.logical(radius)) {
    stop("'radius' must be a number, not NA (r5r 2.5.0 fails on it in Java).",
         call. = FALSE)
  }
  if (is.na(radius)) radius <- 0
  res <- rs_find_snap(r5r_network$ptr, points$lat, points$lon,
                      as.double(radius), mode)
  out <- as_output(new_df(list(
    point_id = points$id,
    lat = points$lat,
    lon = points$lon,
    snap_lat = res$snap_lat,
    snap_lon = res$snap_lon,
    distance = res$distance,
    found = res$found
  )), kind)
  # r5r's snap_df[found == FALSE, `:=`(...)] leaves an index on `found`
  # (data.table's auto-indexing, when options(datatable.auto.index) is on):
  # the same subset, evaluated where data.table's syntax applies, leaves the
  # same index
  if (kind == "data.table") {
    eval(call("[", out, quote(found == FALSE), which = TRUE),
         envir = new.env(parent = asNamespace("data.table")))
  }
  out
}

#' Extract the geographic bounding box of the transport network
#'
#' The bounding box of the street network: the envelope of the street
#' layer, which covers every 'OpenStreetMap' node of the ways streets were
#' made from (it can be a little larger than the box of the edges of
#' [street_network_to_sf()]).
#'
#' @section Compatibility with r5r:
#' The arguments, their defaults and checks and the result are those of
#' `r5r::street_network_bbox()` in r5r 2.5.0, with the same coordinates.
#'
#' @inheritParams find_snap
#' @param output A character string specifying the desired output format.
#'   One of `"polygon"` (the default), `"bbox"`, or `"vector"`.
#'
#' @return By default (`output = "polygon"`), an `sfc` with a single
#'   `POLYGON` geometry. If `output = "bbox"`, an `sf` `bbox` object. If
#'   `output = "vector"`, a named numeric vector with `xmin`, `ymin`,
#'   `xmax`, `ymax` coordinates. All outputs are in WGS 84 (EPSG 4326).
#'   Package 'sf' is needed for the first two.
#' @family network functions
#' @export
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' street_network_bbox(network, output = "vector")
street_network_bbox <- function(r5r_network,
                                output = c("polygon", "bbox", "vector"),
                                r5r_core = deprecated()) {
  if (is_present(r5r_core)) {
    warn_r5r_core()
    r5r_network <- r5r_core
  }
  assert_network(r5r_network)
  output <- match.arg(output)
  coords_vec <- rs_street_bbox(r5r_network$ptr)
  names(coords_vec) <- c("xmin", "ymin", "xmax", "ymax")
  if (output == "vector") {
    return(coords_vec)
  }
  needs_sf("street_network_bbox")
  bbox <- sf::st_bbox(coords_vec, crs = 4326)
  if (output == "bbox") {
    return(bbox)
  }
  sf::st_as_sfc(bbox)
}

# A POINT sf of the columns in `df` and coordinate vectors (NA for an empty
# point), as r5r builds it with sfheaders::sf_point(): no "agr" attribute,
# and a bounding box that is NA when any coordinate is (Inf/-Inf for no
# points at all).
point_sf <- function(df, lon, lat) {
  pts <- .mapply(function(x, y) structure(c(x, y), class = c("XY", "POINT", "sfg")),
                 list(lon, lat), NULL)
  geometry <- sf::st_sfc(pts, crs = 4326)
  b <- attr(geometry, "bbox")
  b[] <- if (length(lon) == 0L) {
    c(Inf, Inf, -Inf, -Inf)
  } else {
    suppressWarnings(c(min(lon), min(lat), max(lon), max(lat)))
  }
  attr(geometry, "bbox") <- b
  df$geometry <- geometry
  structure(df, sf_column = "geometry", class = c("sf", "data.frame"))
}

# A LINESTRING sfc from n_points and flat x, y.
linestring_sfc <- function(n_points, x, y) {
  ends <- cumsum(n_points)
  starts <- ends - n_points
  lines <- lapply(seq_along(n_points), function(i) {
    at <- seq.int(starts[i] + 1L, length.out = n_points[i])
    structure(matrix(c(x[at], y[at]), ncol = 2L),
              class = c("XY", "LINESTRING", "sfg"))
  })
  sf::st_sfc(lines, crs = 4326)
}

#' Extract OpenStreetMap network in sf format
#'
#' The street network in `sf` format: the vertices at the ends of street
#' edges and every directed street edge (the connectors of transit stops
#' and park-and-rides left out), in the order of the network's edge index.
#'
#' @section Compatibility with r5r:
#' The arguments and the result are those of `r5r::street_network_to_sf()`
#' in r5r 2.5.0: the same rows in the same order, columns and values;
#' with `options(peabiru.output = "data.table")` both tables have r5r's class too (`vertices` a
#' plain `sf` data frame, `edges` an `sf` `data.table`).
#'
#' @inheritParams find_snap
#'
#' @return A list with two `sf` data frames in WGS 84: `vertices`
#'   (`POINT`), with columns `index`, `park_and_ride`, `bike_sharing` and
#'   `geometry`, and `edges` (`LINESTRING`), with columns `edge_index`,
#'   `osm_id`, `from_vertex`, `to_vertex`, `street_class`, `length` (meters),
#'   `walk`, `car`, `car_speed` (km/h), `bicycle`, `bicycle_lts` and
#'   `geometry`. Package 'sf' is needed.
#' @family network functions
#' @export
#' @examplesIf requireNamespace("sf", quietly = TRUE)
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' street_net <- street_network_to_sf(network)
#' head(street_net$edges)
street_network_to_sf <- function(r5r_network, r5r_core = deprecated()) {
  kind <- output_class()
  if (is_present(r5r_core)) {
    warn_r5r_core()
    r5r_network <- r5r_core
  }
  assert_network(r5r_network)
  needs_sf("street_network_to_sf")

  v <- rs_street_vertices(r5r_network$ptr)
  vertices <- point_sf(new_df(list(
    index = v$index,
    park_and_ride = v$park_and_ride,
    bike_sharing = v$bike_sharing
  )), v$lon, v$lat)

  e <- rs_street_edges(r5r_network$ptr)
  edges <- new_df(list(
    edge_index = e$edge_index,
    osm_id = e$osm_id,
    from_vertex = e$from_vertex,
    to_vertex = e$to_vertex,
    street_class = e$street_class,
    length = e$length,
    walk = e$walk,
    car = e$car,
    car_speed = e$car_speed,
    bicycle = e$bicycle,
    bicycle_lts = e$bicycle_lts
  ))
  edges$geometry <- linestring_sfc(e$n_points, e$x, e$y)
  edges <- sf::st_sf(edges)

  list(vertices = as_output_sf(vertices, kind, data_table = FALSE),
       edges = as_output_sf(edges, kind, data_table = TRUE))
}

#' Extract transit network in sf format
#'
#' The public transport network in `sf` format: the stops of the 'GTFS'
#' feeds, and the line of every trip pattern of each route.
#'
#' @section Compatibility with r5r:
#' The arguments and the result are those of `r5r::transit_network_to_sf()`
#' in r5r 2.5.0: the same rows in the same order, columns and values;
#' with `options(peabiru.output = "data.table")` both tables have r5r's class too (`stops` a plain
#' `sf` data frame, `routes` an `sf` `data.table`).
#'
#' @inheritParams find_snap
#'
#' @return A list with two `sf` data frames in WGS 84: `stops` (`POINT`),
#'   with columns `stop_index`, `stop_id` (prefixed by the feed id),
#'   `stop_name`, `linked_to_street` and `geometry`, and `routes`
#'   (`LINESTRING`), with columns `agency_id`, `agency_name`, `route_id`,
#'   `long_name`, `short_name`, `mode` and `geometry`. There is one route
#'   row per trip pattern (the trips of a route with the same stops), so
#'   the same route can appear more than once; its line is the GTFS shape
#'   of the first trip of the pattern or, without one, the positions of its
#'   stops. A stop not linked to the street network (outside the area of the
#'   `.pbf`, for instance) has an empty geometry (`POINT EMPTY`), and a
#'   pattern with no line is left out. Positions of stops are those of
#'   their vertices in the street network (the coordinates rounded down to
#'   1e-7 degrees). Package 'sf' is needed.
#' @family network functions
#' @export
#' @examplesIf requireNamespace("sf", quietly = TRUE)
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' transit_net <- transit_network_to_sf(network)
#' head(transit_net$stops)
transit_network_to_sf <- function(r5r_network, r5r_core = deprecated()) {
  kind <- output_class()
  if (is_present(r5r_core)) {
    warn_r5r_core()
    r5r_network <- r5r_core
  }
  assert_network(r5r_network)
  needs_sf("transit_network_to_sf")

  r <- rs_transit_routes(r5r_network$ptr)
  routes <- new_df(list(
    agency_id = r$agency_id,
    agency_name = r$agency_name,
    route_id = r$route_id,
    long_name = r$long_name,
    short_name = r$short_name,
    mode = r$mode
  ))
  routes$geometry <- linestring_sfc(r$n_points, r$x, r$y)
  routes <- sf::st_sf(routes)
  # as r5r: valid geometries, empty ones (no linked stop) dropped
  if (any(!sf::st_is_valid(routes))) {
    routes <- sf::st_make_valid(routes)
  }
  routes <- routes[!sf::st_is_empty(routes), ]

  s <- rs_transit_stops(r5r_network$ptr)
  stops <- point_sf(new_df(list(
    stop_index = s$stop_index,
    stop_id = s$stop_id,
    stop_name = s$stop_name,
    linked_to_street = s$linked_to_street
  )), s$lon, s$lat)

  list(stops = as_output_sf(stops, kind, data_table = FALSE),
       routes = as_output_sf(routes, kind, data_table = TRUE))
}

#' Check transit service availability by date
#'
#' Counts the public transport services in the GTFS feeds of a network that
#' are active on given dates, and their proportion of all services. Use it
#' to pick a departure
#' date with adequate service: public transport routing only finds trips
#' on dates when services run (per the GTFS calendars and date
#' exceptions).
#'
#' When no date arguments are supplied, the function discovers all dates
#' with an active service referenced by a retained transit trip, including
#' frequency-based trips. This accounts for weekday calendars and date
#' exceptions, including exception-only services. A date with active
#' service does not guarantee a trip at every departure time or between
#' every pair of points.
#'
#' You can also specify the dates to check in two ways: a vector of
#' specific dates in `dates`, or a continuous range with `start_date` and
#' `end_date`. You cannot combine these methods. In all modes,
#' `total_services` counts every imported service, not only those used by
#' trips; counts are of services, not trips. If calendar bounds are
#' invalid, automatic discovery fails naming the service; explicit date
#' queries remain available.
#'
#' @section Compatibility with r5r:
#' The arguments, their defaults and checks, the columns and the values are
#' those of `r5r::check_transit_availability()` in r5r 2.5.0; with `options(peabiru.output = "data.table")`
#' the result is identical to r5r's, its class included.
#'
#' @inheritParams find_snap
#' @param dates A vector of specific dates to be checked. Can be character
#'   strings in "YYYY-MM-DD" format, or objects of class `Date`. This
#'   argument cannot be used with `start_date` or `end_date`. Defaults to
#'   `NULL`; when all date arguments are `NULL`, operating dates are
#'   discovered from the network.
#' @param start_date The start date for a continuous date range. Must be a
#'   single character string in "YYYY-MM-DD" format or a `Date` object. Must
#'   be used with `end_date`.
#' @param end_date The end date for a continuous date range. Must be a
#'   single character string in "YYYY-MM-DD" format or a `Date` object. Must
#'   be used with `start_date`.
#'
#' @return A table with four columns: `date` (`Date`),
#'   `total_services`, `active_services` and `pct_active` (the proportion
#'   of active services). Automatic discovery returns dates in ascending
#'   order, or a zero-row table with these columns if no retained transit
#'   trips operate.
#' @family network functions
#' @export
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' head(check_transit_availability(network))
#' check_transit_availability(network, dates = c("2019-05-13", "2019-05-19"))
check_transit_availability <- function(r5r_network,
                                       dates = NULL,
                                       start_date = NULL,
                                       end_date = NULL,
                                       r5r_core = deprecated()) {
  kind <- output_class()
  if (is_present(r5r_core)) {
    warn_r5r_core()
    r5r_network <- r5r_core
  }
  assert_network(r5r_network)

  is_valid_dates_vector <- !is.null(dates) && is.null(start_date) && is.null(end_date)
  is_valid_date_range <- is.null(dates) && !is.null(start_date) && !is.null(end_date)
  discover_dates <- is.null(dates) && is.null(start_date) && is.null(end_date)
  if (!discover_dates && !is_valid_dates_vector && !is_valid_date_range) {
    cli_abort_text("Incorrect date arguments provided.", c(
      i = "Omit all date arguments for discovery, or use one of these methods:",
      "*" = "Use the `dates` argument to provide a vector of specific dates.",
      "*" = "Use both `start_date` and `end_date` to provide a continuous date range.",
      x = "You cannot mix these methods or provide an incomplete date range."
    ))
  }

  # r5r's parse_date_input()
  parse_date_input <- function(date_input, arg_name) {
    if (inherits(date_input, "Date")) {
      return(date_input)
    }
    if (!is.character(date_input)) {
      cli_abort_text(paste0("`", arg_name,
                            "` must be a vector of character strings or Date objects."))
    }
    if (any(!grepl("^\\d{4}-\\d{2}-\\d{2}$", date_input))) {
      cli_abort_text(bullets = c(
        x = paste0("Invalid date format found in `", arg_name, "`."),
        i = "Please use the strict \"'YYYY-MM-DD'\" format for all date strings."
      ))
    }
    parsed <- lapply(date_input, function(d) {
      tryCatch(as.Date(d), error = function(e) as.Date(NA))
    })
    parsed <- as.Date(do.call("c", parsed))
    if (anyNA(parsed)) {
      cli_abort_text(bullets = c(
        x = paste0("Could not parse all values in `", arg_name, "`."),
        i = "One or more dates are logically invalid (e.g., '2025-02-29')."
      ))
    }
    parsed
  }

  if (discover_dates) {
    found <- rs_transit_service_dates(r5r_network$ptr)
    dates_formatted <- sprintf("%04d-%02d-%02d", found %/% 10000L,
                               (found %/% 100L) %% 100L, found %% 100L)
    if (length(found) == 0L) dates_formatted <- character(0)
  } else if (!is.null(dates)) {
    dates_formatted <- format(parse_date_input(dates, "dates"), "%Y-%m-%d")
  } else {
    start_d <- parse_date_input(start_date, "start_date")
    end_d <- parse_date_input(end_date, "end_date")
    if (length(start_d) > 1 || length(end_d) > 1) {
      cli_abort_text("`start_date` and `end_date` must each be a single date.")
    }
    if (start_d > end_d) {
      cli_abort_text("`start_date` must be before or the same as `end_date`.")
    }
    dates_formatted <- format(seq(from = start_d, to = end_d, by = "day"), "%Y-%m-%d")
  }

  yyyymmdd <- as.integer(gsub("-", "", dates_formatted, fixed = TRUE))
  counts <- vapply(yyyymmdd, function(d) rs_transit_availability(r5r_network$ptr, d),
                   numeric(2))
  dim(counts) <- c(2L, length(yyyymmdd))
  total <- as.integer(counts[1, ])
  active <- as.integer(counts[2, ])
  as_output(new_df(list(
    date = as.Date(dates_formatted),
    total_services = total,
    active_services = active,
    # as.numeric: ifelse() of no dates is logical(0), r5r's column double
    pct_active = as.numeric(ifelse(total == 0L, 0, active / total))
  )), kind)
}

#' Get GTFS errors encountered in network building
#'
#' The errors that the 'GTFS' library of 'R5' (Conveyal's gtfs-lib) reports
#' when it reads the network's feeds. [build_network()] refuses to build a
#' network when any error has priority `"HIGH"`; this lists them, also for
#' a directory a network could not be built from.
#'
#' The feeds are read again (the files the network was built from, or the
#' `.zip` files of the directory given) with gtfs-lib's checks:
#' every check of its table loaders (empty required fields, numbers, times
#' and URLs that do not parse, values out of range, duplicate and missing
#' keys, references to stops, trips, routes, services, agencies and fares
#' that do not exist, tables missing, empty or in a subfolder of the
#' `.zip`) and of its post-load validation (services, parent stations,
#' stops in places with nearly no population, GTFS-Flex trips), with the
#' `file`, `line`, `type`, `field`, `id` and `priority` gtfs-lib gives. The
#' rows are listed feed by feed, in the order gtfs-lib finds them. A feed on
#' which gtfs-lib fails instead of listing errors (a date that is not
#' `YYYYMMDD`, a frequency of a trip that does not exist, a service or fare
#' without an id) is an error naming the cause, as is a `locations.geojson`
#' (of GTFS-Flex zones) that is not in UTF-8. GTFS-Flex zones and location
#' groups are read as gtfs-lib reads them, for its errors; on-demand trips
#' are not routed.
#'
#' @section Compatibility with r5r:
#' The argument and the columns are those of `r5r::get_gtfs_errors()` in
#' r5r 2.5.0, with the same rows, and the column types `data.table::fread()`
#' gives r5r (exactly, when 'data.table' is installed); with `options(peabiru.output = "data.table")`
#' the result is a `data.table`, as r5r's. r5r writes the errors to
#' `gtfs_errors.csv` in the data folder when it builds the network and
#' reads that file back, listing them in the order of a Java `HashSet`,
#' which changes from one run to the next; peabiru writes nothing, reads
#' the feeds again and lists the rows in a fixed order. On a feed where
#' gtfs-lib fails, r5r's `build_network()` fails without writing the file.
#' A folder that does not exist is an error of peabiru's own.
#'
#' @param r5r_network A network built with [build_network()], or the path
#'   of a directory with 'GTFS' feeds (`.zip`), for instance one a network
#'   could not be built from.
#'
#' @return A table with columns `file` (the table, without
#'   `.txt`), `line` (the record, the header being line 1; 0 for a whole
#'   table), `type` (gtfs-lib's error class), `field`, `id` (the key
#'   concerned, for duplicate keys and suspect stop locations) and
#'   `priority` (`"HIGH"`, `"MEDIUM"`, `"LOW"` or `"UNKNOWN"`).
#' @family network functions
#' @export
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' get_gtfs_errors(network)
get_gtfs_errors <- function(r5r_network) {
  kind <- output_class()
  if (inherits(r5r_network, "peabiru_network")) {
    assert_network(r5r_network)
    paths <- rs_feed_paths(r5r_network$ptr)
  } else if (is.character(r5r_network)) {
    assert_string(r5r_network, "r5r_network")
    if (!dir.exists(r5r_network)) {
      stop("Directory '", r5r_network, "' does not exist.", call. = FALSE)
    }
    paths <- rs_gtfs_files(normalizePath(path.expand(r5r_network)))
  } else {
    stop("Assertion on 'r5r_network' failed: Must inherit from class ",
         "'peabiru_network'/'character', but has class '",
         paste(class(r5r_network), collapse = "/"), "'.", call. = FALSE)
  }
  errors <- rs_gtfs_errors(paths)
  as_output(as_read_by_fread(errors), kind)
}

# The table r5r gets: data.table::fwrite() to gtfs_errors.csv, then
# data.table::fread() of the file, which guesses the type of each column
# (all empty: logical; "" for an NA string; numbers as numbers).
as_read_by_fread <- function(cols) {
  if (requireNamespace("data.table", quietly = TRUE)) {
    f <- tempfile(fileext = ".csv")
    on.exit(unlink(f), add = TRUE)
    data.table::fwrite(new_df(cols), f)
    return(as.data.frame(data.table::fread(f, header = TRUE)))
  }
  # without data.table, the same guesses for the values gtfs-lib writes
  guess <- function(x) {
    x[is.na(x)] <- ""
    if (all(x == "")) return(rep(NA, length(x)))
    if (all(x == "" | grepl("^[-+]?[0-9]+$", x))) {
      v <- suppressWarnings(as.numeric(x))
      if (all(is.na(v) | abs(v) <= .Machine$integer.max)) return(as.integer(v))
      return(v)
    }
    if (all(x == "" | !is.na(suppressWarnings(as.numeric(x))))) {
      return(suppressWarnings(as.numeric(x)))
    }
    x
  }
  out <- lapply(cols, function(x) guess(as.character(x)))
  out$line <- if (length(cols$line)) as.integer(cols$line) else logical(0)
  new_df(out)
}
