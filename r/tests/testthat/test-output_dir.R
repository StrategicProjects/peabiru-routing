# output_dir: results written to one CSV file per origin, as r5r writes them.

new_dir <- function() {
  d <- tempfile("peabiru-out-")
  dir.create(d)
  d
}

eol <- if (.Platform$OS.type == "windows") "\r\n" else "\n"

# The files read back as one data.frame of strings, in origin order.
read_dir <- function(dir, origin_ids) {
  files <- file.path(dir, paste0("from_", origin_ids, ".csv"))
  files <- files[file.exists(files)]
  parts <- lapply(files, utils::read.csv, colClasses = "character",
                  na.strings = character(0), check.names = FALSE)
  do.call(rbind, parts)
}

# The files hold the rows of the in-memory result: NA as an empty field,
# doubles compared as numbers.
expect_same_rows <- function(disk, mem) {
  expect_identical(names(disk), names(mem))
  expect_identical(nrow(disk), nrow(mem))
  for (j in names(mem)) {
    if (is.double(mem[[j]])) {
      # R's own parser is not correctly rounded for every 17-digit number
      # (off by an ulp at times), hence the tolerance; the Python tests and
      # the byte comparison with r5r check the digits exactly
      x <- disk[[j]]
      expect_equal(ifelse(nzchar(x), as.double(x), NA_real_), mem[[j]],
                   tolerance = 1e-14, label = j)
    } else {
      y <- as.character(mem[[j]])
      y[is.na(y)] <- ""
      expect_identical(disk[[j]], y, label = j)
    }
  }
}

transit_args <- function() {
  list(mode = c("WALK", "TRANSIT"), departure_datetime = departure, seed = 42,
       n_threads = 2)
}

test_that("street matrix files hold the in-memory result", {
  network <- fixture_network()
  pts <- fixture_points()
  d <- new_dir()
  ret <- travel_time_matrix(network, pts, pts, mode = "BICYCLE",
                            percentiles = c(75, 25), n_threads = 2, output_dir = d)
  expect_identical(ret, d)
  mem <- travel_time_matrix(network, pts, pts, mode = "BICYCLE",
                            percentiles = c(75, 25), n_threads = 2)
  expect_setequal(list.files(d), paste0("from_", unique(mem$from_id), ".csv"))
  expect_same_rows(read_dir(d, pts$id), mem)
  f <- file.path(d, list.files(d)[1])
  text <- rawToChar(readBin(f, "raw", file.size(f)))
  expect_true(startsWith(text, paste0("from_id,to_id,travel_time_p25,travel_time_p75", eol)))
  expect_true(endsWith(text, eol))
})

test_that("transit, expanded and arrival files hold the in-memory result", {
  network <- fixture_network()
  pts <- fixture_points()
  a <- c(transit_args(), list(percentiles = c(5, 50, 95), max_trip_duration = 40))
  d <- new_dir()
  do.call(travel_time_matrix, c(list(network, pts, pts, output_dir = d), a))
  mem <- do.call(travel_time_matrix, c(list(network, pts, pts), a))
  expect_true(anyNA(mem$travel_time_p95))
  expect_same_rows(read_dir(d, pts$id), mem)

  for (breakdown in c(FALSE, TRUE)) {
    a <- c(transit_args(), list(time_window = 3, breakdown = breakdown,
                                max_trip_duration = 30))
    d <- new_dir()
    o <- pts[1:6, ]
    do.call(expanded_travel_time_matrix, c(list(network, o, pts, output_dir = d), a))
    mem <- do.call(expanded_travel_time_matrix, c(list(network, o, pts), a))
    expect_true(anyNA(mem$total_time))
    expect_same_rows(read_dir(d, o$id), mem)
  }

  d <- new_dir()
  o <- pts[1:4, ]
  arr <- function(...) {
    arrival_travel_time_matrix(network, o, pts, mode = "TRANSIT",
                               arrival_datetime = departure + 3600,
                               max_trip_duration = 40, seed = 1, n_threads = 2, ...)
  }
  expect_identical(arr(output_dir = d), d)
  expect_same_rows(read_dir(d, o$id), arr())
})

test_that("accessibility files hold the in-memory result, with r5r's cutoff", {
  network <- fixture_network()
  pts <- fixture_points()
  for (decay in c("step", "fixed_exponential")) {
    a <- c(transit_args(), list(
      opportunities_colnames = c("jobs", "schools"), decay_function = decay,
      cutoffs = if (decay == "step") c(30, 15),
      decay_value = if (decay != "step") 0.05
    ))
    d <- new_dir()
    do.call(accessibility, c(list(network, pts, pts, output_dir = d), a))
    mem <- do.call(accessibility, c(list(network, pts, pts), a))
    disk <- read_dir(d, pts$id)
    # one file per origin; the files keep r5r's placeholder cutoff 0
    expect_length(list.files(d), nrow(pts))
    if (decay == "fixed_exponential") {
      expect_true(all(disk$cutoff == "0"))
      disk$cutoff <- NULL
    }
    expect_same_rows(disk, mem)
  }
})

test_that("WALK is routed from the origins, ids are quoted, files replaced", {
  network <- fixture_network()
  pts <- fixture_points()
  # more origins than destinations: in memory r5r routes WALK from the
  # destinations; the files are routed, and named, from the origins
  d <- new_dir()
  travel_time_matrix(network, pts, pts[1:3, ], n_threads = 2, output_dir = d)
  disk <- read_dir(d, pts$id)
  expect_true(all(disk$to_id %in% pts$id[1:3]))
  expect_gt(length(list.files(d)), 3)

  p <- pts[1:3, ]
  p$id <- c("a,b", "q\"x", "plain")
  d <- new_dir()
  writeLines(c("old", "old"), file.path(d, "from_plain.csv"))
  writeLines("kept", file.path(d, "other.txt"))
  travel_time_matrix(network, p, p, n_threads = 2, output_dir = d)
  expect_setequal(list.files(d), c("from_a,b.csv", "from_q\"x.csv",
                                   "from_plain.csv", "other.txt"))
  expect_true("\"a,b\",\"a,b\",0" %in% readLines(file.path(d, "from_a,b.csv")))
  expect_true(any(grepl("\"q\"\"x\"", readLines(file.path(d, "from_q\"x.csv")))))
  expect_false("old" %in% readLines(file.path(d, "from_plain.csv")))
  expect_identical(readLines(file.path(d, "other.txt")), "kept")
})

test_that("output_dir is checked as r5r checks it", {
  network <- fixture_network()
  pts <- fixture_points()
  call <- function(d) travel_time_matrix(network, pts, pts, n_threads = 2, output_dir = d)
  missing_dir <- file.path(tempdir(), "peabiru-no-such-dir")
  expect_error(call(missing_dir), paste0(
    "Assertion on 'output_dir' failed: Directory '", missing_dir, "' does not exist."
  ), fixed = TRUE)
  expect_error(call(""), "Directory '' does not exist", fixed = TRUE)
  f <- tempfile()
  writeLines("x", f)
  expect_error(call(f), "Directory expected, but file in place", fixed = TRUE)
  expect_error(call(1), "output_dir")
  expect_error(call(c("a", "b")), "output_dir")
  expect_error(call(NA_character_), "output_dir")
  p <- pts[1:2, ]
  p$id <- c(NA, "x")
  expect_error(
    suppressWarnings(travel_time_matrix(network, p, p, output_dir = new_dir())),
    "cannot be missing"
  )
  # a file that cannot be written is an error naming it
  p$id <- c("no/such/dir", "x")
  expect_error(travel_time_matrix(network, p, p, n_threads = 1, output_dir = new_dir()),
               "from_no/such/dir.csv", fixed = TRUE)
  expect_error(accessibility(network, pts, pts, opportunities_colnames = "jobs",
                             cutoffs = 30, output_dir = missing_dir), "does not exist")
  expect_error(expanded_travel_time_matrix(network, pts, pts, mode = "TRANSIT",
                                           departure_datetime = departure,
                                           output_dir = missing_dir), "does not exist")
})

# r5r on its own sample data writing the same files; skipped on CRAN and
# wherever r5r cannot start (see test-parity.R)
test_that("the files are byte-identical to r5r's", {
  skip_on_cran()
  skip_if_not_installed("r5r")
  src <- system.file("extdata/spo", package = "r5r")
  skip_if(!file.exists(file.path(src, "spo_osm.pbf")), "r5r sample data not found")
  dir <- file.path(tempdir(), "peabiru-output-parity-spo")
  unlink(dir, recursive = TRUE)
  dir.create(dir)
  file.copy(file.path(src, c("spo_osm.pbf", "spo.zip")), dir)
  if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx2G")
  r5 <- tryCatch(suppressMessages(r5r::build_network(dir)), error = function(e) NULL)
  skip_if(is.null(r5), "r5r could not build a network (Java 21 or the R5 jar missing?)")
  pb <- build_network(dir, n_threads = 2)
  pts <- read.csv(file.path(src, "spo_hexgrid.csv"), colClasses = c(id = "character"))
  pts$jobs <- pts$population
  pts <- pts[1:120, ]

  same_files <- function(fun, ...) {
    a <- new_dir()
    b <- new_dir()
    expect_identical(getExportedValue("peabiru", fun)(pb, ..., n_threads = 2,
                                                      output_dir = a), a)
    expect_identical(getExportedValue("r5r", fun)(r5, ..., n_threads = 2,
                                                  progress = FALSE, output_dir = b), b)
    files <- sort(list.files(b))
    expect_gt(length(files), 0)
    expect_identical(sort(list.files(a)), files)
    for (f in files) {
      x <- readBin(file.path(a, f), "raw", file.size(file.path(a, f)))
      y <- readBin(file.path(b, f), "raw", file.size(file.path(b, f)))
      expect_identical(x, y, label = paste(fun, f))
    }
  }
  tr <- c("WALK", "TRANSIT")
  same_files("travel_time_matrix", origins = pts, destinations = pts, mode = "WALK",
             percentiles = c(25, 75))
  same_files("travel_time_matrix", origins = pts, destinations = pts[1:10, ],
             mode = "WALK")
  same_files("travel_time_matrix", origins = pts, destinations = pts, mode = "CAR")
  same_files("travel_time_matrix", origins = pts, destinations = pts, mode = tr,
             departure_datetime = departure, percentiles = c(5, 50, 95),
             max_trip_duration = 40, seed = 42)
  same_files("expanded_travel_time_matrix", origins = pts[1:5, ], destinations = pts,
             mode = tr, departure_datetime = departure, time_window = 3,
             breakdown = TRUE, max_trip_duration = 45, seed = 42)
  same_files("arrival_travel_time_matrix", origins = pts[1:5, ], destinations = pts,
             mode = tr, arrival_datetime = departure + 3600, max_trip_duration = 45,
             seed = 42)
  same_files("accessibility", origins = pts, destinations = pts,
             opportunities_colnames = c("jobs", "schools"), mode = tr,
             departure_datetime = departure, decay_function = "logistic",
             cutoffs = c(20, 40), decay_value = 8, seed = 42)
  same_files("accessibility", origins = pts, destinations = pts,
             opportunities_colnames = "jobs", mode = "BICYCLE",
             decay_function = "fixed_exponential", decay_value = 0.002)
})
