# Parity with r5r on networks built with an elevation raster: r5r's Porto
# Alegre sample with its poa_elevation.tif, and the Sao Paulo fixture with
# the synthetic fixtures/spo_hills.tif, also warped by GDAL to SIRGAS 2000 /
# UTM zone 23S (spo_hills_31983.tif, 50 m, bilinear) and to SIRGAS 2000
# longitude and latitude (spo_hills_4674.tif, Float32), which r5r reads
# through GeoTools' coordinate transforms. Needs r5r, a JDK 21 and the R5 jar,
# so it is skipped on CRAN and wherever r5r cannot start (see helper-r5r.R).
# bench/compare/elevation_parity.R runs the full comparison.

elevation_networks <- local({
  nets <- list()
  function(key) {
    if (!is.null(nets[[key]])) return(nets[[key]])
    city <- sub("-.*", "", key)
    model <- toupper(sub(".*-", "", key))
    dir <- file.path(tempdir(), paste0("peabiru-parity-elev-", key))
    unlink(dir, recursive = TRUE)
    dir.create(dir)
    if (city == "poa") {
      src <- r5r_sample("poa", packages = "sf") # helper-r5r.R
      file.copy(file.path(src, c("poa_osm.pbf", "poa_eptc.zip", "poa_trensurb.zip",
                                 "poa_elevation.tif")), dir)
      points <- read.csv(file.path(src, "poa_hexgrid.csv"), colClasses = c(id = "character"))
      points <- points[seq(1, nrow(points), by = 9), ]
    } else {
      r5r_sample("spo", packages = "sf")
      file.copy(file.path(fixture_path(), c("spo_osm.pbf", "spo_gtfs.zip")), dir)
      tif <- if (city == "spo") "spo_hills.tif" else paste0("spo_hills_", sub("spo", "", city), ".tif")
      file.copy(test_path("fixtures", tif), dir)
      points <- fixture_points()
    }
    if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx2G")
    why <- NULL
    r5 <- tryCatch(suppressMessages(r5r::build_network(dir, elevation = model)),
                   error = function(e) {
                     why <<- conditionMessage(e)
                     NULL
                   })
    skip_without_r5r(is.null(r5), paste0("r5r could not build a network: ", why))
    pb <- build_network(dir, elevation = model, n_threads = 2)
    nets[[key]] <<- list(r5 = r5, pb = pb, points = points)
    nets[[key]]
  }
})

expect_elevation_parity <- function(key, fun, ...) {
  n <- elevation_networks(key)
  pb <- getExportedValue("peabiru", fun)(n$pb, ..., n_threads = 2)
  r5 <- getExportedValue("r5r", fun)(n$r5, ..., n_threads = 2, progress = FALSE)
  expect_identical(pb, as.data.frame(r5))
}

test_that("walking and cycling with elevation equal r5r's", {
  for (key in c("poa-tobler", "poa-minetti", "spo-tobler", "spo31983-tobler",
                 "spo4674-minetti")) {
    pts <- elevation_networks(key)$points
    expect_identical(elevation_networks(key)$pb$elevation, toupper(sub(".*-", "", key)))
    expect_elevation_parity(key, "travel_time_matrix", origins = pts, destinations = pts,
                            mode = "WALK")
    expect_elevation_parity(key, "travel_time_matrix", origins = pts,
                            destinations = pts[1:20, ], mode = "WALK", walk_speed = 4.4)
    expect_elevation_parity(key, "travel_time_matrix", origins = pts, destinations = pts,
                            mode = "BICYCLE", bike_speed = 15, max_lts = 3)
  }
})

test_that("transit access and accessibility with elevation equal r5r's", {
  key <- "poa-tobler"
  pts <- elevation_networks(key)$points
  pts$jobs <- seq_len(nrow(pts))
  expect_elevation_parity(key, "travel_time_matrix", origins = pts, destinations = pts,
                          mode = c("BICYCLE", "TRANSIT"), max_bike_time = 20L,
                          departure_datetime = departure, seed = 1L)
  expect_elevation_parity(key, "accessibility", origins = pts, destinations = pts,
                          opportunities_colnames = "jobs", mode = c("WALK", "TRANSIT"),
                          cutoffs = c(20L, 40L), departure_datetime = departure, seed = 1L)
})

test_that("a scenario on a network with elevation equals r5r's, with its warning", {
  key <- "spo-tobler"
  n <- elevation_networks(key)
  pts <- n$points
  expect_warning(
    pb <- travel_time_matrix(n$pb, pts, pts, mode = "WALK", carspeed_scale = 0.9,
                             n_threads = 2),
    "missing elevation"
  )
  r5 <- suppressMessages(suppressWarnings(
    r5r::travel_time_matrix(n$r5, pts, pts, mode = "WALK", carspeed_scale = 0.9,
                            n_threads = 2, progress = FALSE)
  ))
  expect_identical(pb, as.data.frame(r5))
})

test_that("rasters in SIRGAS 2000 give r5r's transit times and itineraries", {
  for (key in c("spo31983-tobler", "spo4674-minetti")) {
    n <- elevation_networks(key)
    pts <- n$points
    expect_elevation_parity(key, "travel_time_matrix", origins = pts, destinations = pts,
                            mode = c("BICYCLE", "TRANSIT"), max_bike_time = 20L,
                            departure_datetime = departure, seed = 1L)
    expect_elevation_parity(key, "detailed_itineraries", origins = pts[1:8, ],
                            destinations = pts[9:16, ], mode = "BICYCLE",
                            departure_datetime = departure, drop_geometry = TRUE)
  }
})
