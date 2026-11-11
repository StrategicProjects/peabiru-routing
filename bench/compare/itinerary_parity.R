# detailed_itineraries() of peabiru against r5r 2.5.0, with identical().
#
#   PEABIRU_R_LIB=<lib with peabiru> Rscript bench/compare/itinerary_parity.R [poa] [spo] [n_pairs]
#
# Needs r5r, a JDK 21 (JAVA_HOME) and r5r's sample data. Each city's .pbf
# and GTFS .zip files are copied into a fresh directory (r5r's extdata
# also holds a .tif and a cached network.dat). The pairs mix random points
# in the box of the sample points, sample points, equal points and points
# off the network. Exits with status 1 if any scenario differs.

args <- commandArgs(TRUE)
n_pairs <- suppressWarnings(as.integer(args[grepl("^[0-9]+$", args)]))
n_pairs <- if (length(n_pairs) == 1) n_pairs else 300L
cities <- intersect(args, c("poa", "spo"))
if (length(cities) == 0) cities <- c("poa", "spo")
lib <- Sys.getenv("PEABIRU_R_LIB")
if (nzchar(lib)) .libPaths(c(lib, .libPaths()))
if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx6G")
suppressMessages({
  library(r5r)
  library(peabiru)
})

plain <- function(x) {
  class(x) <- setdiff(class(x), "data.table")
  attr(x, ".internal.selfref") <- NULL
  x
}

pairs_for <- function(points, n, seed) {
  set.seed(seed)
  rnd <- function(k) data.frame(lat = runif(k, min(points$lat), max(points$lat)),
                                lon = runif(k, min(points$lon), max(points$lon)))
  k1 <- floor(n * 0.6)
  o <- rbind(rnd(k1), points[sample(nrow(points), n - k1, TRUE), c("lat", "lon")])
  d <- rbind(rnd(k1), points[sample(nrow(points), n - k1, TRUE), c("lat", "lon")])
  eq <- sample(n, max(1, n %/% 50))
  d[eq, ] <- o[eq, ]
  off <- sample(n, max(1, n %/% 100))
  o[off, "lat"] <- o[off, "lat"] + 0.5
  o$id <- paste0("o", seq_len(n))
  d$id <- paste0("d", seq_len(n))
  rownames(o) <- rownames(d) <- NULL
  list(o = o, d = d)
}

failed <- 0
for (city in cities) {
  src <- system.file("extdata", city, package = "r5r")
  dir <- file.path(tempdir(), paste0("itin-", city))
  unlink(dir, recursive = TRUE)
  dir.create(dir)
  file.copy(list.files(src, "\\.(pbf|zip)$", full.names = TRUE), dir)
  r5 <- suppressMessages(r5r::build_network(dir, verbose = FALSE))
  pb <- peabiru::build_network(dir)
  pts <- read.csv(file.path(src, paste0(city, "_hexgrid.csv")))
  dep <- as.POSIXct("2019-05-13 14:00:00")
  scenarios <- list(
    walk = list(mode = "WALK"),
    bike = list(mode = "BICYCLE", max_lts = 3),
    car = list(mode = "CAR"),
    walk_slow = list(mode = "WALK", walk_speed = 2.5, max_walk_time = 50),
    car_short = list(mode = "CAR", max_trip_duration = 20, drop_geometry = TRUE),
    bike_all = list(mode = "BICYCLE", shortest_path = FALSE),
    walk_osm = list(mode = "WALK", osm_link_ids = TRUE),
    bike_osm = list(mode = "BICYCLE", max_lts = 1, osm_link_ids = TRUE),
    car_osm = list(mode = "CAR", osm_link_ids = TRUE)
  )
  if (city == "poa") {
    scenarios <- c(scenarios, list(
      transit = list(mode = c("WALK", "TRANSIT")),
      bus_bike = list(mode = c("BICYCLE", "BUS"), max_bike_time = 15, max_rides = 2),
      car_egress = list(mode = c("WALK", "TRANSIT"), mode_egress = "CAR", time_window = 25,
                        drop_geometry = TRUE),
      rail = list(mode = c("WALK", "RAIL"), max_walk_time = 30),
      transit_osm = list(mode = c("WALK", "TRANSIT"), osm_link_ids = TRUE),
      car_access_osm = list(mode = c("CAR", "TRANSIT"), max_car_time = 20, osm_link_ids = TRUE),
      bike_egress_osm = list(mode = c("WALK", "BUS"), mode_egress = "BICYCLE",
                             osm_link_ids = TRUE)
    ))
    # scenarios: r5r's Porto Alegre files
    osm <- read.csv(file.path(src, "poa_osm_congestion.csv"))
    kmh <- transform(osm, max_speed = 15, speed_type = "km/h")
    poly <- readRDS(file.path(src, "poa_poly_congestion.rds"))
    lts <- read.csv(file.path(src, "poa_osm_lts.csv"))
    lines <- readRDS(file.path(src, "poa_ls_lts.rds"))
    scenarios <- c(scenarios, list(
      scen_car_osm = list(mode = "CAR", new_carspeeds = osm, carspeed_scale = 0.8),
      scen_car_kmh = list(mode = "CAR", new_carspeeds = kmh),
      scen_car_poly = list(mode = "CAR", new_carspeeds = poly, carspeed_scale = 1.1),
      scen_bike_lts = list(mode = "BICYCLE", new_lts = lts),
      scen_bike_lines = list(mode = "BICYCLE", new_lts = lines, max_lts = 3),
      scen_transit_bike = list(mode = c("BICYCLE", "TRANSIT"), new_lts = lines,
                               max_bike_time = 20),
      scen_transit_car = list(mode = c("CAR", "TRANSIT"), new_carspeeds = poly,
                              carspeed_scale = 0.6, max_car_time = 20, drop_geometry = TRUE),
      scen_egress_bike = list(mode = c("WALK", "TRANSIT"), mode_egress = "BICYCLE",
                              new_lts = lines, drop_geometry = TRUE),
      scen_car_ids = list(mode = "CAR", new_carspeeds = osm, carspeed_scale = 0.8,
                          osm_link_ids = TRUE),
      scen_bike_ids = list(mode = "BICYCLE", new_lts = lts, osm_link_ids = TRUE),
      scen_egress_car = list(mode = c("WALK", "TRANSIT"), mode_egress = "CAR",
                             new_carspeeds = poly, carspeed_scale = 0.7, osm_link_ids = TRUE)
    ))
    files_scen <- list(
      scen_car_files = list(mode = "CAR", new_carspeeds = kmh, osm_link_ids = TRUE),
      scen_egress_files = list(mode = c("WALK", "TRANSIT"), mode_egress = "CAR",
                               new_carspeeds = osm)
    )
  }
  k <- 0
  for (name in names(scenarios)) {
    k <- k + 1
    p <- pairs_for(pts, n_pairs, 100 * k + nchar(city))
    a <- c(list(origins = p$o, destinations = p$d, departure_datetime = dep), scenarios[[name]])
    r <- plain(suppressWarnings(do.call(r5r::detailed_itineraries,
                                        c(list(r5), a, progress = FALSE))))
    q <- suppressWarnings(do.call(peabiru::detailed_itineraries, c(list(pb), a)))
    ok <- identical(q, r)
    if (!ok) failed <- failed + 1
    message(sprintf("%s %-10s rows %5d  %s", city, name, nrow(r),
                    if (ok) "identical" else "DIFFERENT"))
    if (!ok) print(all.equal(q, r))
  }
  # output_dir: every file byte for byte, with and without the geometry
  # (ids with commas and spaces, which r5r allows in file names)
  files <- list(
    walk = list(mode = "WALK"),
    bike = list(mode = "BICYCLE", max_lts = 3),
    car = list(mode = "CAR"),
    car_nogeom = list(mode = "CAR", drop_geometry = TRUE)
  )
  if (city == "poa") {
    files <- c(files, list(
      transit = list(mode = c("WALK", "TRANSIT")),
      bus_bike = list(mode = c("BICYCLE", "BUS"), max_bike_time = 15),
      transit_osm = list(mode = c("WALK", "TRANSIT"), osm_link_ids = TRUE)
    ))
  }
  files$car_osm <- list(mode = "CAR", osm_link_ids = TRUE)
  if (city == "poa") files <- c(files, files_scen)
  for (name in names(files)) {
    k <- k + 1
    p <- pairs_for(pts, min(n_pairs, 200L), 100 * k + nchar(city))
    p$o$id <- paste0("o,", seq_len(nrow(p$o)), " x")
    p$d$id <- paste0("d ", seq_len(nrow(p$d)), ",y")
    dirs <- file.path(tempdir(), paste0("itin-files-", city, "-", name, c("-r5r", "-peabiru")))
    for (d in dirs) {
      unlink(d, recursive = TRUE)
      dir.create(d)
    }
    a <- c(list(origins = p$o, destinations = p$d, departure_datetime = dep), files[[name]])
    suppressWarnings(do.call(r5r::detailed_itineraries,
                             c(list(r5), a, progress = FALSE, output_dir = dirs[1])))
    suppressWarnings(do.call(peabiru::detailed_itineraries, c(list(pb), a, output_dir = dirs[2])))
    fr <- sort(list.files(dirs[1]))
    fp <- sort(list.files(dirs[2]))
    same <- vapply(intersect(fr, fp), function(f) {
      identical(readBin(file.path(dirs[1], f), "raw", 1e8), readBin(file.path(dirs[2], f), "raw", 1e8))
    }, logical(1))
    ok <- identical(fr, fp) && all(same)
    if (!ok) failed <- failed + 1
    message(sprintf("%s files %-8s %4d files, %4d identical  %s", city, name, length(fr),
                    sum(same), if (ok) "identical" else "DIFFERENT"))
  }
  r5r::stop_r5(r5)
}
if (failed > 0) quit(status = 1)
