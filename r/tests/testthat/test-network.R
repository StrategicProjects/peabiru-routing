test_that("a network is built from a directory with a .pbf and GTFS", {
  network <- fixture_network()
  expect_s3_class(network, "peabiru_network")
  expect_identical(network$elevation, "NONE")
  out <- capture.output(print(network))
  expect_match(out[1], "<peabiru_network>")
  expect_true(any(grepl("1 feed\\(s\\) \\(spo_gtfs\\)", out)))
})

test_that("a street-only network has no transit", {
  dir <- tempfile("net")
  dir.create(dir)
  on.exit(unlink(dir, recursive = TRUE))
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), dir)
  network <- build_network(dir, n_threads = 2)
  expect_true(any(grepl("transit:   none", capture.output(print(network)))))
  pts <- fixture_points()
  expect_error(
    travel_time_matrix(network, pts, pts, mode = "TRANSIT",
                       departure_datetime = departure, n_threads = 2),
    "no transit services available"
  )
  ttm <- travel_time_matrix(network, pts, pts, mode = "CAR", n_threads = 2)
  expect_gt(nrow(ttm), 0)
})

test_that("bad data paths are rejected", {
  expect_error(build_network(tempfile("missing")), "does not exist")
  empty <- tempfile("empty")
  dir.create(empty)
  on.exit(unlink(empty, recursive = TRUE))
  expect_error(build_network(empty), "OSM .pbf file is required")
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), file.path(empty, "a.pbf"))
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), file.path(empty, "b.pbf"))
  expect_error(build_network(empty), "more than one .pbf")
  expect_error(build_network(fixture_path(), elevation = "FLAT"), "Invalid value")
})

test_that("elevation needs elevation = 'NONE' when there is a raster", {
  dir <- tempfile("net")
  dir.create(dir)
  on.exit(unlink(dir, recursive = TRUE))
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), dir)
  writeBin(raw(10), file.path(dir, "elevation.tif"))
  expect_error(build_network(dir),
               "Elevation is not supported by peabiru yet, and .* has the raster 'elevation.tif'")
  network <- build_network(dir, elevation = "NONE", n_threads = 2)
  expect_true(network$has_tif)
})

test_that("a file whose name merely contains 'tif' does not block the build", {
  # r5r's grep(".tif") is a regular expression, so 'identificacao.csv'
  # matches; r5r keeps elevation = "TOBLER" and R5 finds no raster.
  dir <- tempfile("net")
  dir.create(dir)
  on.exit(unlink(dir, recursive = TRUE))
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), dir)
  writeLines("zone", file.path(dir, "identificacao_zonas.csv"))
  network <- build_network(dir, n_threads = 2)
  expect_s3_class(network, "peabiru_network")
  expect_identical(network$elevation, "NONE")
  # the swap of origins and destinations stays off, as in r5r
  expect_true(network$has_tif)
})

test_that("a network that was saved and reloaded is refused", {
  network <- fixture_network()
  path <- tempfile(fileext = ".rds")
  on.exit(unlink(path))
  saveRDS(network, path)
  reloaded <- readRDS(path)
  pts <- fixture_points()
  expect_error(travel_time_matrix(reloaded, pts, pts, n_threads = 2), "no longer valid")
  expect_true(any(grepl("no longer valid", capture.output(print(reloaded)))))
})
