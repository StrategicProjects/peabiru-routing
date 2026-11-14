# sf inputs in any coordinate reference system: reprojected to WGS 84 with
# sf::st_transform() and a message; an sf without a CRS is an error, and so
# is a geometry that is not a point where points are needed.

sf_points <- function(epsg = 31983) {
  pts <- fixture_points()
  sf::st_transform(sf::st_as_sf(pts, coords = c("lon", "lat"), crs = 4326), epsg)
}

test_that("points in another CRS are reprojected, with one message per argument", {
  skip_if_not_installed("sf")
  net <- fixture_network()
  for (epsg in c(31983, 3857, 4674)) {
    s <- sf_points(epsg)
    # the same points, reprojected by hand: the same result
    back <- sf::st_transform(s, 4326)
    msgs <- character()
    res <- withCallingHandlers(
      travel_time_matrix(net, s, s, mode = "WALK", n_threads = 2),
      peabiru_reprojected = function(m) {
        msgs <<- c(msgs, conditionMessage(m))
        invokeRestart("muffleMessage")
      }
    )
    expect_identical(res, travel_time_matrix(net, back, back, mode = "WALK", n_threads = 2))
    expect_length(msgs, 2L)
    expect_match(msgs[1], paste0("^'origins' reprojected from .* \\(EPSG:", epsg,
                                 "\\) to WGS 84 \\(EPSG:4326\\)\\.\n$"))
    expect_match(msgs[2], "^'destinations' reprojected")
  }
  s <- sf_points()
  expect_message(find_snap(net, s), "'points' reprojected from SIRGAS 2000 / UTM zone 23S",
                 fixed = TRUE)
  # an sf in WGS 84 and a data.frame say nothing
  expect_no_message(travel_time_matrix(net, sf::st_transform(s, 4326), fixture_points(),
                                       mode = "WALK", n_threads = 2))
})

test_that("every function that takes points reprojects them", {
  skip_if_not_installed("sf")
  net <- fixture_network()
  s <- sf_points()
  same <- function(f, ...) {
    expect_message(a <- f(net, ...), "reprojected", class = "peabiru_reprojected")
    args <- lapply(list(...), function(x) if (inherits(x, "sf")) sf::st_transform(x, 4326) else x)
    expect_identical(a, do.call(f, c(list(net), args)))
  }
  same(accessibility, s, s, opportunities_colnames = "jobs", cutoffs = 20, n_threads = 2)
  same(expanded_travel_time_matrix, s[1:2, ], s, mode = c("WALK", "TRANSIT"),
       departure_datetime = departure, time_window = 2, seed = 1, n_threads = 2)
  same(detailed_itineraries, s[1:3, ], s[4:6, ], mode = "WALK",
       departure_datetime = departure, n_threads = 2)
  same(isochrone, s[1, ], mode = "WALK", cutoffs = c(0, 10),
       departure_datetime = departure, n_threads = 2)
  same(trip_options, s[10, ], s[12, ], departure_datetime = departure, time_window = 2L,
       itineraries = FALSE, n_threads = 2)
  # results stay in WGS 84
  it <- suppressMessages(detailed_itineraries(net, s[1:3, ], s[4:6, ], mode = "WALK",
                                              departure_datetime = departure, n_threads = 2))
  expect_identical(sf::st_crs(it), sf::st_crs(4326))
})

test_that("an sf without a CRS and geometries that are not points are errors", {
  skip_if_not_installed("sf")
  net <- fixture_network()
  s <- sf::st_set_crs(sf_points(), NA)
  expect_error(travel_time_matrix(net, s, s, n_threads = 2),
               paste0("'origins' has no coordinate reference system (CRS). If its ",
                      "coordinates are longitude and latitude in WGS 84, set it with ",
                      "sf::st_set_crs(origins, 4326)"),
               fixed = TRUE)
  expect_error(find_snap(net, s), "sf::st_set_crs(points, 4326)", fixed = TRUE)
  # MULTIPOINT and POLYGON are refused, whatever the CRS
  u <- sf_points()
  mp <- sf::st_sf(id = "a", geometry = sf::st_cast(sf::st_combine(u[1:3, ]), "MULTIPOINT"))
  poly <- sf::st_sf(id = "a", geometry = sf::st_as_sfc(sf::st_bbox(u)))
  for (x in list(mp, poly, sf::st_set_crs(poly, NA))) {
    expect_error(travel_time_matrix(net, x, u, n_threads = 2),
                 "'origins' must be either a 'data.frame' or a 'POINT sf'.", fixed = TRUE)
  }
})

test_that("scenario polygons and lines in another CRS are reprojected", {
  skip_if_not_installed("sf")
  net <- fixture_network()
  pts <- fixture_points()
  box <- sf::st_bbox(sf::st_as_sf(pts[1:12, ], coords = c("lon", "lat"), crs = 4326))
  poly <- sf::st_sf(poly_id = "a", scale = 0.5, priority = 1L, geometry = sf::st_as_sfc(box))
  utm <- sf::st_transform(poly, 31983)
  ttm <- function(...) travel_time_matrix(net, pts, pts, n_threads = 2, ...)
  expect_message(a <- ttm(mode = "CAR", new_carspeeds = utm),
                 "'new_carspeeds' reprojected from SIRGAS 2000 / UTM zone 23S (EPSG:31983)",
                 fixed = TRUE)
  expect_identical(a, ttm(mode = "CAR", new_carspeeds = sf::st_transform(utm, 4326)))
  line <- sf::st_sf(line_id = "a", lts = 4L, priority = 1L,
                    geometry = sf::st_sfc(sf::st_linestring(matrix(box, 2, byrow = TRUE)),
                                          crs = 4326))
  lutm <- sf::st_transform(line, 32723)
  expect_message(b <- ttm(mode = "BICYCLE", new_lts = lutm), "'new_lts' reprojected", fixed = TRUE)
  expect_identical(b, ttm(mode = "BICYCLE", new_lts = sf::st_transform(lutm, 4326)))
  expect_error(ttm(mode = "BICYCLE", new_lts = sf::st_set_crs(line, NA)),
               "'new_lts' has no coordinate reference system", fixed = TRUE)
})
