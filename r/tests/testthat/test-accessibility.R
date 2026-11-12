acc <- function(...) {
  pts <- fixture_points()
  accessibility(fixture_network(), pts, pts, n_threads = 2, ...)
}

test_that("accessibility has r5r's columns, types and row order", {
  pts <- fixture_points()
  a <- acc(opportunities_colnames = c("schools", "jobs"), mode = "TRANSIT",
           departure_datetime = departure, cutoffs = c(30, 15),
           percentiles = c(50, 25), seed = 1)
  expect_identical(class(a), c("tbl_df", "tbl", "data.frame"))
  expect_identical(names(a), c("id", "opportunity", "percentile", "cutoff", "accessibility"))
  expect_type(a$percentile, "integer")
  expect_type(a$cutoff, "integer")
  expect_type(a$accessibility, "double")
  expect_identical(nrow(a), nrow(pts) * 2L * 2L * 2L)
  # per origin: opportunities as given, percentiles and cutoffs ascending
  expect_identical(a$id[1:8], rep(pts$id[1], 8))
  expect_identical(a$opportunity[1:8], rep(c("schools", "jobs"), each = 4))
  expect_identical(a$percentile[1:4], c(25L, 25L, 50L, 50L))
  expect_identical(a$cutoff[1:4], c(15L, 30L, 15L, 30L))
  # cumulative counts grow with the cutoff
  wide <- a$accessibility[a$cutoff == 30] - a$accessibility[a$cutoff == 15]
  expect_true(all(wide >= 0))
})

test_that("every decay function runs, fixed_exponential without cutoffs", {
  for (m in c("WALK", "BICYCLE", "CAR")) {
    for (f in c("step", "exponential", "linear", "logistic")) {
      v <- switch(f, linear = 10, logistic = 8, NULL)
      a <- acc(opportunities_colnames = "jobs", mode = m, decay_function = f,
               decay_value = v, cutoffs = c(10, 20))
      expect_identical(names(a), c("id", "opportunity", "percentile", "cutoff", "accessibility"))
      expect_true(all(a$accessibility >= 0))
    }
    a <- acc(opportunities_colnames = "jobs", mode = m,
             decay_function = "fixed_exponential", decay_value = 0.001)
    expect_identical(names(a), c("id", "opportunity", "percentile", "accessibility"))
  }
})

test_that("decay arguments are checked as r5r checks them", {
  base <- list(opportunities_colnames = "jobs", mode = "WALK")
  call <- function(...) do.call(acc, c(base, list(...)))
  expect_error(call(cutoffs = NULL), "must not be NULL")
  expect_error(call(decay_function = "fixed_exponential", decay_value = 0.1, cutoffs = 30),
               "must be NULL")
  expect_error(call(decay_function = "step", decay_value = 1, cutoffs = 30), "must be NULL")
  expect_error(call(decay_function = "linear", cutoffs = 30), "must not be NULL")
  expect_error(call(decay_function = "linear", decay_value = 2.5, cutoffs = 30), "integerish")
  expect_error(call(decay_function = "logistic", decay_value = 0.5, cutoffs = 30),
               "greater than or equal to 1")
  expect_error(call(decay_function = "fixed_exponential", decay_value = 1), "between 0 and 1")
  expect_error(call(decay_function = "gaussian", cutoffs = 30), "Names must be a subset")
  expect_error(call(cutoffs = 121), "<= 120")
  expect_error(call(cutoffs = 1:13), "length <= 12")
  expect_error(call(cutoffs = c(30, 30)), "duplicated")
  expect_error(call(cutoffs = 60, max_trip_duration = 30), "cannot be shorter")
  expect_error(acc(opportunities_colnames = "nothing", cutoffs = 30), "missing elements")
})

test_that("the step function ignores trips beyond the largest cutoff", {
  # r5r sets max_trip_duration to max(cutoffs) with the step function, so
  # the result does not depend on max_trip_duration
  a <- acc(opportunities_colnames = "jobs", mode = "WALK", cutoffs = 20,
           max_trip_duration = 120)
  b <- acc(opportunities_colnames = "jobs", mode = "WALK", cutoffs = 20,
           max_trip_duration = 20)
  expect_identical(a, b)
})

test_that("missing opportunity counts count as zero, with a warning", {
  pts <- fixture_points()
  pts$jobs[1:3] <- NA
  expect_warning(
    a <- accessibility(fixture_network(), pts, pts, opportunities_colnames = "jobs",
                       cutoffs = 30, n_threads = 2),
    "3 missing value"
  )
  pts$jobs[1:3] <- 0
  b <- accessibility(fixture_network(), pts, pts, opportunities_colnames = "jobs",
                     cutoffs = 30, n_threads = 2)
  expect_identical(a, b)
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
