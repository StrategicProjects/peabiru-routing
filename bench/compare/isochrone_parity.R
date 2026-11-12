# Parity of the peabiru R package's isochrone() with r5r 2.5.0's: the same
# calls through peabiru:: and r5r::, on r5r's sample cities.
#
# Each scenario is first compared with identical() (after dropping the
# "data.table" class and its internal attribute from r5r's polygons, which
# peabiru returns as a plain sf data.frame). Where that fails, the rows are
# compared one by one: the attribute columns must be identical, and each
# geometry is counted as identical, or as "within tolerance" when its area
# agrees with r5r's to 1e-9 (relative) and the symmetric difference of the
# two is below 1e-9 of r5r's area; anything else is a difference. The
# script stops with an error if any row is a difference.
#
# Usage (from the repository root; r5r needs a JDK 21):
#   JAVA_HOME=/path/to/jdk21 Rscript bench/compare/isochrone_parity.R [poa] [spo]
#
# The peabiru package must be installed (R_LIBS can point to a private
# library), and sf. Environment:
#   N_THREADS   threads for both packages (default 4)
#   R5R_XMX     JVM heap (default 6G)
#   SCENARIOS   comma-separated scenario name prefixes to run (default all)
#
# The networks are built from r5r's sample data (the OSM .pbf and the GTFS
# zips, never the elevation raster) in fresh directories under tempdir().
# POA's GTFS only has service in the afternoon: every call departs at
# 14:00 on 2019-05-13 (15:00 for one scenario); SPO's frequency-based GTFS is
# routed with a seed.

# peabiru returns tibbles by default; this script works with data.frames.
options(peabiru.output = "data.frame")
xmx <- Sys.getenv("R5R_XMX", "6G")
options(java.parameters = paste0("-Xmx", xmx))
suppressPackageStartupMessages({
  library(r5r)
  library(peabiru)
  library(sf)
})

args <- commandArgs(trailingOnly = TRUE)
cities <- if (length(args) > 0) args else c("poa", "spo")
stopifnot(all(cities %in% c("poa", "spo")))
n_threads <- as.integer(Sys.getenv("N_THREADS", "4"))
only <- Sys.getenv("SCENARIOS", "")
only <- if (nzchar(only)) strsplit(only, ",")[[1]] else character(0)
wanted <- function(name) length(only) == 0 || any(startsWith(name, only))

tz <- "America/Sao_Paulo"
departure <- as.POSIXct("2019-05-13 14:00:00", tz = tz)
work <- file.path(tempdir(), "isochrone_parity")
unlink(work, recursive = TRUE)
dir.create(work)

results <- list()
failures <- character(0)

as_plain <- function(x) {
  class(x) <- setdiff(class(x), "data.table")
  attr(x, ".internal.selfref") <- NULL
  x
}

# Per row: "identical", "tolerance" or "different", with the worst relative
# area and symmetric-difference figures over the rows within tolerance.
compare_rows <- function(pb, r5) {
  gcol <- attr(r5, "sf_column")
  n <- nrow(r5)
  status <- rep("different", n)
  if (nrow(pb) != n || !identical(names(pb), names(r5))) {
    return(list(status = status, area = NA, symdiff = NA, why = "rows or columns differ"))
  }
  cols <- setdiff(names(r5), gcol)
  same_cols <- vapply(cols, function(j) identical(pb[[j]], r5[[j]]), logical(1))
  if (!all(same_cols)) {
    return(list(status = status, area = NA, symdiff = NA,
                why = paste("columns differ:", paste(cols[!same_cols], collapse = ","))))
  }
  ga <- sf::st_geometry(pb)
  gb <- sf::st_geometry(r5)
  worst_area <- 0
  worst_sym <- 0
  for (i in seq_len(n)) {
    if (identical(unclass(ga[[i]]), unclass(gb[[i]])) && identical(class(ga[[i]]), class(gb[[i]]))) {
      status[i] <- "identical"
      next
    }
    if (sf::st_is_empty(ga[i]) && sf::st_is_empty(gb[i])) {
      status[i] <- "tolerance"
      next
    }
    a <- as.numeric(sf::st_area(ga[i]))
    b <- as.numeric(sf::st_area(gb[i]))
    if (b == 0) next
    rel <- abs(a - b) / b
    sym <- suppressMessages(as.numeric(sf::st_area(sf::st_sym_difference(ga[i], gb[i]))))
    sym <- if (length(sym) == 0) 0 else sum(sym) / b
    if (rel <= 1e-9 && sym <= 1e-9) {
      status[i] <- "tolerance"
      worst_area <- max(worst_area, rel)
      worst_sym <- max(worst_sym, sym)
    }
  }
  list(status = status, area = worst_area, symdiff = worst_sym, why = "")
}

compare <- function(city, name, detail, call_pb, call_r5) {
  label <- sprintf("%s/%s", city, name)
  t0 <- proc.time()[["elapsed"]]
  pb <- call_pb()
  t_pb <- proc.time()[["elapsed"]] - t0
  t0 <- proc.time()[["elapsed"]]
  r5 <- as_plain(call_r5())
  t_r5 <- proc.time()[["elapsed"]] - t0
  same <- identical(pb, r5)
  if (same) {
    st <- list(status = rep("identical", nrow(r5)), area = 0, symdiff = 0, why = "")
  } else {
    st <- compare_rows(pb, r5)
  }
  k <- table(factor(st$status, levels = c("identical", "tolerance", "different")))
  message(sprintf(
    "%-30s %-42s rows %7d  identical() %-5s  rows identical %7d  tolerance %5d  different %5d  peabiru %6.2f s  r5r %6.2f s",
    label, detail, nrow(r5), same, k[["identical"]], k[["tolerance"]], k[["different"]], t_pb, t_r5
  ))
  if (k[["different"]] > 0) {
    failures <<- c(failures, sprintf("%s: %d rows differ %s", label, k[["different"]], st$why))
  }
  results[[length(results) + 1]] <<- data.frame(
    city = city, scenario = name, detail = detail, rows = nrow(r5), identical_call = same,
    identical_rows = k[["identical"]], tolerance_rows = k[["tolerance"]],
    different_rows = k[["different"]], worst_area = st$area, worst_symdiff = st$symdiff,
    peabiru_s = round(t_pb, 3), r5r_s = round(t_r5, 3)
  )
  invisible(same)
}

copy_network <- function(city) {
  src <- system.file("extdata", city, package = "r5r")
  dir <- file.path(work, city)
  dir.create(dir)
  files <- c(file.path(src, paste0(city, "_osm.pbf")),
             list.files(src, pattern = "\\.zip$", full.names = TRUE))
  stopifnot(all(file.copy(files, dir)))
  dir
}

for (city in cities) {
  message(sprintf("== %s ==", city))
  points <- read.csv(
    system.file("extdata", city, paste0(city, "_hexgrid.csv"), package = "r5r"),
    colClasses = c(id = "character")
  )
  dir <- copy_network(city)
  net_pb <- peabiru::build_network(dir, n_threads = n_threads)
  net_r5 <- suppressMessages(r5r::build_network(dir))
  both <- function(...) {
    a <- list(...)
    list(
      pb = function() do.call(peabiru::isochrone, c(list(net_pb), a, n_threads = n_threads)),
      r5 = function() do.call(r5r::isochrone, c(list(net_r5), a, n_threads = n_threads,
                                                progress = FALSE))
    )
  }
  run <- function(name, detail, calls) {
    if (wanted(name)) compare(city, name, detail, calls$pb, calls$r5)
  }
  # origins spread over the city
  idx <- unique(round(seq(1, nrow(points), length.out = 6)))
  o <- points[idx, ]
  o2 <- points[idx[c(2, 5)], ]
  transit <- c("WALK", "TRANSIT")
  seed <- 42L

  for (m in c("WALK", "BICYCLE", "CAR")) {
    run(paste0("poly_", m), "cutoffs 0,10,20,30", both(
      origins = o, mode = m, cutoffs = c(0, 10, 20, 30), departure_datetime = departure))
    run(paste0("lines_", m), "cutoffs 0,10,20,30", both(
      origins = o, mode = m, cutoffs = c(0, 10, 20, 30), departure_datetime = departure,
      polygon_output = FALSE))
  }
  run("poly_TRANSIT", "default cutoffs, p50", both(
    origins = o, mode = transit, departure_datetime = departure, seed = seed))
  run("poly_TRANSIT_pcts", "cutoffs 0,20,45, p 10,50,90", both(
    origins = o, mode = "TRANSIT", cutoffs = c(0, 20, 45), percentiles = c(90, 10, 50),
    departure_datetime = departure, seed = seed))
  run("lines_TRANSIT", "default cutoffs", both(
    origins = o, mode = transit, departure_datetime = departure, seed = seed,
    polygon_output = FALSE))
  run("lines_TRANSIT_v2", "15:00, tw 3, mwt 10, rides 1, 0,7.5,25", both(
    origins = o, mode = "TRANSIT", cutoffs = c(25, 7.5, 0),
    departure_datetime = departure + 3600, time_window = 3L, max_walk_time = 10,
    max_rides = 1, draws_per_minute = 2L, seed = 7L, polygon_output = FALSE))
  run("poly_WALK_z9", "zoom 9, cutoffs 5,15,25,40", both(
    origins = o, mode = "WALK", cutoffs = c(5, 15, 25, 40), zoom = 9,
    departure_datetime = departure))
  run("poly_BICYCLE_z12", "zoom 12, lts 3, 14 km/h, 0,8,16", both(
    origins = o2, mode = "BICYCLE", cutoffs = c(0, 8, 16), zoom = 12, max_lts = 3,
    bike_speed = 14, departure_datetime = departure))
  run("poly_CAR_z11", "zoom 11, cutoffs 0,2.5,5,7.5,10", both(
    origins = o, mode = "CAR", cutoffs = c(0, 2.5, 5, 7.5, 10), zoom = 11,
    departure_datetime = departure))
  run("poly_WALK_mwt", "max_walk_time 12, 4.5 km/h, 0,10,20", both(
    origins = o, mode = "WALK", cutoffs = c(0, 10, 20), max_walk_time = 12,
    walk_speed = 4.5, departure_datetime = departure))
  run("poly_TRANSIT_z11", "zoom 11, tw 1, 0,10,25,40", both(
    origins = o2, mode = transit, cutoffs = c(0, 10, 25, 40), zoom = 11, time_window = 1L,
    departure_datetime = departure, seed = seed))
  run("lines_BICYCLE_v2", "lts 4, 10 km/h, 0,3,6,9", both(
    origins = o, mode = "BICYCLE", cutoffs = c(0, 3, 6, 9), max_lts = 4, bike_speed = 10,
    departure_datetime = departure, polygon_output = FALSE))
  # transit mode subsets, bicycle and car access and egress
  run("poly_BUS", "WALK+BUS, cutoffs 0,20,40", both(
    origins = o, mode = c("WALK", "BUS"), cutoffs = c(0, 20, 40),
    departure_datetime = departure, seed = seed))
  run("poly_RAIL_SUBWAY", "RAIL+SUBWAY, p 25,75", both(
    origins = o2, mode = c("RAIL", "SUBWAY"), percentiles = c(25, 75),
    departure_datetime = departure, seed = seed))
  run("poly_BICYCLE_TRANSIT", "BICYCLE+TRANSIT, mbt 15, lts 3", both(
    origins = o, mode = c("BICYCLE", "TRANSIT"), max_bike_time = 15, max_lts = 3,
    departure_datetime = departure, seed = seed))
  run("poly_CAR_TRANSIT_egress_BICYCLE", "CAR+TRANSIT, egress BICYCLE, z 10", both(
    origins = o2, mode = c("CAR", "TRANSIT"), mode_egress = "BICYCLE", zoom = 10,
    max_car_time = 10, max_bike_time = 10, departure_datetime = departure, seed = seed))
  run("poly_TRANSIT_egress_CAR", "TRANSIT, egress CAR, 0,15,30", both(
    origins = o2, mode = "TRANSIT", mode_egress = "CAR", max_car_time = 10,
    departure_datetime = departure, seed = seed))
  run("lines_CAR_BUS", "CAR+BUS, mct 10", both(
    origins = o, mode = c("CAR", "BUS"), max_car_time = 10,
    departure_datetime = departure, seed = seed, polygon_output = FALSE))
  run("lines_TRANSIT_egress_BICYCLE", "TRANSIT, egress BICYCLE", both(
    origins = o, mode = "TRANSIT", mode_egress = "BICYCLE",
    departure_datetime = departure, seed = seed, polygon_output = FALSE))
}

res <- do.call(rbind, results)
cat("\n")
print(res, row.names = FALSE)
cat(sprintf(
  "\nscenarios %d, identical() %d; rows %d: identical %d, within tolerance %d, different %d\n",
  nrow(res), sum(res$identical_call), sum(res$rows), sum(res$identical_rows),
  sum(res$tolerance_rows), sum(res$different_rows)
))
if (length(failures) > 0) {
  stop(paste(c("isochrones differ from r5r:", failures), collapse = "\n  "))
}
