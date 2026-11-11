# Parity of the network functions with r5r, on r5r's Sao Paulo sample (the
# .pbf and the GTFS only, no elevation). Needs r5r, a JDK 21 and the R5 jar;
# skipped otherwise, as test-parity.R.

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

# r5r's results are data.tables (with data.table's attributes, and an index
# left by find_snap()); the columns and attributes are otherwise the same.
plain <- function(x) {
  class(x) <- setdiff(class(x), "data.table")
  attr(x, ".internal.selfref") <- NULL
  attr(x, "index") <- NULL
  x
}

test_that("find_snap equals r5r's", {
  n <- r5r_spo()
  for (m in c("WALK", "BICYCLE", "CAR")) {
    for (radius in c(1600, 100, 2500)) {
      expect_identical(find_snap(n$pb, n$points, radius = radius, mode = m),
                       plain(r5r::find_snap(n$r5, n$points, radius = radius, mode = m)))
    }
  }
})

test_that("street_network_bbox and street_network_to_sf equal r5r's", {
  n <- r5r_spo()
  for (o in c("polygon", "bbox", "vector")) {
    expect_identical(street_network_bbox(n$pb, o), r5r::street_network_bbox(n$r5, o))
  }
  pb <- street_network_to_sf(n$pb)
  r5 <- r5r::street_network_to_sf(n$r5)
  expect_identical(pb$vertices, plain(r5$vertices))
  expect_identical(pb$edges, plain(r5$edges))
})

test_that("transit_network_to_sf equals r5r's", {
  n <- r5r_spo()
  pb <- transit_network_to_sf(n$pb)
  r5 <- r5r::transit_network_to_sf(n$r5)
  expect_identical(pb$stops, plain(r5$stops))
  expect_identical(pb$routes, plain(r5$routes))
})

test_that("check_transit_availability equals r5r's", {
  n <- r5r_spo()
  same <- function(...) {
    expect_identical(check_transit_availability(n$pb, ...),
                     plain(r5r::check_transit_availability(n$r5, ...)))
  }
  same()
  same(dates = c("2019-05-13", "2019-05-19", "2007-12-31"))
  same(start_date = as.Date("2019-01-01"), end_date = "2020-12-31")
})

test_that("get_gtfs_errors equals r5r's, but for the row order", {
  n <- r5r_spo()
  sorted <- function(d) {
    d <- as.data.frame(d)
    d <- d[do.call(order, unname(as.list(d))), , drop = FALSE]
    rownames(d) <- NULL
    d
  }
  r5 <- plain(r5r::get_gtfs_errors(n$r5))
  expect_identical(sorted(get_gtfs_errors(n$pb)), sorted(r5))
  expect_identical(sorted(get_gtfs_errors(n$dir)), sorted(r5))
})
