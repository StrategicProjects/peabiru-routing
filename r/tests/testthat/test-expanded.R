test_that("expanded matrices have r5r's columns and types", {
  network <- fixture_network()
  pts <- fixture_points()
  e <- expanded_travel_time_matrix(network, pts[1:3, ], pts, mode = "TRANSIT",
                                   departure_datetime = departure, time_window = 3,
                                   breakdown = TRUE, seed = 1, n_threads = 2)
  expect_identical(class(e), "data.frame")
  expect_identical(names(e), c(
    "from_id", "to_id", "departure_time", "draw_number", "access_time",
    "wait_time", "ride_time", "transfer_time", "egress_time", "routes",
    "n_rides", "total_time"
  ))
  expect_type(e$departure_time, "character")
  expect_type(e$draw_number, "integer")
  expect_type(e$access_time, "double")
  expect_type(e$n_rides, "integer")
  expect_type(e$total_time, "double")
  expect_setequal(unique(e$departure_time), c("14:00:00", "14:01:00", "14:02:00"))
  # 5 draws per minute for frequency-based GTFS
  expect_true(all(e$draw_number %in% 1:5))
  # as in r5r, walk-only trips ("[WALK]") have no breakdown, only a total
  expect_true(all(e$access_time[e$routes %in% "[WALK]"] == 0))
  ok <- !is.na(e$total_time) & e$n_rides > 0
  expect_true(any(ok))
  parts <- e$access_time + e$wait_time + e$ride_time + e$transfer_time + e$egress_time
  expect_true(all(abs(parts[ok] - e$total_time[ok]) < 0.51))

  nobd <- expanded_travel_time_matrix(network, pts[1:3, ], pts, mode = "TRANSIT",
                                      departure_datetime = departure, time_window = 3,
                                      seed = 1, n_threads = 2)
  expect_identical(names(nobd), c("from_id", "to_id", "departure_time",
                                  "draw_number", "routes", "total_time"))
  expect_identical(nobd, e[names(nobd)])
})

test_that("trips beyond max_trip_duration have NA times and routes", {
  network <- fixture_network()
  pts <- fixture_points()
  e <- expanded_travel_time_matrix(network, pts[1:3, ], pts, mode = "TRANSIT",
                                   departure_datetime = departure, time_window = 2,
                                   max_trip_duration = 8, breakdown = TRUE,
                                   seed = 1, n_threads = 2)
  expect_true(all(e$total_time <= 8, na.rm = TRUE))
  na <- is.na(e$total_time)
  expect_true(any(na))
  expect_true(all(is.na(e$routes[na]) & is.na(e$n_rides[na]) & is.na(e$wait_time[na])))
})

test_that("arrival matrices keep the latest departure arriving in time", {
  network <- fixture_network()
  pts <- fixture_points()
  arrival <- as.POSIXct("2019-05-13 15:00:00", tz = "America/Sao_Paulo")
  a <- arrival_travel_time_matrix(network, pts[1:3, ], pts, mode = "TRANSIT",
                                  arrival_datetime = arrival, max_trip_duration = 40,
                                  breakdown = TRUE, seed = 1, n_threads = 2)
  expect_identical(names(a), c(
    "from_id", "to_id", "departure_time", "draw_number", "access_time",
    "wait_time", "ride_time", "transfer_time", "egress_time", "routes",
    "n_rides", "total_time"
  ))
  expect_false(anyDuplicated(paste(a$from_id, a$to_id)) > 0)
  dep <- as.POSIXct(paste("2019-05-13", a$departure_time), tz = "America/Sao_Paulo")
  expect_true(all(dep >= arrival - 40 * 60 & dep < arrival))
  expect_true(all(dep + round(a$total_time * 60) <= arrival))
})

test_that("arrival_datetime may be a POSIXlt, as in r5r", {
  # r5r computes arrival_datetime - difftime, which turns a POSIXlt into a
  # POSIXct
  network <- fixture_network()
  pts <- fixture_points()
  arrival <- as.POSIXct("2019-05-13 15:00:00", tz = "America/Sao_Paulo")
  arr <- function(a) {
    arrival_travel_time_matrix(network, pts[1:2, ], pts, mode = "TRANSIT",
                               arrival_datetime = a, max_trip_duration = 30,
                               seed = 1, n_threads = 2)
  }
  expect_identical(arr(as.POSIXlt(arrival)), arr(arrival))
  expect_error(arr(as.Date("2019-05-13")), "POSIXct")
  expect_error(arr("2019-05-13 15:00:00"), "POSIXct")
})

test_that("an arrival window crossing midnight reads the previous service day", {
  network <- fixture_network()
  pts <- fixture_points()
  a <- arrival_travel_time_matrix(
    network, pts[1:2, ], pts[1:5, ], mode = "TRANSIT",
    arrival_datetime = as.POSIXct("2019-05-14 00:20:00", tz = "America/Sao_Paulo"),
    max_trip_duration = 30, seed = 1, n_threads = 2
  )
  # departures from 23:50 on the 13th, written past 24:00 after midnight
  expect_true(all(a$departure_time >= "23:50:00" & a$departure_time < "24:20:00"))
})

test_that("street modes alone and other unsupported options are errors", {
  network <- fixture_network()
  pts <- fixture_points()
  expect_error(
    expanded_travel_time_matrix(network, pts, pts, mode = "WALK", n_threads = 2),
    "WALK alone"
  )
  expect_error(
    arrival_travel_time_matrix(network, pts, pts, mode = "BICYCLE",
                               arrival_datetime = departure, n_threads = 2),
    "BICYCLE alone"
  )
  expect_error(
    arrival_travel_time_matrix(network, pts, pts, mode = "TRANSIT"),
    "`arrival_datetime` is required"
  )
  # by car, a record without public transport has r5r's routes "[CAR]"
  car <- expanded_travel_time_matrix(network, pts, pts, mode = c("CAR", "TRANSIT"),
                                     departure_datetime = departure, max_car_time = 5L,
                                     seed = 1L, n_threads = 2)
  expect_true("[CAR]" %in% car$routes)
  expect_false("[WALK]" %in% car$routes)
  expect_error(
    expanded_travel_time_matrix(network, pts, pts, mode = "TRANSIT",
                                departure_datetime = departure, breakdown = NA,
                                n_threads = 2),
    "breakdown"
  )
})
