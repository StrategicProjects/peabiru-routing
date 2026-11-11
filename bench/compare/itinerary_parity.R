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
    bike_all = list(mode = "BICYCLE", shortest_path = FALSE)
  )
  if (city == "poa") {
    scenarios <- c(scenarios, list(
      transit = list(mode = c("WALK", "TRANSIT")),
      bus_bike = list(mode = c("BICYCLE", "BUS"), max_bike_time = 15, max_rides = 2),
      car_egress = list(mode = c("WALK", "TRANSIT"), mode_egress = "CAR", time_window = 25,
                        drop_geometry = TRUE),
      rail = list(mode = c("WALK", "RAIL"), max_walk_time = 30)
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
                              new_lts = lines, drop_geometry = TRUE)
    ))
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
  r5r::stop_r5(r5)
}
if (failed > 0) quit(status = 1)
