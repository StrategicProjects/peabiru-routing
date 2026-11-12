# Fare structures, as r5r's R/fare_structure.R. The object is r5r's: a list
# with the global settings, three tables and the debug settings. r5r's
# tables are data.tables; these are data.frames.

# A table of the core (a list of columns) as a data.frame; with no rows,
# r5r's setDT() of jsonlite's empty list: a data.frame without columns.
fare_table <- function(cols) {
  if (length(cols[[1]]) == 0L) return(new_df(list()))
  new_df(cols)
}

fare_object <- function(x) {
  x$fares_per_type <- fare_table(x$fares_per_type)
  x$fares_per_transfer <- fare_table(x$fares_per_transfer)
  x$fares_per_route <- fare_table(x$fares_per_route)
  x
}

#' Setup a fare structure to calculate the monetary costs of trips
#'
#' Creates a basic fare structure that describes how transit fares should be
#' calculated in [travel_time_matrix()], [accessibility()],
#' [detailed_itineraries()] and [pareto_frontier()]: every route, fare type
#' and transfer costs `base_fare` (read as a 32-bit float, the type R5 keeps
#' fares in). The structure can then be edited to follow the fare rules of
#' the study area.
#'
#' The structure is a list:
#' * `max_discounted_transfers`: most transfers with a discount (default 1).
#' * `transfer_time_allowance`: minutes between the boardings of a
#'   discounted transfer (default 120).
#' * `fare_cap`: highest fare of a trip (`Inf`: no cap).
#' * `fares_per_type`: one row per fare type (`type`, `unlimited_transfers`,
#'   `allow_same_route_transfer`, `use_route_fare`, `fare`).
#' * `fares_per_transfer`: the fare of two rides together (`first_leg`,
#'   `second_leg`, `fare`), for the pairs of types with a discount.
#' * `fares_per_route`: the fare type of each GTFS route (`agency_id`,
#'   `agency_name`, `route_id`, `route_short_name`, `route_long_name`, `mode`,
#'   `route_fare`, `fare_type`), matched by `agency_id` and `route_id`.
#' * `debug_settings`: the debug settings (`output_file`, `trip_info`).
#'
#' How the fare of a trip follows from it: the first ride costs its route's
#' fare (the type's `fare`, or the route's `route_fare` when the type has
#' `use_route_fare`); each later ride of the same type as the previous one is
#' free when the type has `unlimited_transfers`; otherwise, while fewer than
#' `max_discounted_transfers` discounts were used, a ride boarded within
#' `transfer_time_allowance` minutes of the previous boarding, with a row of
#' `fares_per_transfer` for the two types (and not on the same route, unless
#' the type has `allow_same_route_transfer`) brings the two rides to that
#' row's `fare`; any other ride costs its full fare. The total is capped at
#' `fare_cap`. These are the rules of R5's `RuleBasedInRoutingFareCalculator`,
#' written for r5r and ported to the 'Rust' core.
#'
#' @section Compatibility with r5r:
#' The arguments, their defaults and checks and the structure are those of
#' `r5r::setup_fare_structure()` in r5r 2.5.0; its tables are `data.frame`s
#' rather than `data.table`s. Structures move between the two packages
#' through [write_fare_structure()] and [read_fare_structure()]. R5's own
#' fare calculators (a structure with a `type`, which r5r passes to R5) are
#' not available: the routing functions stop with an error on one.
#'
#' @param r5r_network A network built with [build_network()].
#' @param base_fare A number, at least 0: the fare of every route, type and
#'   transfer.
#' @param by A string: one fare type per mode (`"MODE"`, the default), per
#'   agency (`"AGENCY_ID"`, or `"AGENCY_NAME"`), or one type for all routes
#'   (`"GENERIC"`).
#' @param debug_path,debug_info A `.csv` path (or `NULL`) and one of
#'   `"MODE"`, `"ROUTE"` and `"MODE_ROUTE"` (or `NULL`), stored in the
#'   structure's `debug_settings`. No debug file is written whatever they
#'   say (nor does r5r 2.5.0 write one).
#' @param r5r_core Deprecated: the network under its name before r5r 2.3.0,
#'   accepted with a warning.
#'
#' @return A fare structure (a list, see Details).
#' @family fare structure
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' fare_structure <- setup_fare_structure(network, base_fare = 4.4)
#' fare_structure$fares_per_type
#' @export
setup_fare_structure <- function(r5r_network,
                                 base_fare,
                                 by = "MODE",
                                 debug_path = NULL,
                                 debug_info = NULL,
                                 r5r_core = deprecated()) {
  if (is_present(r5r_core)) {
    warn_r5r_core()
    r5r_network <- r5r_core
  }
  # r5r's checks, in its order (R/fare_structure.R)
  assert_network(r5r_network)
  cm_assert(check_numeric(base_fare, lower = 0, len = 1, any.missing = FALSE), "base_fare")
  by_options <- c("MODE", "AGENCY_ID", "AGENCY_NAME", "GENERIC")
  by <- toupper(by)
  cm_assert_and("by", check_string(by), check_names(by, subset.of = by_options))
  cm_assert(check_string_pattern(debug_path, "\\.csv$", null.ok = TRUE), "debug_path")
  if (is.null(debug_path) && !is.null(debug_info)) {
    stop("Please specify a file to write debug info to with 'debug_path'.", call. = FALSE)
  } else if (!is.null(debug_path) && is.null(debug_info)) {
    debug_info <- "ROUTE"
  }
  assert_string(debug_info, "debug_info", null_ok = TRUE)
  if (!is.null(debug_info)) {
    debug_info <- toupper(debug_info)
    cm_assert(check_names(debug_info, subset.of = c("MODE", "ROUTE", "MODE_ROUTE")),
              "debug_info")
  }
  # R5 reads base_fare as a Java float: past the float range it is
  # Infinity, which r5r's JSON round trip hands back as the text "Infinity"
  infinite <- base_fare >= 3.4028235677973366e38
  fs <- fare_object(rs_setup_fare_structure(r5r_network$ptr,
                                            if (infinite) 0 else as.double(base_fare), by))
  if (infinite) {
    text <- function(df, col) {
      if (nrow(df) > 0) df[[col]] <- rep("Infinity", nrow(df))
      df
    }
    fs$fares_per_type <- text(fs$fares_per_type, "fare")
    fs$fares_per_transfer <- text(fs$fares_per_transfer, "fare")
    fs$fares_per_route <- text(fs$fares_per_route, "route_fare")
  }
  fs$debug_settings <- if (!is.null(debug_path)) {
    list(output_file = debug_path, trip_info = debug_info)
  } else {
    list(output_file = "", trip_info = "MODE")
  }
  fs
}

#' Write a fare structure object to disk
#'
#' Writes a fare structure to a `.zip` file holding five `.csv` files
#' (`global_settings.csv`, `fares_per_type.csv`, `fares_per_transfer.csv`,
#' `fares_per_route.csv` and `debug_settings.csv`, in the format of
#' `data.table::fwrite()`), which [read_fare_structure()] reads back.
#'
#' @section Compatibility with r5r:
#' The arguments are those of `r5r::write_fare_structure()` in r5r 2.5.0,
#' and the CSV files are those r5r writes, byte for byte; the archive
#' around them stores them uncompressed, so its bytes differ from r5r's
#' (whose archive also holds the files' modification times). A file written
#' by either package reads back in the other. A `fare_structure` that is not
#' a fare structure, or whose settings have the wrong length, and a `NULL`
#' `file_path` are errors that say so (r5r checks nothing there and fails
#' while writing, or writes tables recycled to the wrong length).
#'
#' @param fare_structure A fare structure (see [setup_fare_structure()]).
#' @param file_path A path to a `.zip` file.
#' @return The path passed to `file_path`, invisibly.
#' @family fare structure
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' fare_structure <- setup_fare_structure(network, base_fare = 4.4)
#' tmpfile <- tempfile("sample_fare_structure", fileext = ".zip")
#' write_fare_structure(fare_structure, tmpfile)
#' @export
write_fare_structure <- function(fare_structure, file_path) {
  cm_assert(check_string_pattern(file_path, "\\.zip$", null.ok = TRUE), "file_path")
  global <- list(
    setting = c("max_discounted_transfers", "transfer_time_allowance", "fare_cap"),
    value = c(fare_structure$max_discounted_transfers,
              fare_structure$transfer_time_allowance, fare_structure$fare_cap)
  )
  debug <- list(
    setting = c("output_file", "trip_info"),
    value = c(fare_structure$debug_settings$output_file,
              fare_structure$debug_settings$trip_info)
  )
  tables <- list(global, fare_structure$fares_per_type,
                 fare_structure$fares_per_transfer, fare_structure$fares_per_route, debug)
  texts <- vapply(tables, function(t) rs_fwrite(as.list(t)), "")
  names <- c("global_settings.csv", "fares_per_type.csv", "fares_per_transfer.csv",
             "fares_per_route.csv", "debug_settings.csv")
  writeBin(rs_zip_texts(names, texts), file_path)
  invisible(file_path)
}

#' Read a fare structure object from a file
#'
#' Reads a fare structure from a `.zip` file written by
#' [write_fare_structure()] (or by r5r's): the five `.csv` files are read
#' with the columns and types of the structure (see
#' [setup_fare_structure()]), the way `data.table::fread()` reads them.
#'
#' @section Compatibility with r5r:
#' The arguments and the result are those of `r5r::read_fare_structure()`
#' in r5r 2.5.0, with `data.frame`s rather than `data.table`s. A
#' `file_path` that names several files is an error (r5r's check lets it
#' through and then fails unzipping them).
#'
#' @param file_path A path to a fare structure `.zip` file.
#' @param encoding A string, `"UTF-8"` (the default), `"unknown"` or
#'   `"Latin-1"`: how the text of the files is read.
#' @return A fare structure (see [setup_fare_structure()]).
#' @family fare structure
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' tmpfile <- tempfile("sample_fare_structure", fileext = ".zip")
#' write_fare_structure(setup_fare_structure(network, base_fare = 4.4), tmpfile)
#' fare_structure <- read_fare_structure(tmpfile)
#' fare_structure$transfer_time_allowance
#' @export
read_fare_structure <- function(file_path, encoding = "UTF-8") {
  cm_assert(check_file_exists(file_path, extension = "zip"), "file_path")
  cm_assert_and("encoding", check_string(encoding),
                check_names(encoding, subset.of = c("unknown", "UTF-8", "Latin-1")))
  if (length(file_path) > 1L) {
    stop("'file_path' names more than one file (r5r fails unzipping them).", call. = FALSE)
  }
  bytes <- readBin(file_path, "raw", file.info(file_path)$size)
  fs <- fare_object(rs_read_fare_structure(bytes, encoding))
  if (encoding == "Latin-1") {
    fix <- function(df) {
      # fread marks the bytes as latin1 without converting them; the core
      # read each byte as one character: back to the bytes, marked latin1
      for (j in seq_along(df)) if (is.character(df[[j]])) df[[j]] <- iconv(df[[j]], "UTF-8", "latin1")
      df
    }
    fs$fares_per_type <- fix(fs$fares_per_type)
    fs$fares_per_transfer <- fix(fs$fares_per_transfer)
    fs$fares_per_route <- fix(fs$fares_per_route)
  }
  fs
}

# r5r's assert_fare_structure() (R/fare_structure.R), check by check and in
# its order. r5r checks the tables with checkmate's expect_*(), which raise
# testthat's "expectation_failure" condition outside a test.
assert_fare_structure <- function(fare_structure) {
  fs <- fare_structure
  cm_assert(check_list_cm(fs, any.missing = FALSE), "fare_structure")
  if ("type" %in% names(fs)) {
    unsupported("A fare calculator of R5 (a fare structure with a 'type')")
  }
  element_names <- c("max_discounted_transfers", "transfer_time_allowance", "fare_cap",
                     "fares_per_type", "fares_per_transfer", "fares_per_route",
                     "debug_settings")
  cm_assert(check_names(names(fs), type = "unique", must.include = element_names,
                        subset.of = element_names), "names(fare_structure)")
  for (n in c("max_discounted_transfers", "transfer_time_allowance", "fare_cap")) {
    cm_assert(check_number(fs[[n]], lower = 0), paste0("fare_structure$", n))
  }
  at <- function(tbl, col) paste0("fare_structure$", tbl, "$", col)
  types <- fs$fares_per_type
  cm_expect(check_data_frame_cm(types), "fare_structure$fares_per_type")
  cm_expect(check_character(types$type, any.missing = FALSE, unique = TRUE),
            at("fares_per_type", "type"))
  for (col in c("unlimited_transfers", "allow_same_route_transfer", "use_route_fare")) {
    cm_expect(check_logical(types[[col]], any.missing = FALSE), at("fares_per_type", col))
  }
  cm_expect(check_numeric(types$fare, any.missing = FALSE, lower = 0, finite = TRUE),
            at("fares_per_type", "fare"))
  tr <- fs$fares_per_transfer
  cm_expect(check_data_frame_cm(tr), "fare_structure$fares_per_transfer")
  if (length(names(tr)) > 0) {
    for (col in c("first_leg", "second_leg")) {
      cm_expect(check_character(tr[[col]], any.missing = FALSE), at("fares_per_transfer", col))
      cm_assert(check_names(tr[[col]], subset.of = unique(types$type)),
                at("fares_per_transfer", col))
    }
    cm_expect(check_numeric(tr$fare, any.missing = FALSE, lower = 0, finite = TRUE),
              at("fares_per_transfer", "fare"))
  }
  rt <- fs$fares_per_route
  cm_expect(check_data_frame_cm(rt), "fare_structure$fares_per_route")
  for (col in c("agency_id", "agency_name", "route_id", "route_short_name",
                "route_long_name", "mode")) {
    cm_expect(check_character(rt[[col]], any.missing = FALSE), at("fares_per_route", col))
  }
  cm_expect(check_numeric(rt$route_fare, any.missing = FALSE, lower = 0, finite = TRUE),
            at("fares_per_route", "route_fare"))
  cm_expect(check_character(rt$fare_type, any.missing = FALSE),
            at("fares_per_route", "fare_type"))
  cm_assert(check_names(rt$fare_type, subset.of = unique(types$type)),
            at("fares_per_route", "fare_type"))
  debug_elements <- c("output_file", "trip_info")
  cm_assert(check_list_cm(fs$debug_settings), "fare_structure$debug_settings")
  cm_assert(check_names(names(fs$debug_settings), type = "unique",
                        must.include = debug_elements, subset.of = debug_elements),
            "names(fare_structure$debug_settings)")
  cm_assert(check_string(fs$debug_settings$output_file),
            "fare_structure$debug_settings$output_file")
  cm_assert(check_string(fs$debug_settings$trip_info),
            "fare_structure$debug_settings$trip_info")

  # past r5r's checks, R5 reads the structure in Java (Jackson), which fails
  # on a column it does not know
  columns <- list(
    fares_per_type = c("type", "unlimited_transfers", "allow_same_route_transfer",
                       "use_route_fare", "fare"),
    fares_per_transfer = c("first_leg", "second_leg", "fare"),
    fares_per_route = c("agency_id", "agency_name", "route_id", "route_short_name",
                        "route_long_name", "mode", "route_fare", "fare_type")
  )
  for (n in names(columns)) {
    extra <- setdiff(names(fs[[n]]), columns[[n]])
    if (length(extra) > 0) {
      stop("fare_structure$", n, " has column(s) r5r cannot read: ",
           paste0("'", extra, "'", collapse = ", "),
           " (r5r fails reading the structure in Java).", call. = FALSE)
    }
  }
  invisible(TRUE)
}

# checkmate's expect_*(): outside a test, testthat signals the failure as an
# error of class "expectation_failure" with this message
cm_expect <- function(res, name) {
  if (!isTRUE(res)) {
    stop(structure(
      class = c("expectation_failure", "expectation", "error", "condition"),
      list(message = sprintf("Check on '%s' failed: %s", name, res), call = NULL)
    ))
  }
  invisible(TRUE)
}

# checkmate::check_list(): a list that is not a data.frame
check_list_cm <- function(x, any.missing = TRUE) {
  if (!is.list(x) || is.data.frame(x)) return(cm_type_msg(x, "list"))
  cm_check_missings(x, any.missing = any.missing)
}

# checkmate::check_data_frame()
check_data_frame_cm <- function(x) {
  if (!is.data.frame(x)) return(cm_type_msg(x, "data.frame"))
  TRUE
}

# checkmate::check_string(pattern = ): the string checks, then the pattern
check_string_pattern <- function(x, pattern, null.ok = FALSE) {
  r <- check_string(x, null.ok = null.ok)
  if (!isTRUE(r)) return(r)
  if (!is.null(x) && !all(grepl(pattern, x[!is.na(x)]))) {
    return(sprintf("Must comply to pattern '%s'", pattern))
  }
  TRUE
}

# checkmate::check_file_exists(extension = ): S+, directory, existence, then
# the extension (case insensitive)
check_file_exists <- function(x, extension = NULL) {
  if (!(is.character(x) && length(x) >= 1L && !anyNA(x))) return("No file provided")
  w <- which(dir.exists(x))
  if (length(w)) return(sprintf("File expected, but directory in place: '%s'", x[w[1L]]))
  w <- which(!file.exists(x))
  if (length(w)) return(sprintf("File does not exist: '%s'", x[w[1L]]))
  if (!is.null(extension)) {
    ii <- Reduce(`|`, lapply(tolower(extension), endsWith, x = tolower(x)))
    if (!all(ii)) {
      return(sprintf(
        "File extension must be in {'%s'} (case insensitive), but file name is '%s'",
        paste0(extension, collapse = "','"), x[which(!ii)[1L]]
      ))
    }
  }
  TRUE
}

# r5r's set_fare_structure() and set_max_fare(): the structure is checked
# and, as in r5r, read in for the network's routes (whatever the modes); a
# finite max_fare needs a structure.
check_fares <- function(fare_structure, max_fare, r5r_network) {
  if (!is.null(fare_structure)) {
    assert_fare_structure(fare_structure)
    rs_check_fare_structure(r5r_network$ptr, fare_structure)
  }
  assert_number(max_fare, "max_fare", lower = 0)
  if (is.finite(max_fare) && is.null(fare_structure)) {
    cli_abort_text("`max_fare` requires a `fare_structure`.", c(
      x = paste0("`max_fare` is ", cli_val(max_fare), " but `fare_structure` is `NULL`, ",
                 "so no fares are calculated and the limit would be ignored."),
      i = paste0("Pass a fare structure (see `setup_fare_structure()`) or leave ",
                 "`max_fare` as ", cli_val(Inf), ".")
    ))
  }
  invisible(TRUE)
}

#' Calculate travel time and monetary cost Pareto frontier
#'
#' Fits the Pareto frontier of travel time and fare between each origin and
#' each destination: for every value of `fare_cutoffs`, in the order given,
#' the travel times of R5's fare router (McRAPTOR, over `time_window`
#' sampled departure times) when no trip may cost more than the cutoff; a
#' destination and percentile gets a row at each cutoff where its travel
#' time changes and is below `max_trip_duration`. `monetary_cost` is the
#' cutoff as the router reads it, a 32-bit float (`4.8` is
#' `4.800000190734863`). Routes given by frequencies are routed with Monte
#' Carlo draws: pass `seed` for reproducible results.
#'
#' @section Compatibility with r5r:
#' The arguments, their defaults and checks, the columns and the rows of the
#' result are those of `r5r::pareto_frontier()` in r5r 2.5.0, with the same
#' values (with frequencies, for the same `seed`); the result is a
#' `data.frame` rather than a `data.table`. `output_dir`, with which r5r
#' writes the result to CSV files, is not available: anything but `NULL`
#' is an error.
#'
#' @inheritParams travel_time_matrix
#' @param fare_structure A fare structure (see [setup_fare_structure()]);
#'   required.
#' @param fare_cutoffs A numeric vector, required: the fare cutoffs, at
#'   least 0, unique.
#' @param seed An integer or `NULL`: the seed of the Monte Carlo draws of
#'   frequency-based routes.
#' @param output_dir Must be `NULL` (the default): writing the frontier to
#'   files is not available.
#'
#' @return A `data.frame` with `from_id`, `to_id` (character), `percentile`,
#'   `travel_time` and `monetary_cost`.
#' @family fare structure
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' points <- read.csv(file.path(data_path, "spo_points.csv"))
#' fares <- setup_fare_structure(network, base_fare = 4.4)
#' pf <- pareto_frontier(
#'   network, origins = points[1:3, ], destinations = points,
#'   mode = c("WALK", "TRANSIT"),
#'   departure_datetime = as.POSIXct("2019-05-13 14:00:00",
#'                                   tz = "America/Sao_Paulo"),
#'   fare_structure = fares, fare_cutoffs = c(0, 4.4, 8.8),
#'   seed = 42, n_threads = 2
#' )
#' head(pf)
#' @export
pareto_frontier <- function(r5r_network,
                            origins,
                            destinations,
                            mode = c("WALK", "TRANSIT"),
                            mode_egress = "WALK",
                            departure_datetime = NULL,
                            time_window = 10L,
                            seed = NULL,
                            percentiles = 50L,
                            max_walk_time = Inf,
                            max_bike_time = Inf,
                            max_car_time = Inf,
                            max_trip_duration = 120L,
                            fare_structure,
                            fare_cutoffs,
                            walk_speed = 3.6,
                            bike_speed = 12,
                            max_rides = 3,
                            max_lts = 2,
                            n_threads = Inf,
                            verbose = FALSE,
                            progress = TRUE,
                            output_dir = NULL,
                            r5r_core = deprecated()) {
  # mirrors r5r R/pareto_frontier.R
  if (is_present(r5r_core)) {
    warn_r5r_core()
    r5r_network <- r5r_core
  }
  if (missing(fare_structure) || is.null(fare_structure)) {
    cli_abort_text("`fare_structure` is required.", c(
      i = "Create one with `setup_fare_structure()` or `read_fare_structure()`."
    ))
  }
  if (missing(fare_cutoffs)) {
    cli_abort_text("`fare_cutoffs` is required.", c(
      i = "Pass the monetary cutoffs of the frontier, e.g. the fares of your `fare_structure`."
    ))
  }
  origins <- assign_points_input(origins, "origins")
  destinations <- assign_points_input(destinations, "destinations")
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

  max_walk_time <- assign_max_street_time(max_walk_time, walk_speed, max_trip_duration, "walk")
  max_bike_time <- assign_max_street_time(max_bike_time, bike_speed, max_trip_duration, "bike")
  max_car_time <- assign_max_street_time(max_car_time, NULL, max_trip_duration, "car")
  max_trip_duration <- assign_max_trip_duration(
    max_trip_duration, mode_list, max_walk_time, max_bike_time, max_car_time
  )

  assert_count(time_window, "time_window", positive = TRUE)
  # r5r's set_percentiles(): no shortest length (an empty frontier then)
  assert_integerish(percentiles, "percentiles", lower = 1, upper = 99, max_len = 5,
                    unique = TRUE)
  percentiles <- sort(as.integer(percentiles))
  check_draws(1L, time_window, mcraptor = mode_list$transit)
  seed <- seed_args(seed)
  check_speed(walk_speed, "walk")
  check_speed(bike_speed, "bike")
  assert_count(max_rides, "max_rides", positive = TRUE)
  check_max_lts(max_lts)
  threads <- assign_n_threads(n_threads)
  check_messages(verbose, progress)
  check_fares(fare_structure, Inf, r5r_network)
  check_output_dir(output_dir)
  if (!is.null(output_dir)) unsupported("'output_dir' in pareto_frontier()")
  check_fare_cutoffs(fare_cutoffs)
  if (length(percentiles) == 0L) {
    return(new_df(list(from_id = character(), to_id = character(), percentile = integer(),
                       travel_time = integer(), monetary_cost = numeric())))
  }

  res <- raise_if_stopped(rs_pareto_frontier(
    r5r_network$ptr, origins$lat, origins$lon, destinations$lat, destinations$lon,
    mode_list$access_mode, if (mode_list$transit) mode_list$transit_mode else "",
    if (mode_list$transit) mode_list$egress_mode else mode_list$access_mode,
    departure$yyyymmdd, departure$seconds,
    as.integer(time_window), percentiles, max_walk_time, max_bike_time, max_car_time,
    max_trip_duration, as.double(walk_speed), as.double(bike_speed),
    as.integer(max_lts), as.integer(max_rides), seed$has, seed$value, threads,
    fare_structure, as.double(fare_cutoffs)
  ))
  out <- new_df(list(
    from_id = origins$id[res$from], to_id = destinations$id[res$to],
    percentile = res$percentile, travel_time = res$travel_time,
    monetary_cost = res$monetary_cost
  ))
  over <- which(out$travel_time > max_trip_duration)
  out$travel_time[over] <- NA_integer_
  out
}

# r5r set_fare_cutoffs(): checkmate::assert_numeric(lower = 0, any.missing =
# FALSE, min.len = 1, unique = TRUE)
check_fare_cutoffs <- function(fare_cutoffs) {
  cm_assert(check_numeric(fare_cutoffs, lower = 0, any.missing = FALSE, min.len = 1,
                          unique = TRUE), "fare_cutoffs")
  invisible(TRUE)
}
