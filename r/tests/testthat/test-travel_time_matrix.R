test_that("street matrices have r5r's columns and types", {
  network <- fixture_network()
  pts <- fixture_points()
  for (m in c("WALK", "BICYCLE", "CAR")) {
    ttm <- travel_time_matrix(network, pts, pts, mode = m, n_threads = 2)
    expect_identical(class(ttm), "data.frame")
    expect_identical(names(ttm), c("from_id", "to_id", "travel_time_p50"))
    expect_type(ttm$from_id, "character")
    expect_type(ttm$travel_time_p50, "integer")
    expect_true(all(ttm$travel_time_p50 <= 120L))
    # origin-major, in input order
    expect_identical(unique(ttm$from_id), pts$id[pts$id %in% ttm$from_id])
  }
})

test_that("a street mode reports its one time for every percentile", {
  network <- fixture_network()
  pts <- fixture_points()
  ttm <- travel_time_matrix(network, pts, pts, mode = "BICYCLE",
                            percentiles = c(75, 10), n_threads = 2)
  expect_identical(names(ttm), c("from_id", "to_id", "travel_time_p10", "travel_time_p75"))
  expect_identical(ttm$travel_time_p10, ttm$travel_time_p75)
})

test_that("the trip limit and the mode limit drop pairs", {
  network <- fixture_network()
  pts <- fixture_points()
  full <- travel_time_matrix(network, pts, pts, mode = "WALK", n_threads = 2)
  short <- travel_time_matrix(network, pts, pts, mode = "WALK",
                              max_trip_duration = 10, n_threads = 2)
  walk10 <- travel_time_matrix(network, pts, pts, mode = "WALK",
                               max_walk_time = 10, n_threads = 2)
  expect_true(all(short$travel_time_p50 <= 10L))
  expect_identical(short, walk10)
  expect_lt(nrow(short), nrow(full))
})

test_that("WALK with more origins than destinations keeps the caller's columns", {
  network <- fixture_network()
  pts <- fixture_points()
  # r5r routes from the destinations here, then swaps the ids back
  ttm <- travel_time_matrix(network, pts, pts[1:3, ], mode = "WALK", n_threads = 2)
  expect_identical(names(ttm), c("from_id", "to_id", "travel_time_p50"))
  expect_true(all(ttm$to_id %in% pts$id[1:3]))
  expect_setequal(unique(ttm$from_id), pts$id)
  # rows follow the routed origins, i.e. the destinations
  expect_identical(unique(ttm$to_id), pts$id[1:3])
})

test_that("transit matrices have one column per percentile, ascending", {
  network <- fixture_network()
  pts <- fixture_points()
  ttm <- travel_time_matrix(network, pts, pts, mode = c("WALK", "TRANSIT"),
                            departure_datetime = departure,
                            percentiles = c(95, 5, 50), seed = 1, n_threads = 2)
  expect_identical(names(ttm), c("from_id", "to_id", "travel_time_p05",
                                 "travel_time_p50", "travel_time_p95"))
  expect_true(all(ttm$travel_time_p05 <= ttm$travel_time_p50, na.rm = TRUE))
  expect_true(all(ttm$travel_time_p50 <= ttm$travel_time_p95, na.rm = TRUE))
  expect_false(anyNA(ttm$travel_time_p05))
  again <- travel_time_matrix(network, pts, pts, mode = "TRANSIT",
                              departure_datetime = departure,
                              percentiles = c(5, 50, 95), seed = 1, n_threads = 1)
  expect_identical(ttm, again)
})

test_that("points are checked as r5r checks them", {
  network <- fixture_network()
  pts <- fixture_points()
  expect_error(travel_time_matrix(network, as.list(pts), pts), "must be either a 'data.frame'")
  expect_error(travel_time_matrix(network, pts[, c("id", "lon")], pts), "missing elements \\{'lat'\\}")
  expect_error(travel_time_matrix(network, rbind(pts, pts[1, ]), pts, n_threads = 2),
               "must not contain duplicated ids")
  num <- pts
  num$id <- seq_len(nrow(num))
  expect_warning(travel_time_matrix(network, num, pts, n_threads = 2),
                 "forcefully cast to character")
  na <- pts
  na$lon[2] <- NA
  expect_warning(res <- travel_time_matrix(network, na, pts, mode = "CAR", n_threads = 2),
                 "missing lon/lat")
  expect_false(pts$id[2] %in% res$from_id)
})

test_that("arguments are checked as r5r checks them", {
  network <- fixture_network()
  pts <- fixture_points()
  ttm <- function(...) travel_time_matrix(network, pts, pts, n_threads = 2, ...)
  expect_error(ttm(mode = "TRANSIT"), "must pass a 'departure_datetime'")
  expect_error(ttm(mode = c("WALK", "CAR")), "Please use only 1 of")
  expect_error(ttm(mode = "CAR_PARK"), "unsupported by r5r")
  expect_error(ttm(mode = "PLANE"), "Must be a subset")
  expect_error(ttm(percentiles = 1:6), "length <= 5")
  expect_error(ttm(percentiles = 100), "<= 99")
  expect_error(ttm(time_window = 0), "Must be >= 1")
  expect_error(ttm(max_trip_duration = 1.5), "count")
  expect_error(ttm(max_walk_time = 0), ">= 1")
  expect_error(ttm(walk_speed = 0), "greater than 0")
  expect_error(ttm(max_lts = 5), "between 1 and 4")
  expect_error(ttm(seed = 1.5), "integerish")
  expect_error(travel_time_matrix(network, pts, pts, n_threads = 0), ">= 1")
  expect_error(ttm(max_fare = 5), "requires a 'fare_structure'")
  expect_error(ttm(departure_datetime = "2019-05-13 14:00"), "POSIXct")
  expect_error(travel_time_matrix(list(), pts, pts), "peabiru_network")
})

test_that("what the core does not support is an error, never ignored", {
  network <- fixture_network()
  pts <- fixture_points()
  ttm <- function(...) {
    travel_time_matrix(network, pts, pts, departure_datetime = departure,
                       n_threads = 2, ...)
  }
  expect_error(ttm(mode = c("BICYCLE", "TRANSIT")), "BICYCLE access is not supported")
  expect_error(ttm(mode = c("CAR", "TRANSIT")), "CAR access is not supported")
  expect_error(ttm(mode = "TRANSIT", mode_egress = "BICYCLE"), "BICYCLE egress is not supported")
  expect_error(ttm(mode = c("WALK", "BUS")), "specific transit modes")
  expect_error(ttm(fare_structure = list()), "fare_structure.*not supported")
  expect_error(ttm(carspeed_scale = 0.5), "car speeds.*not supported")
  expect_error(ttm(new_carspeeds = data.frame(osm_id = 1, max_speed = 1)),
               "car speeds.*not supported")
  expect_error(ttm(new_lts = data.frame(osm_id = 1, lts = 1L)), "LTS.*not supported")
  expect_error(ttm(output_dir = tempdir()), "output_dir.*not supported")
  # TRANSIT with other transit modes is all transit modes, as in r5r
  expect_identical(
    ttm(mode = c("WALK", "TRANSIT", "BUS"), seed = 1),
    ttm(mode = "TRANSIT", seed = 1)
  )
})
