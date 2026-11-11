# Parity of the peabiru R package with r5r 2.5.0 on networks built with an
# elevation raster: the same calls through peabiru:: and r5r::, compared with
# identical() (r5r's data.table turned into a data.frame; its sf results lose
# the data.table class). Any difference is reported and makes the script stop
# with an error.
#
# Usage (from the repository root; r5r needs a JDK 21):
#   JAVA_HOME=/path/to/jdk21 Rscript bench/compare/elevation_parity.R [poa] [poa-city]
#
# The peabiru package must be installed (R_LIBS can point to a private
# library), and sf. Environment:
#   N_THREADS   threads for both packages (default 4)
#   R5R_XMX     JVM heap (default 6G)
#   MODELS      comma-separated elevation models (default TOBLER,MINETTI)
#   SCENARIOS   comma-separated scenario name prefixes to run (default all)
#   POA_CITY    directory with poa_city.osm.pbf and points_700m.csv (default
#               bench/data/poa-city), for "poa-city"
#   RASTERS     comma-separated .tif files to use instead of poa_elevation.tif,
#               one network each (for instance the raster warped by GDAL to
#               other coordinate reference systems: SIRGAS 2000 and its UTM
#               zone, Polyconic, Pseudo-Mercator, SAD69...); peabiru's build
#               warnings (voids, no-data) are printed, not compared
#
# Each network is built by both packages in a fresh directory under
# tempdir() holding the OSM .pbf, r5r's two Porto Alegre GTFS zips and r5r's
# poa_elevation.tif ("poa-city": the whole city, which the raster covers only
# in part, so many streets run off its edge, where R5 reads 0 m). Origins and
# destinations: r5r's hexgrid (poa) or the 700 m grid (poa-city), and 400
# random points in the same box; a matrix with more origins than
# destinations checks that r5r does not swap them for walking when the
# directory has a raster. POA's GTFS only has service in the afternoon:
# transit departs at 14:00 on 2019-05-13.

xmx <- Sys.getenv("R5R_XMX", "6G")
options(java.parameters = paste0("-Xmx", xmx))
suppressPackageStartupMessages({
  library(r5r)
  library(peabiru)
  library(sf)
})

args <- commandArgs(trailingOnly = TRUE)
cities <- if (length(args) > 0) args else "poa"
stopifnot(all(cities %in% c("poa", "poa-city")))
n_threads <- as.integer(Sys.getenv("N_THREADS", "4"))
models <- strsplit(Sys.getenv("MODELS", "TOBLER,MINETTI"), ",")[[1]]
only <- Sys.getenv("SCENARIOS", "")
only <- if (nzchar(only)) strsplit(only, ",")[[1]] else character(0)
wanted <- function(name) length(only) == 0 || any(startsWith(name, only))
city_dir <- Sys.getenv("POA_CITY", file.path("bench", "data", "poa-city"))

tz <- "America/Sao_Paulo"
departure <- as.POSIXct("2019-05-13 14:00:00", tz = tz)
work <- file.path(tempdir(), "elevation_parity")
unlink(work, recursive = TRUE)
dir.create(work)
src <- system.file("extdata", "poa", package = "r5r")

failures <- character(0)
n_compared <- 0L

as_plain <- function(x) {
  if (inherits(x, "sf")) {
    class(x) <- setdiff(class(x), "data.table")
    attr(x, ".internal.selfref") <- NULL
    x
  } else {
    as.data.frame(x)
  }
}

compare <- function(label, call_pb, call_r5) {
  t0 <- proc.time()[["elapsed"]]
  w_pb <- character(0)
  pb <- withCallingHandlers(call_pb(), warning = function(w) {
    w_pb <<- c(w_pb, conditionMessage(w))
    invokeRestart("muffleWarning")
  })
  t_pb <- proc.time()[["elapsed"]] - t0
  t0 <- proc.time()[["elapsed"]]
  w_r5 <- character(0)
  r5 <- withCallingHandlers(as_plain(call_r5()), warning = function(w) {
    w_r5 <<- c(w_r5, conditionMessage(w))
    invokeRestart("muffleWarning")
  })
  t_r5 <- proc.time()[["elapsed"]] - t0
  # r5r gives the scenario warning on any network, peabiru with elevation
  # only (here, always): compare the elevation warnings
  # (cli wraps r5r's lines: compare the words)
  w_r5 <- gsub("\\s+", " ", grep("missing elevation", w_r5, value = TRUE))
  w_pb <- gsub("\\s+", " ", grep("missing elevation", w_pb, value = TRUE))
  same <- identical(pb, r5)
  same_w <- identical(w_pb, w_r5)
  n_compared <<- n_compared + 1L
  message(sprintf("%-46s rows %8d  peabiru %6.2f s  r5r %6.2f s  identical %s%s",
                  label, nrow(r5), t_pb, t_r5, same,
                  if (length(w_r5) || length(w_pb)) sprintf("  warnings %s", same_w) else ""))
  if (!same) {
    print(all.equal(pb, r5))
    failures <<- c(failures, label)
  }
  if (!same_w) {
    message("    peabiru warnings: ", paste(w_pb, collapse = " | "))
    message("    r5r warnings:     ", paste(w_r5, collapse = " | "))
    failures <<- c(failures, paste(label, "(warnings)"))
  }
}

rasters <- Sys.getenv("RASTERS", "")
rasters <- if (nzchar(rasters)) strsplit(rasters, ",")[[1]] else file.path(src, "poa_elevation.tif")
stopifnot(all(file.exists(rasters)))

make_dir <- function(city, model, raster) {
  dir <- file.path(work, paste0(city, "-", tolower(model), "-",
                                tools::file_path_sans_ext(basename(raster))))
  dir.create(dir)
  pbf <- if (city == "poa") file.path(src, "poa_osm.pbf") else file.path(city_dir, "poa_city.osm.pbf")
  files <- c(pbf, list.files(src, pattern = "\\.zip$", full.names = TRUE))
  stopifnot(all(file.copy(files, dir)),
            file.copy(raster, file.path(dir, "elevation.tif")))
  dir
}

random_points <- function(p, n, seed) {
  set.seed(seed)
  data.frame(id = sprintf("r%03d", seq_len(n)),
             lon = runif(n, min(p$lon), max(p$lon)),
             lat = runif(n, min(p$lat), max(p$lat)))
}

for (city in cities) {
  points <- if (city == "poa") {
    read.csv(file.path(src, "poa_hexgrid.csv"), colClasses = c(id = "character"))
  } else {
    read.csv(file.path(city_dir, "points_700m.csv"), colClasses = c(id = "character"))
  }
  points$jobs <- seq_len(nrow(points)) %% 17
  rnd <- random_points(points, 400, 11)
  few <- points[seq(1, nrow(points), length.out = 150), ]
  for (raster in rasters) for (model in models) {
    message(sprintf("== %s, elevation %s, %s ==", city, model, basename(raster)))
    dir <- make_dir(city, model, raster)
    t0 <- proc.time()[["elapsed"]]
    pb <- withCallingHandlers(
      peabiru::build_network(dir, elevation = model, n_threads = n_threads),
      warning = function(w) {
        message("    peabiru build warning: ", conditionMessage(w))
        invokeRestart("muffleWarning")
      })
    t_pb <- proc.time()[["elapsed"]] - t0
    t0 <- proc.time()[["elapsed"]]
    r5 <- suppressMessages(r5r::build_network(dir, elevation = model))
    t_r5 <- proc.time()[["elapsed"]] - t0
    message(sprintf("build peabiru %.2f s, r5r %.2f s; peabiru elevation %s", t_pb, t_r5,
                    pb$elevation))
    stopifnot(identical(pb$elevation, model))
    run <- function(name, fun, ...) {
      if (!wanted(name)) return(invisible())
      a <- list(...)
      label <- sprintf("%s/%s/%s%s", city, tolower(model), name,
                       if (length(rasters) > 1) paste0(" [", basename(raster), "]") else "")
      f5 <- getExportedValue("r5r", fun)
      extra_r5 <- if ("progress" %in% names(formals(f5))) list(progress = FALSE) else list()
      compare(label,
              function() do.call(getExportedValue("peabiru", fun),
                                 c(list(pb), a, n_threads = n_threads)),
              function() do.call(f5,
                                 c(list(r5), a, n_threads = n_threads, extra_r5)))
    }
    walk <- "WALK"
    bike <- "BICYCLE"
    # street matrices
    run("ttm_walk", "travel_time_matrix", origins = points, destinations = points, mode = walk)
    run("ttm_walk_fast", "travel_time_matrix", origins = rnd, destinations = points,
        mode = walk, walk_speed = 5.1, max_trip_duration = 45L)
    run("ttm_walk_swap", "travel_time_matrix", origins = points, destinations = rnd[1:50, ],
        mode = walk)
    run("ttm_bike", "travel_time_matrix", origins = points, destinations = points, mode = bike)
    run("ttm_bike_lts4", "travel_time_matrix", origins = rnd, destinations = points,
        mode = bike, bike_speed = 17, max_lts = 4)
    run("ttm_bike_lts1", "travel_time_matrix", origins = points, destinations = rnd,
        mode = bike, bike_speed = 9, max_lts = 1, max_trip_duration = 60L)
    run("ttm_car", "travel_time_matrix", origins = few, destinations = points, mode = "CAR")
    # transit with walking and cycling access
    run("ttm_transit", "travel_time_matrix", origins = points, destinations = points,
        mode = c("WALK", "TRANSIT"), departure_datetime = departure,
        percentiles = c(5L, 50L, 95L), seed = 42L)
    run("ttm_transit_bike", "travel_time_matrix", origins = rnd, destinations = points,
        mode = c("BICYCLE", "TRANSIT"), departure_datetime = departure, max_bike_time = 20L,
        bike_speed = 14, max_lts = 3, seed = 42L)
    # accessibility
    run("acc_walk", "accessibility", origins = points, destinations = points,
        opportunities_colnames = "jobs", mode = walk, cutoffs = c(15L, 30L))
    run("acc_bike_logistic", "accessibility", origins = rnd, destinations = points,
        opportunities_colnames = "jobs", mode = bike, decay_function = "logistic",
        cutoffs = 30L, decay_value = 6)
    run("acc_transit", "accessibility", origins = points, destinations = points,
        opportunities_colnames = "jobs", mode = c("WALK", "TRANSIT"),
        departure_datetime = departure, cutoffs = c(30L, 60L), seed = 42L)
    # expanded and arrival
    run("expanded_bike", "expanded_travel_time_matrix", origins = few[1:40, ],
        destinations = points, mode = c("BICYCLE", "TRANSIT"), max_bike_time = 20L,
        departure_datetime = departure, seed = 42L)
    run("expanded_transit", "expanded_travel_time_matrix", origins = few[1:40, ],
        destinations = points, mode = c("WALK", "TRANSIT"), departure_datetime = departure,
        breakdown = TRUE, seed = 42L)
    run("arrival_transit", "arrival_travel_time_matrix", origins = few[1:60, ],
        destinations = points, mode = c("BICYCLE", "TRANSIT"), max_bike_time = 15L,
        arrival_datetime = as.POSIXct("2019-05-13 16:00:00", tz = tz), seed = 42L)
    # isochrones
    run("iso_walk", "isochrone", origins = few[1:4, ], mode = walk,
        cutoffs = c(10L, 20L, 30L), departure_datetime = departure)
    run("iso_bike_lines", "isochrone", origins = few[5:7, ], mode = bike,
        cutoffs = c(15L, 30L), polygon_output = FALSE, departure_datetime = departure)
    run("iso_transit", "isochrone", origins = few[8:9, ], mode = c("WALK", "TRANSIT"),
        cutoffs = c(20L, 40L), departure_datetime = departure)
    # detailed itineraries
    set.seed(5)
    o <- points[sample(nrow(points), 40), ]
    d <- points[sample(nrow(points), 40), ]
    run("itin_walk", "detailed_itineraries", origins = o, destinations = d, mode = walk,
        departure_datetime = departure)
    run("itin_bike", "detailed_itineraries", origins = o, destinations = d, mode = bike,
        max_lts = 3, departure_datetime = departure)
    run("itin_transit", "detailed_itineraries", origins = o[1:20, ], destinations = d[1:20, ],
        mode = c("WALK", "TRANSIT"), departure_datetime = departure, drop_geometry = TRUE)
    run("itin_bike_links", "detailed_itineraries", origins = o, destinations = d,
        mode = bike, osm_link_ids = TRUE, departure_datetime = departure)
    run("itin_walk_links_transit", "detailed_itineraries", origins = o[1:15, ],
        destinations = d[1:15, ], mode = c("WALK", "TRANSIT"), osm_link_ids = TRUE,
        departure_datetime = departure)
    if (wanted("itin_output_dir")) {
      # the CSV files written with geometry, compared byte for byte
      d_pb <- tempfile("pb")
      d_r5 <- tempfile("r5")
      dir.create(d_pb)
      dir.create(d_r5)
      a <- list(origins = o[1:20, ], destinations = d[1:20, ], mode = bike,
                departure_datetime = departure, n_threads = n_threads)
      do.call(peabiru::detailed_itineraries, c(list(pb), a, output_dir = d_pb))
      suppressMessages(do.call(r5r::detailed_itineraries,
                               c(list(r5), a, output_dir = d_r5, progress = FALSE)))
      f_pb <- sort(list.files(d_pb))
      f_r5 <- sort(list.files(d_r5))
      same <- identical(f_pb, f_r5) &&
        all(unname(tools::md5sum(file.path(d_pb, f_pb))) ==
              unname(tools::md5sum(file.path(d_r5, f_r5))))
      n_compared <- n_compared + 1L
      message(sprintf("%-46s files %6d  identical %s",
                      sprintf("%s/%s/itin_output_dir", city, tolower(model)), length(f_r5), same))
      if (!same) failures <- c(failures, sprintf("%s/%s/itin_output_dir", city, model))
    }
    # a scenario: r5r routes on a copy of the network without the slope costs
    lts <- read.csv(file.path(src, "poa_osm_lts.csv"))
    run("scen_walk", "travel_time_matrix", origins = points, destinations = rnd,
        mode = walk, carspeed_scale = 0.8)
    run("scen_bike_lts", "travel_time_matrix", origins = rnd, destinations = points,
        mode = bike, new_lts = lts)
    run("scen_transit", "travel_time_matrix", origins = points, destinations = points,
        mode = c("WALK", "TRANSIT"), departure_datetime = departure, carspeed_scale = 1.2,
        seed = 42L)
    r5r::stop_r5(r5)
  }
}

message(sprintf("%d comparisons, %d failures", n_compared, length(failures)))
if (length(failures) > 0) {
  stop("not identical: ", paste(failures, collapse = "; "), call. = FALSE)
}
