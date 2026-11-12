# output_dir: the CSV files peabiru writes, one per origin, against the files
# r5r writes on its own sample data (Sao Paulo), byte for byte. The package's
# tests (r/tests/testthat/test-output_dir.R) check the files against the
# results in memory and against recorded bytes.

new_dir <- function() {
  d <- tempfile("peabiru-out-")
  dir.create(d)
  d
}

test_that("the files are byte-identical to r5r's", {
  src <- r5r_sample("spo") # helper-r5r.R
  dir <- file.path(tempdir(), "peabiru-output-parity-spo")
  unlink(dir, recursive = TRUE)
  dir.create(dir)
  file.copy(file.path(src, c("spo_osm.pbf", "spo.zip")), dir)
  r5 <- r5r_network_or_skip(dir)
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
