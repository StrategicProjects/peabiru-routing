# Input handling shared by the routing functions: the checks, defaults and
# conversions of their arguments.

all_transit_modes <- c(
  "TRANSIT", "TRAM", "SUBWAY", "RAIL", "BUS", "FERRY", "CABLE_CAR",
  "GONDOLA", "FUNICULAR"
)
all_direct_modes <- c("WALK", "BICYCLE", "CAR", "BICYCLE_RENT", "CAR_PARK")

# Points: a data.frame or POINT sf with id, lon, lat. An sf in another CRS
# is reprojected to WGS 84.
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
    # any CRS other than WGS 84 is reprojected, with a message
    df <- as_wgs84(df, name)
    # coordinates come from the geometry, never from attribute columns
    coords <- sf::st_coordinates(df)
    df <- sf::st_drop_geometry(df)
    df$lon <- coords[, 1L]
    df$lat <- coords[, 2L]
  }
  df <- as.data.frame(df)

  cm_assert(check_names(names(df), must.include = c("id", "lat", "lon")), name)
  cm_assert(check_numeric(df$lon), paste0(name, "$lon"))
  cm_assert(check_numeric(df$lat), paste0(name, "$lat"))

  # the core takes double coordinates
  df$lon <- as.double(df$lon)
  df$lat <- as.double(df$lat)

  if (!is.character(df$id)) {
    df$id <- as.character(df$id)
    warning("'", name, "$id' forcefully cast to character.", call. = FALSE)
  }

  na_coords <- is.na(df$lon) | is.na(df$lat)
  if (any(na_coords)) {
    n <- sum(na_coords)
    warning(cli_text(bullets = c(
      "!" = paste0(n, " point", if (n == 1L) "" else "s", " in `", name, "` ",
                   if (n == 1L) "has" else "have",
                   " missing lon/lat coordinates and will not be routed."),
      i = paste0("Affected id", if (n == 1L) "" else "s", ": ",
                 cli_val(df$id[na_coords]), ".")
    )), call. = FALSE)
  }

  cm_assert(check_logical(unique_ids, len = 1, any.missing = FALSE), "unique_ids")
  if (unique_ids && anyDuplicated(df$id) > 0) {
    dup_ids <- unique(df$id[duplicated(df$id)])
    cli_abort_text(paste0("`", name, "` must not contain duplicated ids."), c(
      x = paste0("Duplicated id", if (length(dup_ids) == 1L) "" else "s", ": ",
                 cli_val(dup_ids), ".")
    ))
  }

  df
}

# The modes of a call: `mode` and `mode_egress` as the lists the core takes.
assign_mode <- function(mode, mode_egress) {
  all_modes <- c(all_transit_modes, all_direct_modes)

  # unique() and toupper() come before the checks, so a factor or
  # a repeated mode is accepted
  mode <- toupper(unique(mode))
  cm_assert_and("mode", check_character(mode, min.len = 1, any.missing = FALSE),
                check_names(mode, subset.of = all_modes))

  mode_egress <- toupper(unique(mode_egress))
  cm_assert(check_string(mode_egress), "mode_egress")
  cm_assert(check_names(mode_egress, subset.of = c("WALK", "BICYCLE", "CAR")), "mode_egress")

  direct_modes <- mode[mode %in% all_direct_modes]
  transit_modes <- mode[mode %in% all_transit_modes]

  if (length(direct_modes) > 1) {
    stop(
      "Please use only 1 of {",
      paste0("'", direct_modes, "'", collapse = ","),
      "} when routing.",
      call. = FALSE
    )
  }
  if (any(c("CAR_PARK", "BICYCLE_RENT") %in% direct_modes)) {
    stop("CAR_PARK and BICYCLE_RENT are currently unsupported.",
         call. = FALSE)
  }

  access_mode <- direct_modes
  if (length(transit_modes) == 0) {
    transit_modes <- ""
    egress_mode <- ""
  } else {
    if ("TRANSIT" %in% transit_modes) transit_modes <- all_transit_modes
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

  mode_list
}

# The departure: date and time strings in the POSIXct's own
# time zone, no conversion.
assign_departure <- function(datetime, mode_list, name = "departure_datetime") {
  if (is.null(datetime)) {
    if (!mode_list$transit) {
      datetime <- Sys.time()
    } else {
      cli_abort_text(bullets = c(
        "!" = "You must pass a `departure_datetime` for public transport routing.",
        i = paste("Results depend on the departure time. Supply an explicit POSIXct",
                  "value to make results reproducible.")
      ))
    }
  }
  cm_assert(check_posixct(datetime, len = 1, any.missing = FALSE), name)

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

# max_walk_time, max_bike_time and max_car_time: Inf is the trip's limit.
assign_max_street_time <- function(max_time, speed, max_trip_duration, mode) {
  cm_assert_and("mode", check_string(mode),
                check_names(mode, subset.of = c("bike", "walk", "car")))
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

# max_trip_duration: with a street mode alone, the mode's own
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

# The opportunity columns: numeric columns, NA counted as 0.
assign_opportunities <- function(destinations, opportunities_colnames) {
  cm_assert(check_character(opportunities_colnames, min.len = 1, unique = TRUE,
                            any.missing = FALSE), "opportunities_colnames")
  cm_assert(check_names(names(destinations), must.include = opportunities_colnames),
            "destinations")
  lapply(opportunities_colnames, function(colname) {
    x <- destinations[[colname]]
    cm_assert(check_numeric(x, finite = TRUE), colname)
    x <- as.double(x)
    if (anyNA(x)) {
      n <- sum(is.na(x))
      warning(cli_text(paste0("`", colname, "` has ", n, " missing value",
                              if (n == 1L) "" else "s", ", treated as 0.")),
              call. = FALSE)
      x[is.na(x)] <- 0
    }
    x
  })
}

# decay_function and decay_value, checked together.
assign_decay_function <- function(decay_function, decay_value) {
  all_functions <- c("step", "exponential", "fixed_exponential", "linear", "logistic")
  cm_assert_and("decay_function", check_string(decay_function),
                check_names(decay_function, subset.of = all_functions))
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

  # the placeholder 0 when decay_value is NULL (the engine ignores it)
  list(fun = decay_function,
       value = if (is.null(decay_value)) 0 else as.double(decay_value))
}

# The cutoffs the engine receives (ascending, or the
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

# The percentiles: ascending, as the output columns. An empty vector is an
# error for every mode (without the check a street matrix would come back
# with no travel time column).
assign_percentiles <- function(percentiles) {
  assert_integerish(percentiles, "percentiles", lower = 1, upper = 99,
                    min_len = 1, max_len = 5, unique = TRUE)
  sort(as.integer(percentiles))
}

# n_threads: Inf means all cores (0 for the core). Under R CMD
# check with the limit on cores (_R_CHECK_LIMIT_CORES_, set by --as-cran and
# in CRAN's incoming checks) "all cores" is two, the most CRAN allows; a
# number given by the caller is used as it is.
assign_n_threads <- function(n_threads) {
  assert_number(n_threads, "n_threads", lower = 1)
  if (is.finite(n_threads)) assert_count(n_threads, "n_threads", positive = TRUE)
  if (is.infinite(n_threads)) {
    if (check_limits_cores()) 2L else 0L
  } else {
    as.integer(n_threads)
  }
}

# Whether R CMD check asks packages to keep to two cores: the variable is
# "TRUE" or "warn" then (any value but empty and a false one counts, as in
# parallel::makeCluster()).
check_limits_cores <- function() {
  limit <- tolower(Sys.getenv("_R_CHECK_LIMIT_CORES_", ""))
  nzchar(limit) && !limit %in% c("false", "f", "no", "n", "0")
}

# walk_speed and bike_speed: a finite number above 0.
check_speed <- function(speed, mode) {
  name <- paste0(mode, "_speed")
  assert_number(speed, name, finite = TRUE)
  if (speed <= 0) {
    stop("Assertion on '", name, "' failed: Must have value greater than 0.",
         call. = FALSE)
  }
  invisible(TRUE)
}

# max_lts: 1 to 4.
check_max_lts <- function(max_lts) {
  assert_count(max_lts, "max_lts", positive = TRUE)
  if (max_lts < 1 || max_lts > 4) {
    stop(max_lts, " is not a valid value for the maximum Level of Transit Stress (LTS).\n",
         "Please enter a value between 1 and 4.", call. = FALSE)
  }
  invisible(TRUE)
}

# output_dir: NULL or an existing directory (checkmate::assert_string() and
# checkmate::assert_directory_exists())
check_output_dir <- function(output_dir) {
  assert_string(output_dir, "output_dir", null_ok = TRUE)
  if (!is.null(output_dir)) cm_assert(check_directory_exists(output_dir), "output_dir")
  invisible(TRUE)
}

# The 'output' argument of the rs_* routing functions: NULL to return the
# result, or where to write the CSV files (one per origin, named after its
# id) and with which ids; a missing id is an error, since it names no file.
# The directory is expanded as R's file functions expand it.
csv_output <- function(output_dir, origins, destinations, ...) {
  if (is.null(output_dir)) return(NULL)
  if (anyNA(origins$id) || anyNA(destinations$id)) {
    stop("With 'output_dir', the ids of 'origins' and 'destinations' ",
         "cannot be missing.", call. = FALSE)
  }
  list(dir = path.expand(output_dir), from_ids = origins$id,
       to_ids = destinations$id, ...)
}

# draws_per_minute: with McRAPTOR (a fare structure, or the alternatives of
# detailed_itineraries()), the numbers of draws that R5's sampling of
# departure times cannot produce are rejected
check_draws <- function(draws_per_minute, time_window = 1L, mcraptor = FALSE) {
  assert_count(draws_per_minute, "draws_per_minute", positive = TRUE)
  if (mcraptor) {
    draws <- as.integer(time_window * draws_per_minute)
    if (draws == 200L) {
      stop(structure(class = c("peabiru_200_draws", "error", "condition"), list(
        message = cli_text(paste0(
          "A `time_window` of ", time_window, " minutes with ", draws_per_minute,
          if (draws_per_minute == 1) " draw" else " draws",
          " per minute gives exactly 200 Monte Carlo draws, which the ",
          "multi-criteria router does not accept."
        ), c(i = paste0(
          "Use a slightly different `time_window`, e.g. ", time_window + 1, "."
        ))),
        call = NULL
      )))
    }
    m <- 60 %/% draws_per_minute
    mu <- (m - 1) / 2 + m %/% 2
    span <- 60 * time_window
    sd <- sqrt(span * (m^2 - 1) / 12 / mu^3)
    z <- ((span - (m - 1) / 2) / mu + 1 - draws) / sd
    p <- if (m < 2) 0 else exp(-z^2 / 2) / sqrt(2 * pi) / sd
    if (p < 5e-5) {
      cli_abort_text(paste0(
        "`draws_per_minute` = ", draws_per_minute, " with a `time_window` of ",
        time_window, " minutes is not supported with a `fare_structure`."
      ), c(
        i = paste0("The multi-criteria router cannot sample this many departure times ",
                   "and could run indefinitely."),
        i = "Use a lower `draws_per_minute` or a shorter `time_window`."
      ))
    }
  }
  invisible(TRUE)
}

# Public transport needs services on the departure date: the share of the
# services active on it (R5's Service.activeOn) over all services.
check_transit_availability_on_date <- function(network, departure) {
  counts <- rs_transit_availability(network$ptr, departure$yyyymmdd)
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

# With WALK alone, more origins than destinations and no elevation raster
# in the data directory, the search runs from the destinations (fewer
# searches) and the columns are swapped back afterwards. Walking times are
# taken as the same in both directions for this; with slope costs they are
# not, so a raster turns the swap off.
swap_od <- function(origins, destinations, mode_list, network) {
  !mode_list$transit &&
    mode_list$direct_modes == "WALK" &&
    nrow(origins) > nrow(destinations) &&
    !network$has_tif
}

seed_args <- function(seed) {
  assert_int(seed, "seed", null_ok = TRUE)
  if (is.null(seed)) list(has = FALSE, value = 0L) else list(has = TRUE, value = as.integer(seed))
}

# 'progress' and 'verbose' are checked; peabiru prints no progress counter
# and has no log to make verbose.
check_messages <- function(verbose, progress) {
  assert_flag(verbose, "verbose")
  assert_flag(progress, "progress")
  invisible(TRUE)
}

# HH:MM:SS, hours not wrapped at 24.
seconds_to_hhmmss <- function(s) {
  sprintf("%02d:%02d:%02d", s %/% 3600L, (s %/% 60L) %% 60L, s %% 60L)
}

# An sf in WGS 84 (EPSG:4326): returned as it is when its CRS is WGS 84,
# reprojected with sf::st_transform() from any other CRS, with a message
# (one per argument, of class "peabiru_reprojected"), and an error when it
# has none, since its coordinates could then be in any system.
as_wgs84 <- function(x, name) {
  crs <- sf::st_crs(x)
  if (is.na(crs)) {
    stop("'", name, "' has no coordinate reference system (CRS). If its ",
         "coordinates are longitude and latitude in WGS 84, set it with ",
         "sf::st_set_crs(", name, ", 4326); otherwise set the CRS they are in, ",
         "and peabiru reprojects them to WGS 84.", call. = FALSE)
  }
  if (crs == sf::st_crs(4326)) return(x)
  what <- crs_label(crs)
  x <- tryCatch(sf::st_transform(x, 4326), error = function(e) {
    stop("'", name, "' cannot be reprojected from ", what, " to WGS 84 ",
         "(EPSG:4326): ", conditionMessage(e), call. = FALSE)
  })
  message(structure(class = c("peabiru_reprojected", "message", "condition"), list(
    message = paste0("'", name, "' reprojected from ", what,
                     " to WGS 84 (EPSG:4326).\n"),
    call = NULL
  )))
  x
}

# A CRS as people know it: its name and EPSG code, or what defined it.
crs_label <- function(crs) {
  label <- if (!is.null(crs$Name) && !identical(crs$Name, "unknown")) crs$Name else crs$input
  if (!is.na(crs$epsg)) label <- paste0(label, " (EPSG:", crs$epsg, ")")
  label
}
