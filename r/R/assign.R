# Input handling mirrored from r5r 2.5.0 (R/assign.R, R/set.R, R/utils.R):
# the same checks, defaults and conversions, so that a call gives what r5r
# gives for the same arguments.

r5r_transit_modes <- c(
  "TRANSIT", "TRAM", "SUBWAY", "RAIL", "BUS", "FERRY", "CABLE_CAR",
  "GONDOLA", "FUNICULAR"
)
r5r_direct_modes <- c("WALK", "BICYCLE", "CAR", "BICYCLE_RENT", "CAR_PARK")

# r5r assign_points_input(): a data.frame or POINT sf with id, lon, lat.
assign_points_input <- function(df, name, unique_ids = TRUE) {
  if (!inherits(df, "data.frame")) {
    stop("'", name, "' must be either a 'data.frame' or a 'POINT sf'.",
         call. = FALSE)
  }

  if (inherits(df, "sf")) {
    if (!requireNamespace("sf", quietly = TRUE)) {
      stop("Package 'sf' is needed to use an sf object as '", name, "'.",
           call. = FALSE)
    }
    if (as.character(sf::st_geometry_type(df, by_geometry = FALSE)) != "POINT") {
      stop("'", name, "' must be either a 'data.frame' or a 'POINT sf'.",
           call. = FALSE)
    }
    if (sf::st_crs(df) != sf::st_crs(4326)) {
      stop(
        "'", name, "' CRS must be WGS 84 (EPSG 4326). ",
        "Please use either sf::set_crs() to set it or ",
        "sf::st_transform() to reproject it.",
        call. = FALSE
      )
    }
    # coordinates come from the geometry, never from attribute columns
    coords <- sf::st_coordinates(df)
    df <- sf::st_drop_geometry(df)
    df$lon <- coords[, 1L]
    df$lat <- coords[, 2L]
  }
  df <- as.data.frame(df)

  missing_cols <- setdiff(c("id", "lat", "lon"), names(df))
  if (length(missing_cols) > 0) {
    fail_assert(name, paste0(
      "Names must include the elements {'id','lat','lon'}, but is missing ",
      "elements {", paste0("'", missing_cols, "'", collapse = ","), "}"
    ))
  }
  if (!is.numeric(df$lon)) fail_assert(paste0(name, "$lon"), "Must be of type 'numeric'")
  if (!is.numeric(df$lat)) fail_assert(paste0(name, "$lat"), "Must be of type 'numeric'")

  # the core takes double coordinates
  df$lon <- as.double(df$lon)
  df$lat <- as.double(df$lat)

  if (!is.character(df$id)) {
    df$id <- as.character(df$id)
    warning("'", name, "$id' forcefully cast to character.", call. = FALSE)
  }

  na_coords <- is.na(df$lon) | is.na(df$lat)
  if (any(na_coords)) {
    warning(
      sum(na_coords), " point(s) in '", name, "' have missing lon/lat ",
      "coordinates and will not be routed. Affected id(s): ",
      paste0("\"", df$id[na_coords], "\"", collapse = ", "), ".",
      call. = FALSE
    )
  }

  if (unique_ids && anyDuplicated(df$id) > 0) {
    dup_ids <- unique(df$id[duplicated(df$id)])
    stop(
      "'", name, "' must not contain duplicated ids. Duplicated id(s): ",
      paste0("\"", dup_ids, "\"", collapse = ", "), ".",
      call. = FALSE
    )
  }

  df
}

# r5r assign_mode(), plus the restriction to what the core supports.
assign_mode <- function(mode, mode_egress) {
  all_modes <- c(r5r_transit_modes, r5r_direct_modes)

  if (!is.character(mode) || length(mode) < 1L || anyNA(mode)) {
    fail_assert("mode", "Must be of type 'character' with at least 1 element and no missing values")
  }
  mode <- toupper(unique(mode))
  if (!all(mode %in% all_modes)) {
    fail_assert("mode", paste0(
      "Must be a subset of {", paste0("'", all_modes, "'", collapse = ","), "}"
    ))
  }

  assert_string(mode_egress, "mode_egress")
  mode_egress <- toupper(unique(mode_egress))
  if (!mode_egress %in% c("WALK", "BICYCLE", "CAR")) {
    fail_assert("mode_egress", "Must be a subset of {'WALK','BICYCLE','CAR'}")
  }

  direct_modes <- mode[mode %in% r5r_direct_modes]
  transit_modes <- mode[mode %in% r5r_transit_modes]

  if (length(direct_modes) > 1) {
    stop(
      "Please use only 1 of {",
      paste0("'", direct_modes, "'", collapse = ","),
      "} when routing.",
      call. = FALSE
    )
  }
  if (any(c("CAR_PARK", "BICYCLE_RENT") %in% direct_modes)) {
    stop("CAR_PARK and BICYCLE_RENT are currently unsupported by r5r.",
         call. = FALSE)
  }

  access_mode <- direct_modes
  if (length(transit_modes) == 0) {
    transit_modes <- ""
    egress_mode <- ""
  } else {
    if ("TRANSIT" %in% transit_modes) transit_modes <- r5r_transit_modes
    if (length(direct_modes) == 0) access_mode <- direct_modes <- "WALK"
    egress_mode <- mode_egress
  }

  mode_list <- list(
    direct_modes = paste0(direct_modes, collapse = ";"),
    transit_mode = paste0(transit_modes, collapse = ";"),
    access_mode = paste0(access_mode, collapse = ";"),
    egress_mode = paste0(egress_mode, collapse = ";"),
    transit = !identical(transit_modes, "")
  )

  # What the core can route: walk access and egress around all transit
  # modes, or one street mode alone.
  if (mode_list$transit) {
    if (!setequal(transit_modes, r5r_transit_modes)) {
      unsupported(paste0(
        "Routing on specific transit modes (",
        paste(setdiff(mode, r5r_direct_modes), collapse = ", "),
        "); use 'TRANSIT'"
      ))
    }
    if (access_mode != "WALK") {
      unsupported(paste0("Transit with ", access_mode, " access"))
    }
    if (egress_mode != "WALK") {
      unsupported(paste0("Transit with ", egress_mode, " egress"))
    }
  }

  mode_list
}

# r5r assign_departure(): date and time strings in the POSIXct's own
# time zone, no conversion.
assign_departure <- function(datetime, mode_list, name = "departure_datetime") {
  if (is.null(datetime)) {
    if (!mode_list$transit) {
      datetime <- Sys.time()
    } else {
      stop(
        "You must pass a '", name, "' for public transport routing. ",
        "Results depend on the departure time. Supply an explicit POSIXct ",
        "value to make results reproducible.",
        call. = FALSE
      )
    }
  }
  if (!inherits(datetime, "POSIXct") || length(datetime) != 1L || is.na(datetime)) {
    fail_assert(name, "Must be a single POSIXct date-time without missing values")
  }

  tz <- attr(datetime, "tzone")
  if (is.null(tz)) tz <- ""

  date <- strftime(datetime, format = "%Y-%m-%d", tz = tz)
  time <- strftime(datetime, format = "%H:%M:%S", tz = tz)
  hms <- as.integer(strsplit(time, ":", fixed = TRUE)[[1]])
  list(
    date = date,
    time = time,
    # what R5 makes of them: yyyymmdd and seconds after midnight
    yyyymmdd = as.integer(gsub("-", "", date, fixed = TRUE)),
    seconds = hms[1] * 3600L + hms[2] * 60L + hms[3]
  )
}

# r5r assign_max_street_time()
assign_max_street_time <- function(max_time, speed, max_trip_duration, mode) {
  var <- paste0("max_", mode, "_time")
  assert_number(max_time, var, lower = 1)
  if (is.finite(max_time)) assert_count(max_time, var, positive = TRUE)
  assert_number(speed, paste0(mode, "_speed"), finite = TRUE, null_ok = TRUE)
  assert_count(max_trip_duration, "max_trip_duration", positive = TRUE)
  if (!is.null(speed) && speed <= 0) {
    stop("Assertion on '", mode, "_speed' failed: ",
         "Must have value greater than 0.", call. = FALSE)
  }
  if (is.infinite(max_time)) as.integer(max_trip_duration) else as.integer(max_time)
}

# r5r assign_max_trip_duration(): with a street mode alone, the mode's own
# limit caps the trip.
assign_max_trip_duration <- function(max_trip_duration, modes, max_walk_time,
                                     max_bike_time, max_car_time) {
  assert_count(max_trip_duration, "max_trip_duration", positive = TRUE)
  max_trip_duration <- as.integer(max_trip_duration)
  if (!modes$transit) {
    if (modes$direct_modes == "WALK" && max_walk_time < max_trip_duration) {
      max_trip_duration <- max_walk_time
    }
    if (modes$direct_modes == "BICYCLE" && max_bike_time < max_trip_duration) {
      max_trip_duration <- max_bike_time
    }
    if (modes$direct_modes == "CAR" && max_car_time < max_trip_duration) {
      max_trip_duration <- max_car_time
    }
  }
  max_trip_duration
}

# r5r assign_opportunities(): numeric columns, NA counted as 0.
assign_opportunities <- function(destinations, opportunities_colnames) {
  if (!is.character(opportunities_colnames) || length(opportunities_colnames) < 1L ||
      anyNA(opportunities_colnames) || anyDuplicated(opportunities_colnames) > 0) {
    fail_assert("opportunities_colnames",
                "Must be a character vector of unique names without missing values")
  }
  missing_cols <- setdiff(opportunities_colnames, names(destinations))
  if (length(missing_cols) > 0) {
    fail_assert("destinations", paste0(
      "Names must include the elements {",
      paste0("'", opportunities_colnames, "'", collapse = ","),
      "}, but is missing elements {",
      paste0("'", missing_cols, "'", collapse = ","), "}"
    ))
  }
  lapply(opportunities_colnames, function(colname) {
    x <- destinations[[colname]]
    if (!is.numeric(x)) fail_assert(colname, "Must be of type 'numeric'")
    if (any(is.infinite(x))) fail_assert(colname, "Must be finite")
    x <- as.double(x)
    if (anyNA(x)) {
      warning("'", colname, "' has ", sum(is.na(x)),
              " missing value(s), treated as 0.", call. = FALSE)
      x[is.na(x)] <- 0
    }
    x
  })
}

# r5r assign_decay_function()
assign_decay_function <- function(decay_function, decay_value) {
  all_functions <- c("step", "exponential", "fixed_exponential", "linear", "logistic")
  assert_string(decay_function, "decay_function")
  if (!decay_function %in% all_functions) {
    fail_assert("decay_function", paste0(
      "Must be a subset of {", paste0("'", all_functions, "'", collapse = ","), "}"
    ))
  }
  assert_number(decay_value, "decay_value", finite = TRUE, null_ok = TRUE)

  if (!is.null(decay_value) && decay_function %in% c("step", "exponential")) {
    stop("Assertion on decay_value failed: must be NULL when decay_function ",
         "is ", decay_function, ".", call. = FALSE)
  } else if (is.null(decay_value) &&
             decay_function %in% c("fixed_exponential", "linear", "logistic")) {
    stop("Assertion on decay_value failed: must not be NULL when decay_function ",
         "is ", decay_function, ".", call. = FALSE)
  }
  if (decay_function == "fixed_exponential" && (decay_value <= 0 || decay_value >= 1)) {
    stop("Assertion on decay_value failed: must be a number between 0 and 1 ",
         "(exclusive) when decay_function is fixed_exponential.", call. = FALSE)
  }
  if (decay_function %in% c("logistic", "linear") && decay_value < 1) {
    stop("Assertion on decay_value failed: must be a number greater than or ",
         "equal to 1 when decay_function is ", decay_function, ".", call. = FALSE)
  }
  # R5 takes the linear width as whole minutes, under one hour
  if (decay_function == "linear") {
    assert_integerish(decay_value, "decay_value", lower = 1, upper = 59)
  }

  # r5r passes the placeholder 0 when decay_value is NULL (R5 ignores it)
  list(fun = decay_function,
       value = if (is.null(decay_value)) 0 else as.double(decay_value))
}

# r5r set_cutoffs(): returns the cutoffs R5 receives (ascending, or the
# placeholder 0 for fixed_exponential).
assign_cutoffs <- function(cutoffs, decay_function) {
  assert_integerish(cutoffs, "cutoffs", lower = 1, upper = 120, min_len = 1,
                    max_len = 12, unique = TRUE, null_ok = TRUE)
  if (!is.null(cutoffs) && decay_function == "fixed_exponential") {
    stop("Assertion on cutoffs failed: must be NULL when decay_function ",
         "is ", decay_function, ".", call. = FALSE)
  } else if (is.null(cutoffs) && decay_function != "fixed_exponential") {
    stop("Assertion on cutoffs failed: must not be NULL when decay_function ",
         "is ", decay_function, ".", call. = FALSE)
  }
  if (is.null(cutoffs)) 0L else sort(as.integer(cutoffs))
}

# r5r set_percentiles(): ascending, as the output columns. An empty vector is
# an error for every mode (R5 fails on it too; without the check a street
# matrix would come back with no travel time column).
assign_percentiles <- function(percentiles) {
  assert_integerish(percentiles, "percentiles", lower = 1, upper = 99,
                    min_len = 1, max_len = 5, unique = TRUE)
  sort(as.integer(percentiles))
}

# r5r set_n_threads(): Inf means all cores (0 for the core).
assign_n_threads <- function(n_threads) {
  assert_number(n_threads, "n_threads", lower = 1)
  if (is.finite(n_threads)) assert_count(n_threads, "n_threads", positive = TRUE)
  if (is.infinite(n_threads)) 0L else as.integer(n_threads)
}

# r5r set_speed()
check_speed <- function(speed, mode) {
  name <- paste0(mode, "_speed")
  assert_number(speed, name, finite = TRUE)
  if (speed <= 0) {
    stop("Assertion on '", name, "' failed: Must have value greater than 0.",
         call. = FALSE)
  }
  invisible(TRUE)
}

# r5r set_max_lts()
check_max_lts <- function(max_lts) {
  assert_count(max_lts, "max_lts", positive = TRUE)
  if (max_lts < 1 || max_lts > 4) {
    stop(max_lts, " is not a valid value for the maximum Level of Transit Stress (LTS).\n",
         "Please enter a value between 1 and 4.", call. = FALSE)
  }
  invisible(TRUE)
}

# r5r set_max_fare() and set_fare_structure(): fares are not supported.
check_fares <- function(fare_structure, max_fare) {
  if (!is.null(fare_structure)) unsupported("'fare_structure' (fare calculation)")
  assert_number(max_fare, "max_fare", lower = 0)
  if (is.finite(max_fare)) {
    stop("'max_fare' requires a 'fare_structure': 'max_fare' is ", max_fare,
         " but 'fare_structure' is NULL, so no fares are calculated and the ",
         "limit would be ignored. Leave 'max_fare' as Inf.", call. = FALSE)
  }
  invisible(TRUE)
}

# r5r set_new_congestion() and set_new_lts(): scenarios are not supported.
check_scenarios <- function(new_carspeeds, carspeed_scale, new_lts) {
  if (!is.null(new_carspeeds) && !inherits(new_carspeeds, "data.frame")) {
    fail_assert("new_carspeeds", "Must inherit from class 'data.frame'")
  }
  assert_number(carspeed_scale, "carspeed_scale", lower = 0, finite = TRUE)
  if (!is.null(new_lts) && !inherits(new_lts, "data.frame")) {
    fail_assert("new_lts", "Must inherit from class 'data.frame'")
  }
  if (!is.null(new_carspeeds) || carspeed_scale != 1) {
    unsupported("Changing car speeds ('new_carspeeds', 'carspeed_scale')")
  }
  if (!is.null(new_lts)) unsupported("Changing LTS levels ('new_lts')")
  invisible(TRUE)
}

check_output_dir <- function(output_dir) {
  assert_string(output_dir, "output_dir", null_ok = TRUE)
  if (!is.null(output_dir)) unsupported("Writing results to 'output_dir'")
  invisible(TRUE)
}

# r5r set_monte_carlo_draws() without a fare structure
check_draws <- function(draws_per_minute) {
  assert_count(draws_per_minute, "draws_per_minute", positive = TRUE)
  invisible(TRUE)
}

# r5r check_transit_availability_on_date() (R/utils.R): R5 counts the
# services active on the date (Service.activeOn) over all services.
check_transit_availability_on_date <- function(r5r_network, departure) {
  counts <- rs_transit_availability(r5r_network$ptr, departure$yyyymmdd)
  pct <- if (counts[1] == 0) 0 else counts[2] / counts[1]
  if (pct == 0 || is.na(pct)) {
    stop("There are no transit services available on the selected departure ",
         "date: \"", departure$date, "\". Please ensure there is a GTFS in your ",
         "data path & that the departure date falls within the GTFS calendar.",
         call. = FALSE)
  }
  if (pct < 0.2) {
    message("! Less than 20% of the transit services in the GTFS are running ",
            "on the selected departure date.")
  }
  invisible(TRUE)
}

# r5r reverse_if_direct_mode() (R/set.R): with WALK alone, more origins than
# destinations and no elevation raster in the data directory, r5r routes
# from the destinations and swaps the columns back afterwards. The two
# directions can differ, so the swap is reproduced.
swap_od <- function(origins, destinations, mode_list, r5r_network) {
  !mode_list$transit &&
    mode_list$direct_modes == "WALK" &&
    nrow(origins) > nrow(destinations) &&
    !r5r_network$has_tif
}

seed_args <- function(seed) {
  assert_int(seed, "seed", null_ok = TRUE)
  if (is.null(seed)) list(has = FALSE, value = 0L) else list(has = TRUE, value = as.integer(seed))
}

# 'progress' and 'verbose' are checked as r5r does; peabiru prints no
# progress counter and has no Java log to make verbose.
check_messages <- function(verbose, progress) {
  assert_flag(verbose, "verbose")
  assert_flag(progress, "progress")
  invisible(TRUE)
}

# Utils.getTimeFromSeconds in r5r's Java: HH:MM:SS, hours not wrapped at 24.
seconds_to_hhmmss <- function(s) {
  sprintf("%02d:%02d:%02d", s %/% 3600L, (s %/% 60L) %% 60L, s %% 60L)
}

# A plain data.frame with compact row names, as as.data.frame() gives.
new_df <- function(cols) {
  n <- length(cols[[1]])
  structure(cols, class = "data.frame",
            row.names = if (n > 0) c(NA_integer_, -n) else integer(0))
}
