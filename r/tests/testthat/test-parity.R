# Parity with r5r on its own sample data (Sao Paulo: OSM extract and the
# SPTrans frequency-based GTFS). Needs r5r, a JDK 21 and the R5 jar, so it is
# skipped on CRAN and wherever r5r cannot start. bench/compare/r_parity.R
# runs the full comparison on both sample cities.

r5r_network <- local({
  net <- NULL
  function() {
    if (!is.null(net)) return(net)
    skip_on_cran()
    skip_if_not_installed("r5r")
    src <- system.file("extdata/spo", package = "r5r")
    skip_if(!file.exists(file.path(src, "spo_osm.pbf")), "r5r sample data not found")
    dir <- file.path(tempdir(), "peabiru-parity-spo")
    unlink(dir, recursive = TRUE)
    dir.create(dir)
    file.copy(file.path(src, c("spo_osm.pbf", "spo.zip")), dir)
    if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx2G")
    r5 <- tryCatch(
      suppressMessages(r5r::build_network(dir)),
      error = function(e) NULL
    )
    skip_if(is.null(r5), "r5r could not build a network (Java 21 or the R5 jar missing?)")
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
  # isoband's ring order is that of libc++, the C++ library of macOS
  skip_on_os(c("windows", "linux", "solaris"))
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
