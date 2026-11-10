# Reproducers from the pre-release review of the R package (2026-11-09).
# The first block fails on the code as reviewed: each test states what r5r
# 2.5.0 does with the same call. The second block passes and covers inputs
# that had no test (sf, data.table, empty tables).

## Known differences from r5r (fail until fixed) ---------------------------

test_that("arrival_datetime may be a POSIXlt, as in r5r", {
  # r5r computes arrival_datetime - difftime, which turns a POSIXlt into a
  # POSIXct, and routes; peabiru rejects the POSIXlt before that.
  network <- fixture_network()
  pts <- fixture_points()
  arrival <- as.POSIXct("2019-05-13 15:00:00", tz = "America/Sao_Paulo")
  ct <- arrival_travel_time_matrix(network, pts[1:2, ], pts, mode = "TRANSIT",
                                   arrival_datetime = arrival,
                                   max_trip_duration = 30, seed = 1, n_threads = 2)
  lt <- arrival_travel_time_matrix(network, pts[1:2, ], pts, mode = "TRANSIT",
                                   arrival_datetime = as.POSIXlt(arrival),
                                   max_trip_duration = 30, seed = 1, n_threads = 2)
  expect_identical(lt, ct)
})

test_that("an empty percentiles vector is an error for street modes too", {
  # r5r passes the empty vector to R5, which fails (ArrayIndexOutOfBounds in
  # TravelTimeMatrixComputer); peabiru returns a table without any travel
  # time column. Transit and accessibility already stop in the core.
  network <- fixture_network()
  pts <- fixture_points()
  expect_error(travel_time_matrix(network, pts, pts, mode = "WALK",
                                  percentiles = integer(0), n_threads = 2))
})

test_that("a file whose name merely contains 'tif' does not block the build", {
  # r5r's grep(".tif") is a regular expression, so 'identificacao.csv'
  # matches; r5r keeps elevation = "TOBLER" and R5 finds no raster. peabiru
  # stops with "Elevation ... is not supported" although there is no raster.
  dir <- tempfile("net")
  dir.create(dir)
  on.exit(unlink(dir, recursive = TRUE))
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), dir)
  writeLines("zone", file.path(dir, "identificacao_zonas.csv"))
  network <- build_network(dir, n_threads = 2)
  expect_s3_class(network, "peabiru_network")
  # the swap of origins and destinations stays off, as in r5r
  expect_true(network$has_tif)
})

test_that("mode may be a factor, as in r5r", {
  # r5r applies toupper(unique(mode)) before checking it
  network <- fixture_network()
  pts <- fixture_points()
  expect_identical(
    travel_time_matrix(network, pts, pts, mode = factor("WALK"), n_threads = 2),
    travel_time_matrix(network, pts, pts, mode = "WALK", n_threads = 2)
  )
})

## Inputs without a test (pass) --------------------------------------------

test_that("sf and data.table points give the data.frame result", {
  skip_if_not_installed("sf")
  network <- fixture_network()
  pts <- fixture_points()
  s <- sf::st_as_sf(pts, coords = c("lon", "lat"), crs = 4326)
  expect_identical(
    travel_time_matrix(network, s, s, mode = "CAR", n_threads = 2),
    travel_time_matrix(network, pts, pts, mode = "CAR", n_threads = 2)
  )
  # attribute columns named lon/lat are ignored in favour of the geometry
  s$lat <- 0
  expect_identical(
    accessibility(network, s, s, opportunities_colnames = "jobs", cutoffs = 20,
                  n_threads = 2),
    accessibility(network, pts, pts, opportunities_colnames = "jobs", cutoffs = 20,
                  n_threads = 2)
  )
  expect_error(
    travel_time_matrix(network, sf::st_transform(s, 3857), s, n_threads = 2),
    "WGS 84"
  )
  skip_if_not_installed("data.table")
  dt <- data.table::as.data.table(pts)
  expect_identical(
    travel_time_matrix(network, dt, dt, mode = "WALK", n_threads = 2),
    travel_time_matrix(network, pts, pts, mode = "WALK", n_threads = 2)
  )
})

test_that("empty tables give typed empty results", {
  network <- fixture_network()
  pts <- fixture_points()
  ttm <- travel_time_matrix(network, pts[0, ], pts, mode = "TRANSIT",
                            departure_datetime = departure, n_threads = 2)
  expect_identical(nrow(ttm), 0L)
  expect_type(ttm$travel_time_p50, "integer")
  e <- expanded_travel_time_matrix(network, pts[0, ], pts, mode = "TRANSIT",
                                   departure_datetime = departure, n_threads = 2)
  expect_identical(nrow(e), 0L)
  expect_type(e$total_time, "double")
  a <- accessibility(network, pts, pts[0, ], opportunities_colnames = "jobs",
                     cutoffs = 30, n_threads = 2)
  expect_identical(nrow(a), nrow(pts))
  expect_true(all(a$accessibility == 0))
})

test_that("an origin with missing coordinates has zero accessibility", {
  network <- fixture_network()
  pts <- fixture_points()
  na <- pts
  na$lat[1] <- NA
  expect_warning(
    a <- accessibility(network, na, pts, opportunities_colnames = "jobs",
                       cutoffs = 30, n_threads = 2),
    "missing lon/lat"
  )
  expect_identical(a$accessibility[1], 0)
})
