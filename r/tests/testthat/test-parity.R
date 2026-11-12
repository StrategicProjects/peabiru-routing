# Parity with r5r on its own sample data (Sao Paulo: OSM extract and the
# SPTrans frequency-based GTFS). Needs r5r, a JDK 21 and the R5 jar, so it is
# skipped on CRAN and wherever r5r cannot start. bench/compare/r_parity.R
# runs the full comparison on both sample cities.

r5r_network <- local({
  net <- NULL
  function() {
    if (!is.null(net)) return(net)
    src <- r5r_sample("spo", packages = "sf") # helper-r5r.R
    dir <- file.path(tempdir(), "peabiru-parity-spo")
    unlink(dir, recursive = TRUE)
    dir.create(dir)
    file.copy(file.path(src, c("spo_osm.pbf", "spo.zip")), dir)
    r5 <- r5r_network_or_skip(dir)
    pb <- build_network(dir, n_threads = 2)
    points <- read.csv(file.path(src, "spo_hexgrid.csv"), colClasses = c(id = "character"))
    net <<- list(r5 = r5, pb = pb, points = points)
    net
  }
})

expect_same_as_r5r <- function(fun, ...) {
  n <- r5r_network()
  pb <- getExportedValue("peabiru", fun)(n$pb, ..., n_threads = 2)
  r5 <- getExportedValue("r5r", fun)(n$r5, ..., n_threads = 2, progress = FALSE)
  expect_identical(pb, as.data.frame(r5))
}

test_that("street travel time matrices equal r5r's", {
  pts <- r5r_network()$points[1:80, ]
  for (m in c("WALK", "BICYCLE", "CAR")) {
    expect_same_as_r5r("travel_time_matrix", origins = pts, destinations = pts,
                       mode = m, departure_datetime = departure)
  }
  # r5r swaps origins and destinations for WALK
  expect_same_as_r5r("travel_time_matrix", origins = pts, destinations = pts[1:10, ],
                     mode = "WALK", departure_datetime = departure)
})

test_that("transit travel time matrices equal r5r's", {
  pts <- r5r_network()$points[1:80, ]
  expect_same_as_r5r("travel_time_matrix", origins = pts, destinations = pts,
                     mode = c("WALK", "TRANSIT"), departure_datetime = departure,
                     percentiles = c(5, 25, 50, 75, 95), seed = 42)
  expect_same_as_r5r("travel_time_matrix", origins = pts, destinations = pts,
                     mode = c("WALK", "TRANSIT"), departure_datetime = departure,
                     time_window = 1, max_walk_time = 15, seed = 42)
})

test_that("accessibility equals r5r's, bit for bit", {
  pts <- r5r_network()$points
  o <- pts[1:40, ]
  expect_same_as_r5r("accessibility", origins = o, destinations = pts,
                     opportunities_colnames = c("jobs", "schools"),
                     mode = c("WALK", "TRANSIT"), departure_datetime = departure,
                     cutoffs = c(20, 40), percentiles = c(5, 50), seed = 42)
  expect_same_as_r5r("accessibility", origins = o, destinations = pts,
                     opportunities_colnames = "jobs",
                     mode = c("WALK", "TRANSIT"), departure_datetime = departure,
                     decay_function = "logistic", decay_value = 8,
                     cutoffs = c(20, 40), max_trip_duration = 60, seed = 42)
  expect_same_as_r5r("accessibility", origins = o, destinations = pts,
                     opportunities_colnames = "jobs", mode = "BICYCLE",
                     decay_function = "fixed_exponential", decay_value = 0.002)
})

test_that("expanded and arrival matrices equal r5r's", {
  pts <- r5r_network()$points
  expect_same_as_r5r("expanded_travel_time_matrix", origins = pts[1:5, ],
                     destinations = pts, mode = c("WALK", "TRANSIT"),
                     departure_datetime = departure, time_window = 3,
                     breakdown = TRUE, seed = 42)
  expect_same_as_r5r("arrival_travel_time_matrix", origins = pts[1:5, ],
                     destinations = pts, mode = c("WALK", "TRANSIT"),
                     arrival_datetime = departure + 3600, max_trip_duration = 45,
                     seed = 42)
})

# r5r's isochrone() returns a data.table for polygons; the rest is identical,
# geometry included (the same polygons go through sf::st_make_valid()).
expect_same_isochrone <- function(...) {
  skip_if_not_installed("sf")
  n <- r5r_network()
  pb <- isochrone(n$pb, ..., n_threads = 2)
  r5 <- r5r::isochrone(n$r5, ..., n_threads = 2, progress = FALSE)
  class(r5) <- setdiff(class(r5), "data.table")
  attr(r5, ".internal.selfref") <- NULL
  expect_identical(pb, r5)
}

test_that("isochrones equal r5r's", {
  # isoband's ring order is that of the C++ library it was built with
  # (libc++ on macOS, libstdc++ on Linux and Windows); the core follows the
  # platform's
  o <- r5r_network()$points[c(1, 50, 120), ]
  expect_same_isochrone(origins = o, mode = "WALK", cutoffs = c(0, 10, 20),
                        departure_datetime = departure)
  expect_same_isochrone(origins = o, mode = "TRANSIT", cutoffs = c(0, 15, 30),
                        departure_datetime = departure, percentiles = c(25, 50),
                        seed = 42)
  expect_same_isochrone(origins = o, mode = "CAR", cutoffs = c(0, 5, 10),
                        departure_datetime = departure, polygon_output = FALSE)
  expect_same_isochrone(origins = o, mode = "TRANSIT", cutoffs = c(0, 15, 30),
                        departure_datetime = departure, polygon_output = FALSE,
                        seed = 42)
})

test_that("scenarios equal r5r's", {
  skip_if_not_installed("sf")
  n <- r5r_network()
  pts <- n$points[1:80, ]
  edges <- street_network_to_sf(n$pb)$edges
  ways <- unique(edges$osm_id)
  set.seed(3)
  osm <- data.frame(osm_id = sample(ways, 300), max_speed = sample(c(0, 0.4, 1.7), 300, TRUE),
                    speed_type = "scale")
  kmh <- data.frame(osm_id = sample(ways, 300), max_speed = runif(300, 5, 80),
                    speed_type = "km/h")
  lts <- data.frame(osm_id = sample(ways, 300), lts = sample(1:4, 300, TRUE))
  box <- sf::st_bbox(sf::st_as_sf(pts[1:40, ], coords = c("lon", "lat"), crs = 4326))
  poly <- sf::st_sf(poly_id = "a", scale = 0.6, priority = 1L, geometry = sf::st_as_sfc(box))
  lines <- sf::st_sf(line_id = as.character(1:200), lts = sample(1:4, 200, TRUE),
                     priority = 1L, geometry = sf::st_geometry(edges)[1:200])
  for (s in list(list(mode = "CAR", new_carspeeds = osm, carspeed_scale = 0.8),
                 list(mode = "CAR", new_carspeeds = kmh),
                 list(mode = "CAR", new_carspeeds = poly, carspeed_scale = 1.2),
                 list(mode = "BICYCLE", new_lts = lts),
                 list(mode = "BICYCLE", new_lts = lines, max_lts = 3),
                 list(mode = c("BICYCLE", "TRANSIT"), new_lts = lts, seed = 42,
                      max_bike_time = 20),
                 list(mode = c("CAR", "TRANSIT"), carspeed_scale = 0.5, seed = 42,
                      max_car_time = 15))) {
    suppressWarnings(do.call(expect_same_as_r5r,
                             c(list("travel_time_matrix", origins = pts, destinations = pts,
                                    departure_datetime = departure), s)))
  }
  # detailed itineraries on the scenario's network: osm_link_ids and
  # output_dir with the geometry
  set.seed(4)
  o <- pts[sample(nrow(pts), 30), ]
  d <- pts[sample(nrow(pts), 30), ]
  for (s in list(list(mode = "CAR", new_carspeeds = osm, carspeed_scale = 0.8, osm_link_ids = TRUE),
                 list(mode = "BICYCLE", new_lts = lines, osm_link_ids = TRUE))) {
    args <- c(list(origins = o, destinations = d, departure_datetime = departure), s)
    pb <- suppressWarnings(do.call(detailed_itineraries, c(list(n$pb), args, n_threads = 2)))
    r5 <- suppressWarnings(do.call(r5r::detailed_itineraries,
                                   c(list(n$r5), args, n_threads = 2, progress = FALSE)))
    class(r5) <- setdiff(class(r5), "data.table")
    attr(r5, ".internal.selfref") <- NULL
    expect_identical(pb, r5)
  }
  dirs <- c(tempfile(), tempfile())
  for (x in dirs) dir.create(x)
  args <- list(origins = o, destinations = d, mode = "CAR", new_carspeeds = kmh,
               departure_datetime = departure)
  suppressWarnings(do.call(detailed_itineraries, c(list(n$pb), args, output_dir = dirs[1])))
  suppressWarnings(do.call(r5r::detailed_itineraries,
                           c(list(n$r5), args, output_dir = dirs[2], progress = FALSE)))
  files <- list.files(dirs[2])
  expect_identical(list.files(dirs[1]), files)
  for (f in files) {
    expect_identical(readBin(file.path(dirs[1], f), "raw", 1e7),
                     readBin(file.path(dirs[2], f), "raw", 1e7))
  }
})

# r5r's detailed_itineraries() returns an sf data.table; the rest is
# identical, geometry included. bench/compare/itinerary_parity.R covers
# public transport (Porto Alegre).
test_that("detailed itineraries equal r5r's", {
  skip_if_not_installed("sf")
  n <- r5r_network()
  pts <- n$points
  set.seed(7)
  o <- pts[sample(nrow(pts), 60), ]
  d <- pts[sample(nrow(pts), 60), ]
  d[1:3, ] <- o[1:3, ]
  for (a in list(list(mode = "WALK"), list(mode = "BICYCLE", max_lts = 4),
                 list(mode = "CAR", drop_geometry = TRUE),
                 list(mode = "WALK", walk_speed = 5, max_walk_time = 20),
                 list(mode = "CAR", osm_link_ids = TRUE),
                 list(mode = "BICYCLE", osm_link_ids = TRUE))) {
    args <- c(list(origins = o, destinations = d, departure_datetime = departure), a)
    pb <- do.call(detailed_itineraries, c(list(n$pb), args, n_threads = 2))
    r5 <- do.call(r5r::detailed_itineraries, c(list(n$r5), args, n_threads = 2, progress = FALSE))
    class(r5) <- setdiff(class(r5), "data.table")
    attr(r5, ".internal.selfref") <- NULL
    expect_identical(pb, r5)
  }
  # output_dir: the same files, byte for byte, with the geometry
  dirs <- c(tempfile(), tempfile())
  for (x in dirs) dir.create(x)
  args <- list(origins = o[1:20, ], destinations = d[1:20, ], mode = "BICYCLE",
               departure_datetime = departure)
  do.call(detailed_itineraries, c(list(n$pb), args, output_dir = dirs[1]))
  do.call(r5r::detailed_itineraries, c(list(n$r5), args, output_dir = dirs[2], progress = FALSE))
  files <- list.files(dirs[2])
  expect_identical(list.files(dirs[1]), files)
  for (f in files) {
    expect_identical(readBin(file.path(dirs[1], f), "raw", 1e7),
                     readBin(file.path(dirs[2], f), "raw", 1e7))
  }
  # both refuse public transport on a frequency-based GTFS
  tr <- list(origins = o[1, ], destinations = d[1, ], mode = c("WALK", "TRANSIT"),
             departure_datetime = departure)
  expect_error(suppressMessages(do.call(detailed_itineraries, c(list(n$pb), tr))), "frequencies")
  expect_error(suppressMessages(do.call(r5r::detailed_itineraries, c(list(n$r5), tr))), "frequencies")
})

# ---- fares (the other fare tests are in test-fares.R) ----------------------

test_that("fare structures equal r5r's, and each reads the other's files", {
  n <- r5r_network()
  for (by in c("MODE", "AGENCY_ID", "AGENCY_NAME", "GENERIC")) {
    pb <- setup_fare_structure(n$pb, 4.8, by = by)
    r5 <- r5r::setup_fare_structure(n$r5, 4.8, by = by)
    r5[4:6] <- lapply(r5[4:6], as.data.frame)
    expect_identical(pb, r5)
  }
  fs <- r5r::setup_fare_structure(n$r5, 3.75)
  zr <- tempfile(fileext = ".zip")
  zp <- tempfile(fileext = ".zip")
  r5r::write_fare_structure(fs, zr)
  write_fare_structure(read_fare_structure(zr), zp)
  a <- r5r::read_fare_structure(zp)
  b <- r5r::read_fare_structure(zr)
  expect_identical(lapply(a, function(x) if (is.data.frame(x)) as.data.frame(x) else x),
                   lapply(b, function(x) if (is.data.frame(x)) as.data.frame(x) else x))
})

test_that("travel times with fares and the Pareto frontier equal r5r's", {
  n <- r5r_network()
  pts <- n$points[1:40, ]
  fs <- r5r::setup_fare_structure(n$r5, 4.4)
  fs$fares_per_transfer$fare <- 6.6
  expect_same_as_r5r("travel_time_matrix", origins = pts, destinations = pts,
                     mode = c("WALK", "TRANSIT"), departure_datetime = departure,
                     fare_structure = fs, max_fare = 6.6, draws_per_minute = 2,
                     seed = 42)
  expect_same_as_r5r("pareto_frontier", origins = pts[1:10, ], destinations = pts,
                     mode = c("WALK", "TRANSIT"), departure_datetime = departure,
                     fare_structure = fs, fare_cutoffs = c(0, 4.4, 6.6, 8.8), seed = 42)
})
