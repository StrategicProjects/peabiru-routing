# trip_options(): the multi-criteria trip planner.
# The core checks the search against an exhaustive enumeration; these check
# the R side: columns, types, the merge over the window, the legs, the
# limits and the argument checks.

trip_fares <- local({
  fares <- NULL
  function() {
    if (is.null(fares)) {
      fares <<- setup_fare_structure(fixture_network(), base_fare = 4.4)
      fares$transfer_time_allowance <<- 60
      fares$fares_per_transfer$fare <<- 6
    }
    fares
  }
})

test_that("options have their columns, types and order", {
  pts <- fixture_points()
  r <- trip_options(fixture_network(), pts[1:4, ], pts[5:8, ],
                    departure_datetime = departure, time_window = 5L,
                    fare_structure = trip_fares(), itineraries = FALSE,
                    by_departure = TRUE, n_threads = 2)
  expect_identical(names(r), c("options", "departures"))
  o <- r$options
  expect_identical(names(o), c(
    "from_id", "to_id", "option", "total_time", "fare", "transfers", "walk_time",
    "departure_time", "arrival_time", "n_departures", "first_departure",
    "last_departure", "description"
  ))
  expect_true(nrow(o) >= 4)
  expect_type(o$option, "integer")
  expect_type(o$transfers, "integer")
  expect_type(o$n_departures, "integer")
  expect_type(o$total_time, "double")
  # options numbered from 1 in each pair, ordered by total time
  for (k in split(o, paste(o$from_id, o$to_id))) {
    expect_identical(k$option, seq_len(nrow(k)))
    expect_false(is.unsorted(k$total_time))
  }
  expect_true(all(o$n_departures >= 1L & o$n_departures <= 5L))
  expect_true(all(o$first_departure <= o$departure_time &
                    o$departure_time <= o$last_departure))
  # every minute's frontier: each option at n_departures minutes
  d <- r$departures
  expect_identical(names(d), c("from_id", "to_id", "option", "departure_time",
                               "arrival_time", "total_time"))
  n <- table(paste(d$from_id, d$to_id, d$option))
  expect_identical(as.integer(n[paste(o$from_id, o$to_id, o$option)]), o$n_departures)
  # the option's own minute is its best
  best <- merge(o, d, by = c("from_id", "to_id", "option"))
  expect_true(all(best$total_time.x <= best$total_time.y))
})

test_that("no option of a minute dominates another of the same minute", {
  pts <- fixture_points()
  r <- trip_options(fixture_network(), pts[c(1, 3, 9), ], pts[c(20, 25, 27), ],
                    departure_datetime = departure, time_window = 3L,
                    fare_structure = trip_fares(), itineraries = FALSE,
                    by_departure = TRUE, n_threads = 2)
  v <- merge(r$departures, r$options[, c("from_id", "to_id", "option", "fare",
                                         "transfers", "walk_time")])
  # minutes have one decimal place: a dominates b for sure only where its
  # rounded minutes are smaller (fares and transfers are exact)
  for (g in split(v, paste(v$from_id, v$to_id, v$departure_time))) {
    m <- as.matrix(g[, c("total_time", "walk_time", "fare", "transfers")])
    for (a in seq_len(nrow(m))) for (b in seq_len(nrow(m))) {
      expect_false(all(m[a, 1:2] < m[b, 1:2]) && all(m[a, 3:4] <= m[b, 3:4]))
    }
  }
})

test_that("the legs are those of detailed_itineraries() and add up", {
  skip_if_not_installed("sf")
  pts <- fixture_points()
  r <- trip_options(fixture_network(), pts[2:4, ], pts[10:12, ],
                    departure_datetime = departure, time_window = 2L,
                    fare_structure = trip_fares(), n_threads = 2)
  it <- r$itineraries
  expect_s3_class(it, "sf")
  expect_identical(names(it), c(
    "from_id", "to_id", "option", "segment", "mode", "cumulative_fare",
    "segment_duration", "wait", "distance", "route", "geometry"
  ))
  expect_identical(sf::st_crs(it), sf::st_crs(4326))
  legs <- sf::st_drop_geometry(it)
  key <- paste(legs$from_id, legs$to_id, legs$option)
  okey <- paste(r$options$from_id, r$options$to_id, r$options$option)
  expect_setequal(unique(key), okey)
  # minutes on foot and the last cumulative fare (one decimal place apart
  # at most per leg in the minutes)
  walk <- tapply(ifelse(legs$mode == "WALK", legs$segment_duration, 0), key, sum)
  expect_true(all(abs(walk[okey] - r$options$walk_time) <= 0.05 * table(key)[okey] + 1e-9))
  last_fare <- tapply(legs$cumulative_fare, key, function(x) x[length(x)])
  expect_equal(as.vector(last_fare[okey]), r$options$fare)
  rides <- tapply(legs$route != "", key, sum)
  expect_identical(as.integer(pmax(rides[okey] - 1, 0)), r$options$transfers)

  df <- trip_options(fixture_network(), pts[2:4, ], pts[10:12, ],
                     departure_datetime = departure, time_window = 2L,
                     fare_structure = trip_fares(), drop_geometry = TRUE, n_threads = 2)
  expect_identical(class(df$itineraries), c("tbl_df", "tbl", "data.frame"))
  expect_identical(df$itineraries, legs)
  expect_identical(df$options, r$options)
})

test_that("criteria, limits and threads", {
  pts <- fixture_points()
  call <- function(...) {
    trip_options(fixture_network(), pts[1:3, ], pts[7:9, ],
                 departure_datetime = departure, time_window = 3L,
                 itineraries = FALSE, ...)$options
  }
  # without a fare structure there is no fare column
  a <- call(n_threads = 1)
  expect_false("fare" %in% names(a))
  expect_identical(call(n_threads = several_threads()), a)
  # time only: the fastest trip of the window, as with more criteria
  t <- call(criteria = "TIME")
  fastest <- function(x) tapply(x$total_time, paste(x$from_id, x$to_id), min)
  expect_identical(fastest(t), fastest(a))
  # a shorter trip limit only filters (with the time criterion)
  s <- call(max_trip_duration = 40L, by_departure = FALSE)
  expect_true(all(s$total_time <= 40))
  f <- call(fare_structure = trip_fares(), max_fare = 4.4)
  expect_true(all(f$fare <= 4.4))
  expect_true(nrow(call(max_options = 1)) <= 3)
  expect_error(call(criteria = "FARE"), "needs a `fare_structure`")
  expect_error(call(criteria = c("TIME", "SPEED")), "criteria")
  expect_error(call(criteria = character()), "criteria")
  expect_error(call(max_options = 0), "max_options")
  # exact walking keeps as many options or more
  expect_true(nrow(call(walk_resolution = 1)) >= nrow(a))
  expect_error(call(walk_resolution = 0), "walk_resolution")
  expect_error(call(max_fare = 3), "max_fare")
})

test_that("street-only options and output_dir", {
  pts <- fixture_points()
  w <- trip_options(fixture_network(), pts[1:2, ], pts[3:4, ], mode = "WALK",
                    departure_datetime = departure, time_window = 2L,
                    drop_geometry = TRUE)
  expect_true(all(w$options$description == "WALK"))
  expect_true(all(w$options$n_departures == 2L))
  expect_identical(w$options$walk_time, w$options$total_time)
  dir <- tempfile()
  dir.create(dir)
  expect_identical(
    trip_options(fixture_network(), pts[1:2, ], pts[3:4, ],
                 departure_datetime = departure, time_window = 2L,
                 drop_geometry = TRUE, output_dir = dir),
    dir
  )
  files <- list.files(dir)
  expect_length(files, 2)
  x <- read.csv(file.path(dir, files[1]))
  expect_true(all(c("option", "total_time", "segment", "mode") %in% names(x)))
})
