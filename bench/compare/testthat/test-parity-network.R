# Parity of the network functions with r5r, on r5r's Sao Paulo sample (the
# .pbf and the GTFS only, no elevation). Needs r5r, a JDK 21 and the R5 jar,
# as test-parity.R.

r5r_spo <- local({
  net <- NULL
  function() {
    if (!is.null(net)) return(net)
    src <- r5r_sample("spo", packages = c("sf", "data.table")) # helper-r5r.R
    dir <- file.path(tempdir(), "peabiru-parity-network-spo")
    unlink(dir, recursive = TRUE)
    dir.create(dir)
    file.copy(file.path(src, c("spo_osm.pbf", "spo.zip")), dir)
    r5 <- r5r_network_or_skip(dir)
    pb <- build_network(dir, n_threads = 2)
    points <- read.csv(file.path(src, "spo_hexgrid.csv"), colClasses = c(id = "character"))
    net <<- list(r5 = r5, pb = pb, points = points, dir = dir)
    net
  }
})

# With options(peabiru.output = "data.table") the results are identical to
# r5r's: the class, find_snap()'s index on `found` and the sf attributes
# included (data.table's self-reference pointer aside, no_selfref()).

test_that("find_snap equals r5r's", {
  n <- r5r_spo()
  local_output("data.table")
  for (m in c("WALK", "BICYCLE", "CAR")) {
    for (radius in c(1600, 100, 2500)) {
      expect_identical_r5r(find_snap(n$pb, n$points, radius = radius, mode = m),
                           r5r::find_snap(n$r5, n$points, radius = radius, mode = m))
    }
  }
})

test_that("street_network_bbox and street_network_to_sf equal r5r's", {
  n <- r5r_spo()
  local_output("data.table")
  for (o in c("polygon", "bbox", "vector")) {
    expect_identical(street_network_bbox(n$pb, o), r5r::street_network_bbox(n$r5, o))
  }
  pb <- street_network_to_sf(n$pb)
  r5 <- r5r::street_network_to_sf(n$r5)
  expect_identical_r5r(pb, r5)
})

test_that("transit_network_to_sf equals r5r's", {
  n <- r5r_spo()
  local_output("data.table")
  pb <- transit_network_to_sf(n$pb)
  r5 <- r5r::transit_network_to_sf(n$r5)
  expect_identical_r5r(pb, r5)
})

test_that("check_transit_availability equals r5r's", {
  n <- r5r_spo()
  local_output("data.table")
  same <- function(...) {
    expect_identical_r5r(check_transit_availability(n$pb, ...),
                         r5r::check_transit_availability(n$r5, ...))
  }
  same()
  same(dates = c("2019-05-13", "2019-05-19", "2007-12-31"))
  same(start_date = as.Date("2019-01-01"), end_date = "2020-12-31")
})

test_that("get_gtfs_errors equals r5r's, but for the row order", {
  n <- r5r_spo()
  local_output("data.table")
  # the rows in one order (setorderv() keeps the class and attributes)
  sorted <- function(d) {
    d <- data.table::copy(d)
    data.table::setorderv(d, names(d))
    no_selfref(d)
  }
  r5 <- r5r::get_gtfs_errors(n$r5)
  expect_identical(sorted(get_gtfs_errors(n$pb)), sorted(r5))
  expect_identical(sorted(get_gtfs_errors(n$dir)), sorted(r5))
})
