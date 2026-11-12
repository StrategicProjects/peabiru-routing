# options(peabiru.output = ): the class of every table returned, under the
# three settings, and the same contents whatever the class. The comparison
# of the "data.table" setting with r5r's results is in the parity tests.

# The result of `f()` under each setting.
under_each <- function(f) {
  out <- list()
  for (k in c("tibble", "data.frame", "data.table")) {
    if (k == "data.table" && !requireNamespace("data.table", quietly = TRUE)) next
    local_output(k)
    out[[k]] <- f()
  }
  out
}

# A table as a plain data.frame (or a plain sf), for the comparison of the
# contents across settings.
as_plain <- function(x) {
  if (is.list(x) && !is.data.frame(x)) return(lapply(x, as_plain))
  if (!is.data.frame(x)) return(x)
  attr(x, ".internal.selfref") <- NULL
  attr(x, "index") <- NULL
  class(x) <- if (inherits(x, "sf")) c("sf", "data.frame") else "data.frame"
  x
}

classes <- list(
  tibble = c("tbl_df", "tbl", "data.frame"),
  data.frame = "data.frame",
  data.table = c("data.table", "data.frame")
)
sf_classes <- function(kind, data_table) {
  switch(kind,
    tibble = c("sf", "tbl_df", "tbl", "data.frame"),
    data.frame = c("sf", "data.frame"),
    data.table = if (data_table) c("sf", "data.table", "data.frame") else c("sf", "data.frame")
  )
}

# The class of every table of `res` (a list by setting) and equal contents.
expect_output_classes <- function(res, sf = character(), sf_dt = character(),
                                  tables = NULL) {
  for (k in names(res)) {
    x <- res[[k]]
    if (is.data.frame(x)) {
      want <- if (length(sf)) sf_classes(k, length(sf_dt) > 0) else classes[[k]]
      expect_identical(class(x), want)
    } else {
      for (n in tables) {
        want <- if (n %in% sf) sf_classes(k, n %in% sf_dt) else classes[[k]]
        expect_identical(class(x[[n]]), want, label = paste0(k, ": class of ", n))
      }
    }
    if (k != "data.frame") expect_identical(as_plain(x), as_plain(res$data.frame))
  }
  # a data.table made by data.table::setDT(), usable by reference
  if (is.data.frame(res$data.table) && !length(sf)) {
    expect_true(data.table::is.data.table(res$data.table))
    expect_false(is.null(attr(res$data.table, ".internal.selfref")))
  }
}

test_that("the matrices and accessibility follow the option", {
  net <- fixture_network()
  pts <- fixture_points()
  tr <- list(departure_datetime = departure, seed = 1, n_threads = 2)
  expect_output_classes(under_each(function() {
    travel_time_matrix(net, pts, pts, mode = "WALK", n_threads = 2)
  }))
  expect_output_classes(under_each(function() {
    do.call(travel_time_matrix, c(list(net, pts, pts, mode = c("WALK", "TRANSIT")), tr))
  }))
  expect_output_classes(under_each(function() {
    do.call(expanded_travel_time_matrix,
            c(list(net, pts[1:2, ], pts, mode = c("WALK", "TRANSIT"), time_window = 2), tr))
  }))
  expect_output_classes(under_each(function() {
    arrival_travel_time_matrix(net, pts[1:2, ], pts, mode = c("WALK", "TRANSIT"),
                               arrival_datetime = departure + 3600, seed = 1, n_threads = 2)
  }))
  expect_output_classes(under_each(function() {
    accessibility(net, pts, pts, opportunities_colnames = "jobs", cutoffs = 20,
                  mode = "WALK", n_threads = 2)
  }))
  # no rows: the class still follows
  expect_output_classes(under_each(function() {
    travel_time_matrix(net, pts[0, ], pts, mode = "WALK", n_threads = 2)
  }))
})

test_that("fares and the Pareto frontier follow the option", {
  net <- fixture_network()
  pts <- fixture_points()
  fs <- under_each(function() setup_fare_structure(net, base_fare = 4.4))
  tables <- c("fares_per_type", "fares_per_transfer", "fares_per_route")
  expect_output_classes(fs, tables = tables)
  path <- tempfile(fileext = ".zip")
  write_fare_structure(fs$tibble, path)
  expect_output_classes(under_each(function() read_fare_structure(path)), tables = tables)
  # each class is taken as a fare structure
  for (f in fs) {
    expect_output_classes(under_each(function() {
      pareto_frontier(net, pts[1:2, ], pts, departure_datetime = departure,
                      fare_structure = f, fare_cutoffs = c(4.4, 8.8), seed = 1,
                      n_threads = 2)
    }))
  }
})

test_that("find_snap, check_transit_availability and get_gtfs_errors follow the option", {
  net <- fixture_network()
  pts <- fixture_points()
  snaps <- under_each(function() find_snap(net, pts, radius = 30, mode = "CAR"))
  expect_output_classes(snaps)
  if (!is.null(snaps$data.table)) {
    # the index r5r's update of the points not found leaves
    expect_false(is.null(attr(attr(snaps$data.table, "index"), "__found")))
  }
  expect_output_classes(under_each(function() check_transit_availability(net)))
  expect_output_classes(under_each(function() {
    check_transit_availability(net, dates = character(0))
  }))
  expect_output_classes(under_each(function() get_gtfs_errors(net)))
})

test_that("trip_options() follows the option", {
  net <- fixture_network()
  pts <- fixture_points()
  expect_output_classes(under_each(function() {
    trip_options(net, pts[10, ], pts[12, ], departure_datetime = departure,
                 time_window = 3L, by_departure = TRUE, drop_geometry = TRUE,
                 n_threads = 2)
  }), tables = c("options", "itineraries", "departures"))
  skip_if_not_installed("sf")
  expect_output_classes(under_each(function() {
    trip_options(net, pts[10, ], pts[12, ], departure_datetime = departure,
                 time_window = 3L, n_threads = 2)
  }), tables = c("options", "itineraries"), sf = "itineraries", sf_dt = "itineraries")
})

test_that("sf results follow the option", {
  skip_if_not_installed("sf")
  net <- fixture_network()
  pts <- fixture_points()
  expect_output_classes(under_each(function() {
    isochrone(net, pts[1, ], mode = "WALK", cutoffs = c(0, 10),
              departure_datetime = departure, n_threads = 2)
  }), sf = "polygons", sf_dt = "polygons")
  expect_output_classes(under_each(function() {
    isochrone(net, pts[1, ], mode = "WALK", cutoffs = c(0, 10), polygon_output = FALSE,
              departure_datetime = departure, n_threads = 2)
  }), sf = "lines")
  expect_output_classes(under_each(function() {
    detailed_itineraries(net, pts[1:3, ], pts[4:6, ], mode = "WALK",
                         departure_datetime = departure, n_threads = 2)
  }), sf = "legs", sf_dt = "legs")
  expect_output_classes(under_each(function() {
    detailed_itineraries(net, pts[1:3, ], pts[4:6, ], mode = "WALK",
                         departure_datetime = departure, drop_geometry = TRUE,
                         n_threads = 2)
  }))
  expect_output_classes(under_each(function() street_network_to_sf(net)),
                        tables = c("vertices", "edges"), sf = c("vertices", "edges"),
                        sf_dt = "edges")
  expect_output_classes(under_each(function() transit_network_to_sf(net)),
                        tables = c("stops", "routes"), sf = c("stops", "routes"),
                        sf_dt = "routes")
  # an sf tibble is what sf::st_as_sf() makes of a tibble
  local_output("data.frame")
  legs <- detailed_itineraries(net, pts[1:3, ], pts[4:6, ], mode = "WALK",
                               departure_datetime = departure, n_threads = 2)
  local_output("tibble")
  expect_identical(
    detailed_itineraries(net, pts[1:3, ], pts[4:6, ], mode = "WALK",
                         departure_datetime = departure, n_threads = 2),
    sf::st_as_sf(tibble::as_tibble(legs))
  )
})

test_that("the default is tibble, and a bad value is an error naming the choices", {
  net <- fixture_network()
  pts <- fixture_points()
  local_output(NULL)
  expect_s3_class(find_snap(net, pts), "tbl_df")
  for (bad in list("tibbles", c("tibble", "data.frame"), 1, NA_character_, TRUE)) {
    local_output(bad)
    expect_error(find_snap(net, pts),
                 "must be one of \"tibble\", \"data.frame\" or \"data.table\"", fixed = TRUE)
  }
})
