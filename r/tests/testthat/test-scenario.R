# Scenarios: new_carspeeds, carspeed_scale and new_lts. Parity with r5r is
# in test-parity.R; these check the checks and that the scenarios change
# what they should, and nothing else.

ways_of <- function(network) {
  skip_if_not_installed("sf")
  unique(street_network_to_sf(network)$edges$osm_id)
}

test_that("car speed scenarios change car times only, and as asked", {
  skip_if_not_installed("sf")
  network <- fixture_network()
  pts <- fixture_points()
  ttm <- function(...) {
    travel_time_matrix(network, pts, pts, n_threads = 2, max_trip_duration = 60L, ...)
  }
  base <- ttm(mode = "CAR")
  expect_identical(ttm(mode = "CAR", carspeed_scale = 1), base)
  slow <- ttm(mode = "CAR", carspeed_scale = 0.5)
  both <- merge(base, slow, by = c("from_id", "to_id"))
  expect_true(all(both$travel_time_p50.y >= both$travel_time_p50.x))
  expect_true(any(both$travel_time_p50.y > both$travel_time_p50.x))
  # every way listed at the factor of carspeed_scale: the same speeds on the
  # streets (connectors, way -1, are not listed and keep theirs)
  ways <- ways_of(network)
  df <- data.frame(osm_id = ways, max_speed = 0.5, speed_type = "scale")
  expect_identical(ttm(mode = "CAR", new_carspeeds = df, carspeed_scale = 0.5), slow)
  # a factor of 0 closes the roads to cars
  closed <- ttm(mode = "CAR", carspeed_scale = 0)
  expect_identical(nrow(closed), 0L)
  # walking does not change
  expect_identical(ttm(mode = "WALK", carspeed_scale = 0.5), ttm(mode = "WALK"))
})

test_that("LTS scenarios change cycling times", {
  network <- fixture_network()
  pts <- fixture_points()
  ttm <- function(...) {
    travel_time_matrix(network, pts, pts, mode = "BICYCLE", n_threads = 2, ...)
  }
  ways <- ways_of(network)
  stressful <- data.frame(osm_id = ways, lts = 4L)
  base <- ttm()
  hard <- ttm(new_lts = stressful)
  both <- merge(base, hard, by = c("from_id", "to_id"))
  expect_true(all(both$travel_time_p50.y >= both$travel_time_p50.x))
  expect_true(any(both$travel_time_p50.y > both$travel_time_p50.x))
  # with max_lts = 4 the levels do not matter
  expect_identical(ttm(new_lts = stressful, max_lts = 4), ttm(max_lts = 4))
})

test_that("scenario arguments are checked as r5r checks them", {
  network <- fixture_network()
  pts <- fixture_points()
  ttm <- function(...) {
    travel_time_matrix(network, pts, pts, mode = "CAR", n_threads = 2, ...)
  }
  expect_error(ttm(new_carspeeds = 1),
               "Must inherit from class 'data.frame', but has class 'numeric'", fixed = TRUE)
  expect_error(ttm(carspeed_scale = -1), "Element 1 is not >= 0", fixed = TRUE)
  expect_error(ttm(new_carspeeds = data.frame(osm_id = 1, max_speed = 1)),
               "Names must include the elements {'osm_id','max_speed','speed_type'}, but is missing elements {'speed_type'}",
               fixed = TRUE)
  expect_error(ttm(new_carspeeds = data.frame(osm_id = 1, max_speed = 1,
                                              speed_type = c("kph"))),
               "Must be TRUE", fixed = TRUE)
  expect_error(ttm(new_carspeeds = data.frame(osm_id = numeric(0), max_speed = numeric(0),
                                              speed_type = character(0))),
               "Assertion on 'dt$osm_id' failed: Contains only missing values.", fixed = TRUE)
  # r5r hands R5 as.character(osm_id): 1e+05 is not a Java long
  expect_error(ttm(new_carspeeds = data.frame(osm_id = 1e5, max_speed = 1,
                                              speed_type = "scale")),
               "osm_id '1e+05' is not an integer", fixed = TRUE)
  expect_error(ttm(new_lts = data.frame(osm_id = 1, lts = 1)),
               "Assertion on 'dt$lts' failed: Must be of type 'integer', not 'double'.",
               fixed = TRUE)
  # a speed that does not fit R5's 16-bit cm/s
  expect_error(ttm(carspeed_scale = 1e9), "speed of 0 or less")
  # ids not in the network are ignored, as in r5r
  expect_identical(
    ttm(new_carspeeds = data.frame(osm_id = 1, max_speed = 0.5, speed_type = "scale")),
    ttm()
  )
})

test_that("sf scenarios are checked as r5r checks them, and route", {
  skip_if_not_installed("sf")
  network <- fixture_network()
  pts <- fixture_points()
  ttm <- function(...) {
    travel_time_matrix(network, pts, pts, n_threads = 2, ...)
  }
  box <- sf::st_bbox(sf::st_as_sf(pts, coords = c("lon", "lat"), crs = 4326))
  square <- sf::st_as_sfc(box)
  poly <- sf::st_sf(poly_id = "a", scale = 0.5, priority = 1L, geometry = square)
  # half speed inside the polygon and, by carspeed_scale, outside it
  expect_identical(ttm(mode = "CAR", new_carspeeds = poly, carspeed_scale = 0.5),
                   ttm(mode = "CAR", carspeed_scale = 0.5))
  expect_error(ttm(mode = "CAR", new_carspeeds = sf::st_set_crs(poly, NA)),
               "The CRS of parameter `new_speeds` must be WGS 84", fixed = TRUE)
  bad <- poly
  bad$priority <- 1
  expect_error(ttm(mode = "CAR", new_carspeeds = bad),
               "Assertion on 'new_speeds_poly$priority' failed: Must be of type 'integer', not 'double'.",
               fixed = TRUE)
  line <- sf::st_sf(line_id = "a", lts = 9L, priority = 1L,
                    geometry = sf::st_sfc(sf::st_linestring(matrix(box, 2, byrow = TRUE)),
                                          crs = 4326))
  expect_error(ttm(mode = "BICYCLE", new_lts = line),
               "Assertion on 'new_lts_lines$lts' failed: Element 1 is not <= 4.", fixed = TRUE)
  line$lts <- 4L
  expect_s3_class(ttm(mode = "BICYCLE", new_lts = line), "data.frame")
})
