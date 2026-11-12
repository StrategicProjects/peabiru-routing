iso <- function(...) {
  skip_if_not_installed("sf")
  isochrone(fixture_network(), n_threads = 2, departure_datetime = departure, ...)
}

test_that("polygon isochrones have r5r's columns, types and row order", {
  pts <- fixture_points()[1:2, ]
  x <- iso(pts, mode = "WALK", cutoffs = c(20, 0, 10, 10), percentiles = c(50, 25))
  expect_identical(class(x), c("sf", "tbl_df", "tbl", "data.frame"))
  expect_identical(names(x), c("id", "isochrone", "percentile", "polygons"))
  expect_identical(attr(x, "sf_column"), "polygons")
  expect_identical(sf::st_crs(x), sf::st_crs(4326))
  # per origin, percentiles ascending, cutoffs descending
  expect_identical(x$id, rep(pts$id, each = 4))
  expect_identical(x$percentile, rep(rep(c("p25", "p50"), each = 2), 2))
  expect_identical(x$isochrone, rep(c(20, 10), 4))
  expect_true(all(sf::st_geometry_type(x) %in% c("POLYGON", "MULTIPOLYGON")))
  # the area grows with the cutoff, and holds the smaller one
  a <- as.numeric(sf::st_area(x))
  expect_true(all(a[c(1, 3, 5, 7)] > a[c(2, 4, 6, 8)]))
  both <- sf::st_intersection(x$polygons[1], x$polygons[2])
  expect_gt(as.numeric(sf::st_area(both)) / a[2], 0.999)
})

test_that("line isochrones have r5r's columns, types and row order", {
  pts <- fixture_points()[1:2, ]
  x <- iso(pts, mode = "WALK", cutoffs = c(0, 5, 10), polygon_output = FALSE)
  expect_identical(class(x), c("sf", "tbl_df", "tbl", "data.frame"))
  expect_identical(names(x), c(
    "id", "edge_index", "osm_id", "isochrone", "travel_time_p50", "from_vertex",
    "to_vertex", "street_class", "length", "walk", "car", "car_speed",
    "bicycle", "bicycle_lts", "geometry"
  ))
  expect_type(x$edge_index, "character")
  expect_type(x$osm_id, "double")
  expect_type(x$travel_time_p50, "integer")
  expect_true(all(sf::st_geometry_type(x) == "LINESTRING"))
  expect_true(all(x$travel_time_p50 > 0 & x$travel_time_p50 <= x$isochrone))
  expect_true(all(x$isochrone %in% c(5, 10)))
  expect_true(all(x$street_class %in% c("MOTORWAY", "PRIMARY", "SECONDARY", "TERTIARY", "OTHER")))
  for (id in pts$id) {
    r <- x[x$id == id, ]
    expect_true(nrow(r) > 0)
    # band and travel time descending
    o <- order(-r$isochrone, -r$travel_time_p50)
    expect_identical(o, seq_len(nrow(r)))
  }
  expect_identical(unique(x$id), pts$id)
})

test_that("transit isochrones run with a seed and repeat", {
  pts <- fixture_points()[1, ]
  a <- iso(pts, mode = "TRANSIT", cutoffs = c(0, 15, 25), seed = 3)
  b <- iso(pts, mode = c("WALK", "TRANSIT"), cutoffs = c(0, 15, 25), seed = 3)
  expect_identical(a, b)
  l <- iso(pts, mode = "TRANSIT", cutoffs = c(0, 15, 25), seed = 3, polygon_output = FALSE)
  expect_true(nrow(l) > 0)
})

test_that("an origin far from the streets gets empty polygons and no lines", {
  far <- data.frame(id = "far", lon = -46.0, lat = -23.0)
  x <- iso(far, mode = "WALK", cutoffs = c(0, 10))
  expect_identical(nrow(x), 1L)
  expect_true(sf::st_is_empty(x$polygons))
  l <- iso(far, mode = "WALK", cutoffs = c(0, 10), polygon_output = FALSE)
  expect_identical(nrow(l), 0L)
})

test_that("arguments are checked as r5r checks them", {
  pts <- fixture_points()[1, ]
  expect_error(iso(pts, mode = "WALK", cutoffs = 0), "at least one value greater than 0")
  expect_error(iso(pts, mode = "WALK", cutoffs = c(-5, 10)), "cutoffs")
  expect_error(iso(pts, mode = "WALK", cutoffs = c(10, NA)), "cutoffs")
  expect_error(iso(pts, mode = "WALK", cutoffs = c(10, Inf)), "cutoffs")
  expect_error(iso(pts, mode = "WALK", cutoffs = "10"), "cutoffs")
  expect_error(iso(pts, mode = "WALK", zoom = 8), "zoom")
  expect_error(iso(pts, mode = "WALK", zoom = 13), "zoom")
  expect_error(iso(pts, mode = "WALK", zoom = 10.5), "zoom")
  expect_error(iso(pts, mode = "WALK", polygon_output = NA), "polygon_output")
  expect_error(iso(pts, mode = "WALK", polygon_output = FALSE, percentiles = 50),
               "Percentiles not supported for line-based isochrones")
  expect_error(iso(pts, mode = "WALK", percentiles = 100), "percentiles")
  # bicycle access and egress around public transport route
  expect_gt(nrow(iso(pts, mode = c("BICYCLE", "TRANSIT"), mode_egress = "BICYCLE",
                     seed = 1L)), 0)
  expect_error(isochrone(fixture_network(), pts, mode = "TRANSIT"), "departure_datetime")
  expect_warning(iso(pts, mode = "WALK", cutoffs = c(0, 5), sample_size = 0.5),
                 "sample_size")
  # max_trip_duration is ignored
  expect_identical(iso(pts, mode = "WALK", cutoffs = c(0, 5), max_trip_duration = 1),
                   iso(pts, mode = "WALK", cutoffs = c(0, 5)))
})

test_that("polygon isochrones of origins without rows are an error that says so", {
  # r5r fails there on undefined columns (an internal R error)
  expect_error(iso(fixture_points()[0, ], mode = "WALK", cutoffs = 10),
               "'origins' has no rows", fixed = TRUE)
})
