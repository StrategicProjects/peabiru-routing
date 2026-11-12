# A long computation stops at an interrupt (Ctrl-C, the Stop button) or at a
# limit set by setTimeLimit(), both seen by R_CheckUserInterrupt() while the
# core runs, and the session goes on.

# One call per routing entry point, each taking several seconds at one
# thread on the sample network.
long_calls <- function() {
  net <- fixture_network()
  pts <- fixture_points()
  repeated <- function(times) {
    out <- pts[rep(seq_len(nrow(pts)), times), ]
    out$id <- as.character(seq_len(nrow(out)))
    out
  }
  many <- repeated(1500)
  # a street matrix goes through a contraction hierarchy, about ten times
  # as fast as the searches the accessibility makes
  far_more <- repeated(15000)
  start <- as.POSIXct("2019-05-13 00:00:00", tz = "America/Sao_Paulo")
  transit <- c("WALK", "TRANSIT")
  list(
    street_matrix = function() {
      travel_time_matrix(net, far_more, pts, mode = "CAR", n_threads = 1)
    },
    street_accessibility = function() {
      accessibility(net, many, pts, opportunities_colnames = "jobs", mode = "CAR",
                    cutoffs = 30, n_threads = 1)
    },
    transit_matrix = function() {
      travel_time_matrix(net, pts, pts, mode = transit, departure_datetime = start,
                         time_window = 1440, draws_per_minute = 20, n_threads = 1)
    },
    transit_accessibility = function() {
      accessibility(net, pts, pts, opportunities_colnames = "jobs", mode = transit,
                    departure_datetime = start, time_window = 1440,
                    draws_per_minute = 20, cutoffs = 30, n_threads = 1)
    },
    expanded = function() {
      expanded_travel_time_matrix(net, pts, pts, mode = transit,
                                  departure_datetime = start, time_window = 1440,
                                  draws_per_minute = 20, n_threads = 1)
    },
    arrival = function() {
      arrival_travel_time_matrix(net, pts, pts, mode = transit,
                                 arrival_datetime = start + 86400,
                                 max_trip_duration = 1440, draws_per_minute = 20,
                                 n_threads = 1)
    }
  )
}

session_goes_on <- function() {
  pts <- fixture_points()
  m <- travel_time_matrix(fixture_network(), pts, pts, mode = "WALK", n_threads = 1)
  expect_gt(nrow(m), 0)
}

# Shared CI runners stall for seconds at a time (a JVM next door, a slow
# process start): the upper bounds get slack there. The calls themselves
# take far longer than any of these bounds when they are not stopped.
ci_slack <- if (nzchar(Sys.getenv("CI"))) 4 else 0

test_that("a time limit stops every long computation with R's error", {
  # it depends on how fast the machine is: each call must outlast the limit
  # and stop soon after it
  skip_on_cran()
  calls <- long_calls()
  for (name in names(calls)) {
    t0 <- Sys.time()
    err <- tryCatch({
      setTimeLimit(elapsed = 0.5, transient = TRUE)
      calls[[name]]()
      NULL
    }, error = function(e) e, finally = setTimeLimit())
    elapsed <- as.numeric(Sys.time() - t0, units = "secs")
    expect_s3_class(err, "error")
    # "reached elapsed time limit", in the session's language
    expect_match(conditionMessage(err), gettext("reached elapsed time limit", domain = "R"),
                 fixed = TRUE, info = name)
    expect_lt(elapsed, 1.5 + ci_slack)
    session_goes_on()
  }
})

# A process started in the background of a non-interactive shell (`cmd &`)
# inherits SIGINT ignored: an interrupt sent to this process then never
# arrives.
sigint_ignored <- function() {
  !tryCatch({
    tools::pskill(Sys.getpid(), tools::SIGINT)
    Sys.sleep(0.5)
    FALSE
  }, interrupt = function(c) TRUE)
}

test_that("Ctrl-C stops a long computation with an ordinary interrupt", {
  skip_on_os("windows")
  skip_on_cran()
  skip_if(sigint_ignored(), "SIGINT is ignored in this process")
  calls <- long_calls()
  for (name in c("transit_matrix", "street_matrix")) {
    t0 <- Sys.time()
    # system() would run the sleep in the foreground
    system2("sh", c("-c", shQuote(sprintf("sleep 1; kill -INT %d", Sys.getpid()))),
            wait = FALSE)
    cond <- tryCatch(calls[[name]](), interrupt = function(c) c)
    elapsed <- as.numeric(Sys.time() - t0, units = "secs")
    expect_s3_class(cond, "interrupt")
    expect_gt(elapsed, 0.9)
    expect_lt(elapsed, 2 + ci_slack)
    session_goes_on()
  }
})

test_that("an interrupt reaches calling handlers, then exiting ones", {
  seen <- character()
  res <- structure(list(condition = NULL), class = "peabiru_stopped")
  out <- tryCatch(
    withCallingHandlers(
      raise_if_stopped(res),
      interrupt = function(c) seen <<- c(seen, class(c)[1])
    ),
    interrupt = function(c) "exited"
  )
  expect_identical(out, "exited")
  expect_identical(seen, "interrupt")
  expect_identical(raise_if_stopped(1:3), 1:3)
  err <- simpleError("boom")
  stopped <- structure(list(condition = err), class = "peabiru_stopped")
  expect_identical(tryCatch(raise_if_stopped(stopped), error = function(e) e), err)
})

test_that("the service count takes a calendar date only", {
  net <- fixture_network()
  expect_length(rs_transit_availability(net$ptr, 20190513L), 2)
  expect_error(rs_transit_availability(net$ptr, 20190230L), "calendar date")
  expect_error(rs_transit_availability(net$ptr, -1L), "calendar date")
})
