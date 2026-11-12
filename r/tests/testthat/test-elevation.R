# Elevation: networks built with a .tif raster. The raster in fixtures/ is
# synthetic (smooth hills between 743 and 834 m over the Sao Paulo fixture,
# made with terra for these tests), not measured data.

hills <- function() test_path("fixtures", "spo_hills.tif")

# a data directory with the fixture's .pbf and GTFS and the given rasters
# (named as the names of `rasters`, from the paths in it)
elevation_dir <- function(rasters = c("spo_hills.tif" = hills())) {
  dir <- tempfile("elev")
  dir.create(dir)
  file.copy(file.path(fixture_path(), c("spo_osm.pbf", "spo_gtfs.zip")), dir)
  for (n in names(rasters)) file.copy(rasters[[n]], file.path(dir, n))
  dir
}

test_that("a network takes slope costs from a .tif raster", {
  dir <- elevation_dir()
  on.exit(unlink(dir, recursive = TRUE))
  pts <- fixture_points()
  flat <- fixture_network()
  tobler <- build_network(dir, n_threads = 2)
  expect_identical(tobler$elevation, "TOBLER")
  expect_true(any(grepl("elevation: TOBLER", capture.output(print(tobler)))))
  minetti <- build_network(dir, elevation = "minetti", n_threads = 2)
  expect_identical(minetti$elevation, "MINETTI")
  none <- build_network(dir, elevation = "NONE", n_threads = 2)
  expect_identical(none$elevation, "NONE")
  expect_true(none$has_tif)
  for (mode in c("WALK", "BICYCLE")) {
    t0 <- travel_time_matrix(none, pts, pts, mode = mode, n_threads = 2)
    expect_identical(t0, travel_time_matrix(flat, pts, pts, mode = mode, n_threads = 2))
    t1 <- travel_time_matrix(tobler, pts, pts, mode = mode, n_threads = 2)
    t2 <- travel_time_matrix(minetti, pts, pts, mode = mode, n_threads = 2)
    expect_false(identical(t0$travel_time_p50, t1$travel_time_p50))
    expect_false(identical(t1$travel_time_p50, t2$travel_time_p50))
  }
  # never for cars
  expect_identical(travel_time_matrix(tobler, pts, pts, mode = "CAR", n_threads = 2),
                   travel_time_matrix(flat, pts, pts, mode = "CAR", n_threads = 2))
})

test_that("a scenario routes without the slope costs, with r5r's warning", {
  dir <- elevation_dir()
  on.exit(unlink(dir, recursive = TRUE))
  pts <- fixture_points()
  tobler <- build_network(dir, n_threads = 2)
  expect_warning(
    s <- travel_time_matrix(tobler, pts, pts, mode = "WALK", carspeed_scale = 0.9,
                            n_threads = 2),
    "This may affect results for WALK or BICYCLE due to missing elevation. See issue #555."
  )
  expect_identical(s, travel_time_matrix(fixture_network(), pts, pts, mode = "WALK",
                                         n_threads = 2))
  # no warning on a network without elevation
  expect_no_warning(travel_time_matrix(fixture_network(), pts, pts, mode = "WALK",
                                       carspeed_scale = 0.9, n_threads = 2))
})

test_that("rasters peabiru cannot read as r5r does are errors", {
  bad <- tempfile(fileext = ".tif")
  writeBin(raw(10), bad)
  dir <- elevation_dir(c("elevation.tif" = bad))
  on.exit(unlink(c(dir, bad), recursive = TRUE))
  expect_error(build_network(dir, n_threads = 2), "elevation.tif: not a TIFF file")
  # with "NONE" the raster is not read
  expect_identical(build_network(dir, elevation = "NONE", n_threads = 2)$elevation, "NONE")

  dir2 <- elevation_dir(c("hills.tiff" = hills()))
  on.exit(unlink(dir2, recursive = TRUE), add = TRUE)
  expect_error(build_network(dir2, n_threads = 2), "ends in .tiff")

  dir3 <- elevation_dir(c("a.tif" = hills(), "b.tif" = hills()))
  on.exit(unlink(dir3, recursive = TRUE), add = TRUE)
  expect_error(build_network(dir3, n_threads = 2), "2 elevation rasters \\(a.tif, b.tif\\)")
})

test_that("a raster file in upper case is not one, as in r5r", {
  dir <- elevation_dir(c("HILLS.TIF" = hills()))
  on.exit(unlink(dir, recursive = TRUE))
  network <- build_network(dir, n_threads = 2)
  expect_identical(network$elevation, "NONE")
  expect_false(network$has_tif)
})

test_that("rasters in other coordinate reference systems are read", {
  pts <- fixture_points()
  for (tif in c("spo_hills_31983.tif", "spo_hills_4674.tif")) {
    dir <- elevation_dir(stats::setNames(test_path("fixtures", tif), tif))
    on.exit(unlink(dir, recursive = TRUE), add = TRUE)
    n <- build_network(dir, n_threads = 2)
    expect_identical(n$elevation, "TOBLER")
    expect_false(identical(travel_time_matrix(n, pts, pts, mode = "WALK", n_threads = 2),
                           travel_time_matrix(fixture_network(), pts, pts, mode = "WALK",
                                              n_threads = 2)))
  }
})

test_that("voids and values beyond +/-3276.7 m wrap around as in r5r, with a warning", {
  dir <- elevation_dir(c("voids.tif" = test_path("fixtures", "spo_voids.tif")))
  on.exit(unlink(dir, recursive = TRUE))
  expect_warning(n <- build_network(dir, n_threads = 2),
                 "elevation samples on the network, [0-9]+ with a value beyond \\+/-3276.7 m")
  expect_identical(n$elevation, "TOBLER")
  # a raster without them gives no warning
  expect_no_warning(build_network(elevation_dir(), n_threads = 2))
})
