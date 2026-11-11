test_that("street itineraries have r5r's columns, types and order", {
  skip_if_not_installed("sf")
  pts <- fixture_points()
  it <- detailed_itineraries(fixture_network(), pts[1:5, ], pts[6:10, ], mode = "WALK",
                             departure_datetime = departure, n_threads = 2)
  expect_s3_class(it, "sf")
  expect_identical(names(it), c(
    "from_id", "from_lat", "from_lon", "to_id", "to_lat", "to_lon", "option",
    "departure_time", "total_duration", "total_distance", "segment", "mode",
    "segment_duration", "wait", "distance", "route", "geometry"
  ))
  expect_true(nrow(it) > 0)
  expect_type(it$option, "integer")
  expect_type(it$total_distance, "integer")
  expect_identical(unique(it$departure_time), "14:00:00")
  expect_identical(as.character(sf::st_geometry_type(it, by_geometry = FALSE)), "LINESTRING")
  expect_identical(sf::st_crs(it), sf::st_crs(4326))
  # one walking leg per pair, its distance along its line
  expect_true(all(it$segment == 1L & it$mode == "WALK"))
  len <- as.numeric(sf::st_length(it))
  expect_true(all(abs(len - it$distance) <= 0.01 * it$distance + 2))

  df <- detailed_itineraries(fixture_network(), pts[1:5, ], pts[6:10, ], mode = "WALK",
                             departure_datetime = departure, drop_geometry = TRUE,
                             n_threads = 2)
  expect_identical(class(df), "data.frame")
  expect_identical(df, sf::st_drop_geometry(it))
})

test_that("all_to_all, single points and output_dir", {
  pts <- fixture_points()
  a <- detailed_itineraries(fixture_network(), pts[1:3, ], pts[4:5, ], mode = "CAR",
                            departure_datetime = departure, all_to_all = TRUE,
                            drop_geometry = TRUE, n_threads = 2)
  expect_identical(unique(paste(a$from_id, a$to_id)),
                   paste(rep(pts$id[1:3], each = 2), rep(pts$id[4:5], 3)))
  expect_message(
    detailed_itineraries(fixture_network(), pts[1, ], pts[4:5, ], mode = "CAR",
                         departure_datetime = departure, drop_geometry = TRUE),
    "'origins' was expanded"
  )
  expect_error(
    detailed_itineraries(fixture_network(), pts[1:3, ], pts[4:5, ], mode = "CAR",
                         departure_datetime = departure),
    "same number of rows"
  )
  dir <- tempfile()
  dir.create(dir)
  expect_identical(
    detailed_itineraries(fixture_network(), pts[1:3, ], pts[4:6, ], mode = "WALK",
                         departure_datetime = departure, drop_geometry = TRUE,
                         output_dir = dir),
    dir
  )
  files <- list.files(dir)
  expect_true(length(files) > 0)
  expect_true(all(grepl("^from_.*_to_.*\\.csv$", files)))
})

test_that("unsupported options are errors", {
  pts <- fixture_points()
  net <- fixture_network()
  di <- function(...) detailed_itineraries(net, pts[1, ], pts[2, ], departure_datetime = departure, ...)
  expect_error(di(mode = "WALK", osm_link_ids = TRUE), "not supported")
  expect_error(di(mode = "WALK", osm_link_ids = TRUE, drop_geometry = TRUE), "drop_geometry")
  expect_error(di(mode = "WALK", output_dir = tempdir()), "not supported")
  expect_error(di(mode = "WALK", suboptimal_minutes = 5), "Must be 0")
  expect_error(di(mode = "WALK", fare_structure = list(type = "boston")), "not supported")
  # an empty data frame is not a valid new_lts, as in r5r
  expect_error(di(mode = "WALK", new_lts = data.frame()), "Names must include")
  # the fixture's GTFS is frequency-based, which r5r refuses here
  expect_error(di(mode = c("WALK", "TRANSIT")), "frequencies")
})

test_that("scenarios change itineraries by car", {
  pts <- fixture_points()
  net <- fixture_network()
  di <- function(...) {
    detailed_itineraries(net, pts[1:10, ], pts[11:20, ], mode = "CAR",
                         departure_datetime = departure, drop_geometry = TRUE, ...)
  }
  base <- di()
  expect_identical(di(carspeed_scale = 1), base)
  slow <- di(carspeed_scale = 0.5)
  expect_true(all(slow$total_duration >= base$total_duration))
  expect_true(any(slow$total_duration > base$total_duration))
})
