# Parity of the peabiru R package with r5r 2.5.0: the same calls through
# peabiru:: and r5r::, compared with identical() after converting r5r's
# data.table to a data.frame. Any difference (values, row order, column
# names, types or attributes) is reported and makes the script stop with an
# error; nothing is coerced away.
#
# Usage (from the repository root; r5r needs a JDK 21):
#   JAVA_HOME=/path/to/jdk21 Rscript bench/compare/r_parity.R [poa] [spo]
#
# The peabiru package must be installed (R_LIBS can point to a private
# library). Environment:
#   N_THREADS   threads for both packages (default 4)
#   R5R_XMX     JVM heap (default 6G)
#   SCENARIOS   comma-separated scenario name prefixes to run (default all)
#
# The networks are built from the sample data shipped with r5r (OSM .pbf and,
# for "-transit", the GTFS zips; never the elevation raster) in fresh
# directories under tempdir(), so r5r's network.dat cache is never reused.
#
# Origins and destinations are the r5r hexgrid of each city (POA 1227, SPO
# 323 points), all to all, except where a scenario says otherwise:
#   expanded_*       the first 50 origins x all destinations
#   arrival, arrival_v2  the first 200 origins x all destinations
#   arrival_midnight the first 50 origins x all destinations
#   walk_swap        all origins x the first 100 destinations (r5r swaps
#                    origins and destinations for WALK)
#   edge_*           the first 30 origins plus a point with NA coordinates
#                    and a point far from the network, x the first 60
#                    destinations

xmx <- Sys.getenv("R5R_XMX", "6G")
options(java.parameters = paste0("-Xmx", xmx))
suppressPackageStartupMessages({
  library(r5r)
  library(peabiru)
})

args <- commandArgs(trailingOnly = TRUE)
cities <- if (length(args) > 0) args else c("poa", "spo")
stopifnot(all(cities %in% c("poa", "spo")))
n_threads <- as.integer(Sys.getenv("N_THREADS", "4"))
only <- Sys.getenv("SCENARIOS", "")
only <- if (nzchar(only)) strsplit(only, ",")[[1]] else character(0)
wanted <- function(name) {
  length(only) == 0 || any(startsWith(name, only))
}

script_arg <- grep("^--file=", commandArgs(trailingOnly = FALSE), value = TRUE)
script_dir <- if (length(script_arg) > 0) {
  dirname(normalizePath(sub("^--file=", "", script_arg[1])))
} else {
  file.path(getwd(), "bench", "compare")
}
acc_settings <- read.csv(
  file.path(script_dir, "..", "r5r", "accessibility_settings.csv"),
  colClasses = "character", na.strings = character(0)
)

tz <- "America/Sao_Paulo"
departure <- as.POSIXct("2019-05-13 14:00:00", tz = tz)
work <- file.path(tempdir(), "r_parity")
unlink(work, recursive = TRUE)
dir.create(work)

results <- list()
failures <- character(0)

# Explain how two results differ, without coercing anything.
describe_difference <- function(a, b) {
  out <- character(0)
  if (!identical(class(a), class(b))) {
    out <- c(out, sprintf("class: peabiru %s, r5r %s",
                          paste(class(a), collapse = "/"), paste(class(b), collapse = "/")))
  }
  if (!identical(names(a), names(b))) {
    out <- c(out, sprintf("names: peabiru [%s], r5r [%s]",
                          paste(names(a), collapse = ","), paste(names(b), collapse = ",")))
  }
  attr_a <- attributes(a)
  attr_b <- attributes(b)
  for (k in union(names(attr_a), names(attr_b))) {
    if (k == "names") next
    if (!identical(attr_a[[k]], attr_b[[k]])) {
      out <- c(out, sprintf("attribute '%s': peabiru %s, r5r %s", k,
                            paste(utils::capture.output(str(attr_a[[k]])), collapse = " "),
                            paste(utils::capture.output(str(attr_b[[k]])), collapse = " ")))
    }
  }
  if (nrow(a) != nrow(b)) {
    out <- c(out, sprintf("rows: peabiru %d, r5r %d", nrow(a), nrow(b)))
  }
  for (j in intersect(names(a), names(b))) {
    x <- a[[j]]
    y <- b[[j]]
    if (!identical(typeof(x), typeof(y)) || !identical(attributes(x), attributes(y))) {
      out <- c(out, sprintf("column %s: peabiru %s, r5r %s", j, typeof(x), typeof(y)))
      next
    }
    if (length(x) == length(y) && !identical(x, y)) {
      diff <- which(!(x == y & !is.na(x) & !is.na(y)) & !(is.na(x) & is.na(y)))
      out <- c(out, sprintf("column %s: %d values differ, first at row %d (peabiru %s, r5r %s)",
                            j, length(diff), diff[1], format(x[diff[1]], digits = 17),
                            format(y[diff[1]], digits = 17)))
    }
  }
  if (length(out) == 0) out <- "not identical, but no difference found by column"
  out
}

compare <- function(city, name, detail, call_pb, call_r5) {
  label <- sprintf("%s/%s", city, name)
  t0 <- proc.time()[["elapsed"]]
  pb <- call_pb()
  t_pb <- proc.time()[["elapsed"]] - t0
  t0 <- proc.time()[["elapsed"]]
  r5 <- as.data.frame(call_r5())
  t_r5 <- proc.time()[["elapsed"]] - t0
  same <- identical(pb, r5)
  msg <- sprintf("%-32s %-44s rows %8d  peabiru %7.2f s  r5r %7.2f s  identical %s",
                 label, detail, nrow(r5), t_pb, t_r5, same)
  message(msg)
  if (!same) {
    why <- describe_difference(pb, r5)
    message(paste0("    ", why, collapse = "\n"))
    failures <<- c(failures, paste0(label, ": ", paste(why, collapse = "; ")))
  }
  results[[length(results) + 1]] <<- data.frame(
    city = city, scenario = name, detail = detail, rows = nrow(r5),
    peabiru_s = round(t_pb, 3), r5r_s = round(t_r5, 3), identical = same
  )
  invisible(same)
}

copy_network <- function(city, transit) {
  src <- system.file("extdata", city, package = "r5r")
  dir <- file.path(work, if (transit) paste0(city, "-transit") else city)
  dir.create(dir)
  files <- file.path(src, paste0(city, "_osm.pbf"))
  if (transit) files <- c(files, list.files(src, pattern = "\\.zip$", full.names = TRUE))
  stopifnot(all(file.copy(files, dir)))
  dir
}

for (city in cities) {
  message(sprintf("== %s ==", city))
  points <- read.csv(
    system.file("extdata", city, paste0(city, "_hexgrid.csv"), package = "r5r"),
    colClasses = c(id = "character")
  )
  nets <- list()
  for (kind in c("street", "transit")) {
    dir <- copy_network(city, kind == "transit")
    t0 <- proc.time()[["elapsed"]]
    pb <- peabiru::build_network(dir, n_threads = n_threads)
    t_pb <- proc.time()[["elapsed"]] - t0
    t0 <- proc.time()[["elapsed"]]
    r5 <- suppressMessages(r5r::build_network(dir))
    t_r5 <- proc.time()[["elapsed"]] - t0
    message(sprintf("build %-8s peabiru %.2f s, r5r %.2f s", kind, t_pb, t_r5))
    nets[[kind]] <- list(pb = pb, r5 = r5)
  }
  both <- function(kind, fun, ...) {
    a <- list(...)
    list(
      pb = function() do.call(getExportedValue("peabiru", fun),
                              c(list(nets[[kind]]$pb), a, n_threads = n_threads)),
      r5 = function() do.call(getExportedValue("r5r", fun),
                              c(list(nets[[kind]]$r5), a, n_threads = n_threads,
                                progress = FALSE))
    )
  }
  run <- function(name, detail, calls) {
    if (wanted(name)) compare(city, name, detail, calls$pb, calls$r5)
  }
  pct5 <- c(5L, 25L, 50L, 75L, 95L)
  transit <- c("WALK", "TRANSIT")

  # travel_time_matrix, transit
  run("ttm_transit", "tw 10, 5 percentiles", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = transit, departure_datetime = departure, percentiles = pct5, seed = 42L))
  run("ttm_transit_tw1", "tw 1, 5 percentiles", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = transit, departure_datetime = departure, time_window = 1L,
    percentiles = pct5, seed = 42L))
  run("ttm_transit_mwt15", "max_walk_time 15, 5 percentiles", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = transit, departure_datetime = departure, max_walk_time = 15L,
    percentiles = pct5, seed = 42L))
  run("ttm_transit_v2", "07:30:30, tw 5, mtd 60, rides 2, dpm 3", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = "TRANSIT", departure_datetime = as.POSIXct("2019-05-13 07:30:30", tz = tz),
    time_window = 5L, percentiles = c(1L, 99L, 50L), max_trip_duration = 60L,
    max_rides = 2, draws_per_minute = 3L, walk_speed = 4.5, seed = 7L))

  # subsets of the transit modes: POA has bus (EPTC) and rail (Trensurb),
  # SPO bus, rail and subway; SUBWAY alone leaves POA without any route
  run("modes_ttm_bus", "WALK+BUS, 5 percentiles", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = c("WALK", "BUS"), departure_datetime = departure, percentiles = pct5,
    seed = 42L))
  run("modes_ttm_rail", "RAIL, tw 5, rides 2, mwt 20", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = "RAIL", departure_datetime = departure, time_window = 5L,
    max_rides = 2, max_walk_time = 20L, seed = 42L))
  run("modes_ttm_subway", "SUBWAY, percentiles 10,90", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = c("WALK", "SUBWAY"), departure_datetime = departure,
    percentiles = c(10L, 90L), seed = 42L))
  run("modes_ttm_subway_rail", "SUBWAY+RAIL, 07:30:30, mtd 60, dpm 3", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = c("SUBWAY", "RAIL"), departure_datetime = as.POSIXct("2019-05-13 07:30:30", tz = tz),
    max_trip_duration = 60L, draws_per_minute = 3L, seed = 7L))
  run("modes_access_bus", "WALK+BUS step, cutoffs 30,60", both(
    "transit", "accessibility", origins = points, destinations = points,
    opportunities_colnames = "jobs", mode = c("WALK", "BUS"),
    departure_datetime = departure, cutoffs = c(30L, 60L), seed = 42L))
  run("modes_access_rail", "RAIL logistic, percentiles 25,75", both(
    "transit", "accessibility", origins = points, destinations = points,
    opportunities_colnames = c("jobs", "schools"), mode = "RAIL",
    departure_datetime = departure, decay_function = "logistic", cutoffs = 40L,
    decay_value = 8, percentiles = c(25L, 75L), seed = 42L))
  run("modes_expanded_rail", "first 50, RAIL, breakdown", both(
    "transit", "expanded_travel_time_matrix", origins = points[1:50, ], destinations = points,
    mode = c("WALK", "RAIL"), departure_datetime = departure, breakdown = TRUE, seed = 42L))
  run("modes_expanded_bus", "first 50, BUS, tw 5, no breakdown", both(
    "transit", "expanded_travel_time_matrix", origins = points[1:50, ], destinations = points,
    mode = "BUS", departure_datetime = departure, time_window = 5L, seed = 42L))
  run("modes_arrival_bus", "first 100, BUS, 16:00, breakdown", both(
    "transit", "arrival_travel_time_matrix", origins = points[1:100, ], destinations = points,
    mode = c("WALK", "BUS"), arrival_datetime = as.POSIXct("2019-05-13 16:00:00", tz = tz),
    breakdown = TRUE, seed = 42L))

  # bicycle and car access to public transport (R5 carries the search on
  # on foot from wherever the vehicle got, within the same limit)
  run("via_ttm_bike", "BICYCLE+TRANSIT, 5 percentiles", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = c("BICYCLE", "TRANSIT"), departure_datetime = departure,
    percentiles = pct5, seed = 42L))
  run("via_ttm_bike_v2", "bike 15, lts 3, mbt 20, mwt 15, tw 5", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = c("BICYCLE", "TRANSIT"), departure_datetime = departure,
    bike_speed = 15, max_lts = 3, max_bike_time = 20L, max_walk_time = 15L,
    time_window = 5L, seed = 42L))
  run("via_ttm_bike_lts1", "lts 1, mbt 10, rides 1", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = c("BICYCLE", "TRANSIT"), departure_datetime = departure,
    max_lts = 1, max_bike_time = 10L, max_rides = 1, seed = 42L))
  run("via_ttm_bike_rail", "BICYCLE+RAIL, lts 4, mtd 60", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = c("BICYCLE", "RAIL"), departure_datetime = departure, max_lts = 4,
    max_trip_duration = 60L, seed = 42L))
  run("via_ttm_car", "CAR+TRANSIT, 5 percentiles", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = c("CAR", "TRANSIT"), departure_datetime = departure,
    percentiles = pct5, seed = 42L))
  run("via_ttm_car_v2", "mct 10, mtd 60, 07:30:30, dpm 3", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = c("CAR", "TRANSIT"), departure_datetime = as.POSIXct("2019-05-13 07:30:30", tz = tz),
    max_car_time = 10L, max_trip_duration = 60L, draws_per_minute = 3L,
    walk_speed = 4.5, seed = 7L))
  run("via_ttm_car_bus", "CAR+BUS, mct 15, mwt 10", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = c("CAR", "BUS"), departure_datetime = departure, max_car_time = 15L,
    max_walk_time = 10L, seed = 42L))
  run("via_acc_bike", "BICYCLE+TRANSIT step, cutoffs 20,40,60", both(
    "transit", "accessibility", origins = points, destinations = points,
    opportunities_colnames = c("jobs", "healthcare"), mode = c("BICYCLE", "TRANSIT"),
    departure_datetime = departure, cutoffs = c(20L, 40L, 60L), max_bike_time = 15L,
    seed = 42L))
  run("via_acc_car", "CAR+TRANSIT linear, percentiles 25,50", both(
    "transit", "accessibility", origins = points, destinations = points,
    opportunities_colnames = "schools", mode = c("CAR", "TRANSIT"),
    departure_datetime = departure, decay_function = "linear", cutoffs = 45L,
    decay_value = 10, percentiles = c(25L, 50L), max_car_time = 20L, seed = 42L))
  run("via_expanded_bike", "first 50, BICYCLE+TRANSIT, breakdown", both(
    "transit", "expanded_travel_time_matrix", origins = points[1:50, ], destinations = points,
    mode = c("BICYCLE", "TRANSIT"), departure_datetime = departure, breakdown = TRUE,
    max_bike_time = 20L, seed = 42L))
  run("via_expanded_car", "first 50, CAR+TRANSIT, breakdown, tw 5", both(
    "transit", "expanded_travel_time_matrix", origins = points[1:50, ], destinations = points,
    mode = c("CAR", "TRANSIT"), departure_datetime = departure, breakdown = TRUE,
    time_window = 5L, max_car_time = 15L, seed = 42L))
  run("via_arrival_bike", "first 100, BICYCLE+BUS, 16:00, breakdown", both(
    "transit", "arrival_travel_time_matrix", origins = points[1:100, ], destinations = points,
    mode = c("BICYCLE", "BUS"), arrival_datetime = as.POSIXct("2019-05-13 16:00:00", tz = tz),
    breakdown = TRUE, max_bike_time = 15L, seed = 42L))
  run("via_arrival_car", "first 100, CAR+TRANSIT, 08:00", both(
    "transit", "arrival_travel_time_matrix", origins = points[1:100, ], destinations = points,
    mode = c("CAR", "TRANSIT"), arrival_datetime = as.POSIXct("2019-05-13 08:00:00", tz = tz),
    max_car_time = 20L, max_trip_duration = 60L, seed = 7L))

  # bicycle and car egress (R5 searches from every stop with its default
  # request: 5 km by bicycle at any traffic stress, 30 min by car)
  run("egress_ttm_bike", "TRANSIT, egress BICYCLE, 5 percentiles", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = transit, mode_egress = "BICYCLE", departure_datetime = departure,
    percentiles = pct5, seed = 42L))
  run("egress_ttm_bike_v2", "egress BICYCLE, bike 15, mbt 10, lts 1, tw 5", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = transit, mode_egress = "BICYCLE", departure_datetime = departure,
    bike_speed = 15, max_bike_time = 10L, max_lts = 1, time_window = 5L, seed = 42L))
  run("egress_ttm_car", "TRANSIT, egress CAR, 5 percentiles", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = transit, mode_egress = "CAR", departure_datetime = departure,
    percentiles = pct5, seed = 42L))
  run("egress_ttm_car_v2", "egress CAR, mct 8, mtd 60, 07:30:30", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = "TRANSIT", mode_egress = "CAR",
    departure_datetime = as.POSIXct("2019-05-13 07:30:30", tz = tz),
    max_car_time = 8L, max_trip_duration = 60L, seed = 7L))
  run("egress_ttm_bike_car", "BICYCLE+TRANSIT, egress CAR, mbt 15", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = c("BICYCLE", "TRANSIT"), mode_egress = "CAR", departure_datetime = departure,
    max_bike_time = 15L, seed = 42L))
  run("egress_ttm_car_bike", "CAR+BUS, egress BICYCLE, mct 15, mbt 12", both(
    "transit", "travel_time_matrix", origins = points, destinations = points,
    mode = c("CAR", "BUS"), mode_egress = "BICYCLE", departure_datetime = departure,
    max_car_time = 15L, max_bike_time = 12L, seed = 42L))
  run("egress_acc_bike", "egress BICYCLE step, cutoffs 30,60", both(
    "transit", "accessibility", origins = points, destinations = points,
    opportunities_colnames = c("jobs", "population"), mode = transit,
    mode_egress = "BICYCLE", departure_datetime = departure, cutoffs = c(30L, 60L),
    seed = 42L))
  run("egress_acc_car", "egress CAR exponential, cutoff 40", both(
    "transit", "accessibility", origins = points, destinations = points,
    opportunities_colnames = "jobs", mode = transit, mode_egress = "CAR",
    departure_datetime = departure, decay_function = "exponential", cutoffs = 40L,
    max_car_time = 15L, seed = 42L))
  run("egress_expanded_bike", "first 50, egress BICYCLE, breakdown", both(
    "transit", "expanded_travel_time_matrix", origins = points[1:50, ], destinations = points,
    mode = transit, mode_egress = "BICYCLE", departure_datetime = departure,
    breakdown = TRUE, seed = 42L))
  run("egress_expanded_car", "first 50, CAR+TRANSIT, egress CAR, breakdown", both(
    "transit", "expanded_travel_time_matrix", origins = points[1:50, ], destinations = points,
    mode = c("CAR", "TRANSIT"), mode_egress = "CAR", departure_datetime = departure,
    breakdown = TRUE, time_window = 5L, max_car_time = 10L, seed = 42L))
  run("egress_arrival_bike", "first 100, egress BICYCLE, 16:00, breakdown", both(
    "transit", "arrival_travel_time_matrix", origins = points[1:100, ], destinations = points,
    mode = transit, mode_egress = "BICYCLE",
    arrival_datetime = as.POSIXct("2019-05-13 16:00:00", tz = tz), breakdown = TRUE,
    max_bike_time = 15L, seed = 42L))
  run("egress_arrival_car", "first 100, egress CAR, 08:00", both(
    "transit", "arrival_travel_time_matrix", origins = points[1:100, ], destinations = points,
    mode = transit, mode_egress = "CAR",
    arrival_datetime = as.POSIXct("2019-05-13 08:00:00", tz = tz),
    max_car_time = 20L, max_trip_duration = 60L, seed = 7L))

  # travel_time_matrix, street modes, on the street-only network and on the
  # transit network (whose streets carry the stop links)
  for (kind in c("street", "transit")) {
    for (m in c("WALK", "BICYCLE", "CAR")) {
      run(sprintf("ttm_%s_%s", kind, m), "defaults", both(
        kind, "travel_time_matrix", origins = points, destinations = points,
        mode = m, departure_datetime = departure))
    }
  }
  run("ttm_street_WALK_pct", "percentiles 25,75, mtd 30, walk 4.2", both(
    "street", "travel_time_matrix", origins = points, destinations = points,
    mode = "WALK", departure_datetime = departure, percentiles = c(75L, 25L),
    max_trip_duration = 30L, walk_speed = 4.2))
  run("ttm_street_BICYCLE_v2", "bike 15, lts 3, max_bike_time 20", both(
    "street", "travel_time_matrix", origins = points, destinations = points,
    mode = "BICYCLE", departure_datetime = departure, bike_speed = 15,
    max_lts = 3, max_bike_time = 20L))
  run("walk_swap", "WALK, all x first 100 (r5r swaps O/D)", both(
    "street", "travel_time_matrix", origins = points,
    destinations = points[1:100, ], mode = "WALK", departure_datetime = departure))

  # edge cases: a point without coordinates and one far from the network
  odd <- rbind(points[1:30, ],
               transform(points[1, ], id = "na_point", lon = NA_real_),
               transform(points[1, ], id = "far_point", lon = 0, lat = 0))
  quiet <- function(calls) {
    lapply(calls, function(f) function() suppressWarnings(f()))
  }
  run("edge_ttm_WALK", "NA and far points", quiet(both(
    "street", "travel_time_matrix", origins = odd, destinations = points[1:60, ],
    mode = "WALK", departure_datetime = departure)))
  run("edge_ttm_transit", "NA and far points", quiet(both(
    "transit", "travel_time_matrix", origins = odd, destinations = rbind(points[1:60, ], odd[31:32, ]),
    mode = transit, departure_datetime = departure, seed = 42L)))
  run("edge_access_transit", "NA and far points", quiet(both(
    "transit", "accessibility", origins = odd, destinations = rbind(points[1:60, ], odd[31:32, ]),
    opportunities_colnames = "jobs", mode = transit, departure_datetime = departure,
    cutoffs = c(30L, 60L), seed = 42L)))

  # accessibility, every setting of bench/r5r/accessibility_settings.csv
  split_num <- function(x) if (x == "") NULL else as.numeric(strsplit(x, ";")[[1]])
  for (i in seq_len(nrow(acc_settings))) {
    s <- acc_settings[i, ]
    run(paste0("access_", s$setting), sprintf("%s %s", s$mode, s$decay), both(
      s$network, "accessibility", origins = points, destinations = points,
      opportunities_colnames = strsplit(s$opportunities, ";")[[1]],
      mode = strsplit(s$mode, ";")[[1]], departure_datetime = departure,
      time_window = as.integer(s$time_window),
      percentiles = as.integer(split_num(s$percentiles)),
      decay_function = s$decay, cutoffs = split_num(s$cutoffs),
      decay_value = split_num(s$decay_value),
      max_walk_time = as.numeric(s$max_walk_time),
      max_trip_duration = as.integer(s$max_trip_duration), seed = 42L))
  }

  # expanded and arrival matrices
  o50 <- points[1:50, ]
  o200 <- points[1:200, ]
  run("expanded_breakdown", "first 50 origins, breakdown", both(
    "transit", "expanded_travel_time_matrix", origins = o50, destinations = points,
    mode = transit, departure_datetime = departure, breakdown = TRUE, seed = 42L))
  run("expanded_nobd", "first 50 origins, no breakdown", both(
    "transit", "expanded_travel_time_matrix", origins = o50, destinations = points,
    mode = transit, departure_datetime = departure, breakdown = FALSE, seed = 42L))
  run("expanded_v2", "first 50, 07:30:30 tw 5 mwt 15 mtd 60", both(
    "transit", "expanded_travel_time_matrix", origins = o50, destinations = points,
    mode = transit, departure_datetime = as.POSIXct("2019-05-13 07:30:30", tz = tz),
    time_window = 5L, breakdown = TRUE, max_walk_time = 15L,
    max_trip_duration = 60L, max_rides = 2, draws_per_minute = 3L, seed = 7L))
  run("arrival", "first 200 origins, 16:00, breakdown", both(
    "transit", "arrival_travel_time_matrix", origins = o200, destinations = points,
    mode = transit, arrival_datetime = as.POSIXct("2019-05-13 16:00:00", tz = tz),
    breakdown = TRUE, seed = 42L))
  run("arrival_v2", "first 200 origins, 08:00, mwt 15 mtd 60", both(
    "transit", "arrival_travel_time_matrix", origins = o200, destinations = points,
    mode = transit, arrival_datetime = as.POSIXct("2019-05-13 08:00:00", tz = tz),
    breakdown = FALSE, max_walk_time = 15L, max_trip_duration = 60L,
    max_rides = 2, draws_per_minute = 5L, seed = 7L))
  run("arrival_midnight", "first 50 origins, 00:30 next day", both(
    "transit", "arrival_travel_time_matrix", origins = o50, destinations = points,
    mode = transit, arrival_datetime = as.POSIXct("2019-05-14 00:30:00", tz = tz),
    breakdown = TRUE, seed = 42L))
}

summary <- do.call(rbind, results)
print(summary, row.names = FALSE)
if (length(failures) > 0) {
  stop(length(failures), " of ", nrow(summary), " comparisons are not identical:\n",
       paste0("  ", failures, collapse = "\n"), call. = FALSE)
}
message(sprintf("all %d comparisons identical", nrow(summary)))
