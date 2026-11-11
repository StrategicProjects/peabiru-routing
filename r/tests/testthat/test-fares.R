# Fare structures, fares in the routing functions and pareto_frontier().
# The parity tests at the end compare with r5r and skip without it (see
# test-parity.R); bench/compare/fares_parity.R runs the full comparison.

test_that("setup_fare_structure() builds r5r's object", {
  net <- fixture_network()
  fs <- setup_fare_structure(net, base_fare = 4.4)
  expect_named(fs, c("max_discounted_transfers", "transfer_time_allowance", "fare_cap",
                     "fares_per_type", "fares_per_transfer", "fares_per_route",
                     "debug_settings"))
  expect_identical(fs$max_discounted_transfers, 1L)
  expect_identical(fs$transfer_time_allowance, 120L)
  expect_identical(fs$fare_cap, Inf)
  expect_setequal(fs$fares_per_type$type, unique(fs$fares_per_route$fare_type))
  expect_true(all(fs$fares_per_type$fare == 4.4))
  expect_equal(nrow(fs$fares_per_transfer), nrow(fs$fares_per_type)^2)
  expect_identical(fs$debug_settings, list(output_file = "", trip_info = "MODE"))
  expect_identical(fs$fares_per_route$mode, fs$fares_per_route$fare_type)
  generic <- setup_fare_structure(net, 2, by = "generic", debug_path = "x.csv")
  expect_identical(generic$fares_per_type$type, "GENERIC")
  expect_identical(generic$debug_settings, list(output_file = "x.csv", trip_info = "ROUTE"))
  expect_error(setup_fare_structure(net, -1), ">= 0")
  expect_error(setup_fare_structure(net, 1, by = "ROUTE"), "subset")
  expect_error(setup_fare_structure(net, 1, debug_info = "MODE"), "debug_path")
})

test_that("write_fare_structure() and read_fare_structure() round-trip", {
  net <- fixture_network()
  fs <- setup_fare_structure(net, base_fare = 10 / 3, by = "AGENCY_NAME")
  fs$fares_per_route$route_long_name[1] <- "with, a comma and \"quotes\""
  fs$fare_cap <- 7.5
  path <- tempfile(fileext = ".zip")
  expect_identical(write_fare_structure(fs, path), path)
  back <- read_fare_structure(path)
  # fwrite keeps 15 significant digits
  fs$fares_per_type$fare <- signif(fs$fares_per_type$fare, 15)
  fs$fares_per_transfer$fare <- signif(fs$fares_per_transfer$fare, 15)
  fs$fares_per_route$route_fare <- signif(fs$fares_per_route$route_fare, 15)
  expect_identical(back, fs)
  files <- utils::unzip(path, list = TRUE)$Name
  expect_identical(files, c("global_settings.csv", "fares_per_type.csv",
                            "fares_per_transfer.csv", "fares_per_route.csv",
                            "debug_settings.csv"))
  expect_error(write_fare_structure(fs, "x.csv"), "zip")
  expect_error(read_fare_structure(tempfile(fileext = ".zip")), "does not exist")
})

test_that("fare structures are checked as r5r checks them", {
  net <- fixture_network()
  pts <- fixture_points()
  fs <- setup_fare_structure(net, base_fare = 4.4)
  ttm <- function(f, ...) {
    travel_time_matrix(net, pts, pts, mode = c("WALK", "TRANSIT"),
                       departure_datetime = departure, fare_structure = f,
                       seed = 1, n_threads = 2, ...)
  }
  missing_route <- fs
  missing_route$fares_per_route <- missing_route$fares_per_route[-1, ]
  expect_error(ttm(missing_route), "no row for agency_id")
  na_fare <- fs
  na_fare$fares_per_type$fare <- NA_real_
  expect_error(ttm(na_fare), "fares must be finite")
  unknown_type <- fs
  unknown_type$fares_per_route$fare_type[1] <- "NOPE"
  expect_error(ttm(unknown_type), "not a type")
  expect_error(ttm(fs[-1]), "names")
  extra <- fs
  extra$fares_per_route$note <- "x"
  expect_error(ttm(extra), "cannot read")
  expect_error(ttm(list(type = "boston")), "not supported")
  expect_error(ttm(fs, time_window = 40), "200 Monte Carlo draws")
  expect_error(ttm(fs, max_fare = -1), ">= 0")
  # the structure is read in even for a street-only trip, as in r5r
  expect_error(travel_time_matrix(net, pts, pts, mode = "WALK", fare_structure = missing_route),
               "no row for agency_id")
})

test_that("fares limit public transport trips", {
  net <- fixture_network()
  pts <- fixture_points()
  fs <- setup_fare_structure(net, base_fare = 4.4)
  run <- function(...) {
    travel_time_matrix(net, pts, pts, mode = c("WALK", "TRANSIT"),
                       departure_datetime = departure, seed = 42, n_threads = 2,
                       draws_per_minute = 1, ...)
  }
  free <- run(fare_structure = fs)
  none <- run(fare_structure = fs, max_fare = 0)
  walk <- travel_time_matrix(net, pts, pts, mode = "WALK", n_threads = 2)
  key <- function(x) paste(x$from_id, x$to_id)
  # with no money, the trips are the walks
  expect_setequal(key(none), key(walk[walk$travel_time_p50 <= 120, ]))
  m <- merge(free, none, by = c("from_id", "to_id"), all.x = TRUE)
  expect_true(all(is.na(m$travel_time_p50.y) | m$travel_time_p50.x <= m$travel_time_p50.y))
})

test_that("pareto_frontier() lists the travel time at each fare that changes it", {
  net <- fixture_network()
  pts <- fixture_points()
  fs <- setup_fare_structure(net, base_fare = 4.4)
  pf <- pareto_frontier(net, pts[1:3, ], pts, departure_datetime = departure,
                        fare_structure = fs, fare_cutoffs = c(0, 4.4, 8.8),
                        seed = 42, n_threads = 2)
  expect_named(pf, c("from_id", "to_id", "percentile", "travel_time", "monetary_cost"))
  expect_true(all(pf$monetary_cost %in% as.double(c(0, 4.4, 8.8)) |
                    abs(pf$monetary_cost - 4.4) < 1e-6 | abs(pf$monetary_cost - 8.8) < 1e-6))
  # a later cutoff appears only when the time changes
  by_pair <- split(pf$travel_time, paste(pf$from_id, pf$to_id))
  expect_true(all(vapply(by_pair, function(t) !anyDuplicated(t), TRUE)))
  expect_error(pareto_frontier(net, pts, pts, departure_datetime = departure,
                               fare_cutoffs = 1), "required")
  expect_error(pareto_frontier(net, pts, pts, departure_datetime = departure,
                               fare_structure = fs), "required")
  expect_error(pareto_frontier(net, pts, pts, departure_datetime = departure,
                               fare_structure = fs, fare_cutoffs = c(1, 1)), "duplicated")
})

test_that("detailed itineraries with fares carry fare columns", {
  net <- fixture_network()
  pts <- fixture_points()
  fs <- setup_fare_structure(net, base_fare = 4.4)
  di <- detailed_itineraries(net, pts[1:3, ], pts[4:6, ], mode = "WALK",
                             departure_datetime = departure, fare_structure = fs,
                             drop_geometry = TRUE, n_threads = 2)
  expect_true(all(c("total_fare", "cumulative_fare") %in% names(di)))
  expect_true(all(di$total_fare == 0))
  expect_error(detailed_itineraries(net, pts[1, ], pts[2, ], mode = "WALK",
                                    departure_datetime = departure, fare_structure = fs,
                                    shortest_path = FALSE, suboptimal_minutes = 3),
               "calculating fares")
})

# ---- parity with r5r (skipped without r5r and Java) -------------------------

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
