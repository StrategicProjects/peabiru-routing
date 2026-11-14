test_that("find_snap has its columns, and NA where nothing snaps", {
  pts <- fixture_points()
  far <- data.frame(id = "far", lon = -46.0, lat = -23.0)
  x <- find_snap(fixture_network(), rbind(pts[1:3, c("id", "lon", "lat")], far))
  expect_identical(class(x), c("tbl_df", "tbl", "data.frame"))
  expect_identical(names(x), c("point_id", "lat", "lon", "snap_lat", "snap_lon",
                               "distance", "found"))
  expect_identical(x$point_id, c(pts$id[1:3], "far"))
  expect_identical(x$found, c(TRUE, TRUE, TRUE, FALSE))
  expect_true(all(is.na(x[4, c("snap_lat", "snap_lon", "distance")])))
  expect_true(all(x$distance[1:3] >= 0 & x$distance[1:3] < 300))
  # the snapped position is on the 1e-7 degree grid of the network
  expect_equal(x$snap_lat[1:3] * 1e7, round(x$snap_lat[1:3] * 1e7), tolerance = 1e-6)
  # a radius of 0 snaps nothing, nor does NA (as R5 reads it)
  expect_false(any(find_snap(fixture_network(), pts, radius = 0)$found))
  expect_false(any(find_snap(fixture_network(), pts, radius = NA_real_)$found))
  # car snapping can only use car streets: never closer than walking
  w <- find_snap(fixture_network(), pts, mode = "WALK")
  car <- find_snap(fixture_network(), pts, mode = "CAR")
  both <- w$found & car$found
  expect_true(all(car$distance[both] >= w$distance[both] - 1e-9))
})

test_that("find_snap checks its arguments", {
  pts <- fixture_points()
  net <- fixture_network()
  expect_error(find_snap(net, pts, radius = -1), "radius")
  expect_error(find_snap(net, pts, radius = c(1, 2)), "length <= 1")
  expect_error(find_snap(net, pts, radius = Inf), "finite")
  expect_error(find_snap(net, pts, mode = "walk"), "subset")
  expect_error(find_snap(net, pts, mode = "TRANSIT"), "subset")
  expect_error(find_snap(list(), pts), "peabiru_network")
})

test_that("street_network_bbox returns a vector, a bbox or a polygon", {
  v <- street_network_bbox(fixture_network(), output = "vector")
  expect_identical(names(v), c("xmin", "ymin", "xmax", "ymax"))
  expect_true(v[["xmin"]] < v[["xmax"]] && v[["ymin"]] < v[["ymax"]])
  # every street vertex is inside it
  skip_if_not_installed("sf")
  b <- street_network_bbox(fixture_network(), output = "bbox")
  expect_s3_class(b, "bbox")
  expect_identical(sf::st_crs(b), sf::st_crs(4326))
  p <- street_network_bbox(fixture_network())
  expect_s3_class(p, "sfc_POLYGON")
  expect_identical(unclass(sf::st_bbox(p)), unclass(b))
  expect_error(street_network_bbox(fixture_network(), output = "box"))
})

test_that("street_network_to_sf has its columns, and every edge end is a vertex", {
  skip_if_not_installed("sf")
  s <- street_network_to_sf(fixture_network())
  expect_identical(names(s), c("vertices", "edges"))
  v <- s$vertices
  e <- s$edges
  expect_identical(class(v), c("sf", "tbl_df", "tbl", "data.frame"))
  expect_identical(class(e), c("sf", "tbl_df", "tbl", "data.frame"))
  expect_identical(names(v), c("index", "park_and_ride", "bike_sharing", "geometry"))
  expect_identical(names(e), c("edge_index", "osm_id", "from_vertex", "to_vertex",
                               "street_class", "length", "walk", "car", "car_speed",
                               "bicycle", "bicycle_lts", "geometry"))
  expect_identical(sort(v$index), sort(unique(c(e$from_vertex, e$to_vertex))))
  expect_false(any(duplicated(v$index)))
  expect_identical(e$edge_index, sort(e$edge_index))
  expect_true(all(e$street_class %in% c("MOTORWAY", "PRIMARY", "SECONDARY", "TERTIARY", "OTHER")))
  expect_true(all(e$bicycle_lts %in% 1:4))
  expect_identical(as.character(sf::st_geometry_type(e, by_geometry = FALSE)), "LINESTRING")
  expect_identical(sf::st_crs(v), sf::st_crs(4326))
  # each edge goes from its from_vertex to its to_vertex
  xy <- sf::st_coordinates(v)
  at <- match(e$from_vertex[1:20], v$index)
  first <- t(vapply(sf::st_geometry(e)[1:20], function(g) g[1, ], numeric(2)))
  expect_equal(unname(first), unname(xy[at, ]))
})

test_that("transit_network_to_sf has its columns", {
  skip_if_not_installed("sf")
  tn <- transit_network_to_sf(fixture_network())
  expect_identical(names(tn), c("stops", "routes"))
  s <- tn$stops
  r <- tn$routes
  expect_identical(names(s), c("stop_index", "stop_id", "stop_name", "linked_to_street", "geometry"))
  expect_identical(names(r), c("agency_id", "agency_name", "route_id", "long_name",
                               "short_name", "mode", "geometry"))
  expect_identical(s$stop_index, seq_along(s$stop_index) - 1L)
  expect_true(all(startsWith(s$stop_id, "spo_gtfs:")))
  expect_identical(sf::st_is_empty(s), !s$linked_to_street)
  expect_true(nrow(r) > 0)
  expect_true(all(r$agency_name == "SPTRANS"))
  expect_true(all(r$mode %in% c("BUS", "RAIL", "SUBWAY", "TRAM")))
  # the fixture has no shapes.txt: the lines go through the stops
  expect_identical(as.character(sf::st_geometry_type(r, by_geometry = FALSE)), "LINESTRING")
})

test_that("check_transit_availability discovers dates and counts services", {
  local_output("data.frame") # rows compared with their row names
  net <- fixture_network()
  d <- check_transit_availability(net)
  expect_identical(names(d), c("date", "total_services", "active_services", "pct_active"))
  expect_s3_class(d$date, "Date")
  expect_identical(d$date, sort(d$date))
  expect_true(all(d$active_services > 0))
  expect_type(d$total_services, "integer")
  x <- check_transit_availability(net, dates = c("2019-05-13", "2019-05-19"))
  expect_identical(x$date, as.Date(c("2019-05-13", "2019-05-19")))
  expect_identical(x$pct_active, x$active_services / x$total_services)
  y <- check_transit_availability(net, dates = as.Date("2019-05-13"))
  expect_identical(y, x[1, ])
  r <- check_transit_availability(net, start_date = "2019-05-01", end_date = "2019-05-31")
  expect_identical(nrow(r), 31L)
  expect_identical(r[13, ], `rownames<-`(x[1, ], 13L))
  # a date out of every calendar has no active service
  z <- check_transit_availability(net, dates = "1990-01-01")
  expect_identical(z$active_services, 0L)
})

test_that("check_transit_availability rejects bad dates", {
  net <- fixture_network()
  expect_error(check_transit_availability(net, dates = "2019-05-13", start_date = "2019-05-01"),
               "Incorrect date arguments")
  expect_error(check_transit_availability(net, start_date = "2019-05-01"), "Incorrect date")
  expect_error(check_transit_availability(net, dates = "13/05/2019"), "Invalid date format")
  expect_error(check_transit_availability(net, dates = "2019-02-30"), "logically invalid")
  expect_error(check_transit_availability(net, dates = 20190513), "character strings or Date")
  expect_error(check_transit_availability(net, start_date = "2019-05-31", end_date = "2019-05-01"),
               "before or the same")
  expect_error(check_transit_availability(net, start_date = c("2019-05-01", "2019-05-02"),
                                          end_date = "2019-05-31"), "single date")
})

test_that("get_gtfs_errors lists gtfs-lib's errors, for a network or a directory", {
  e <- get_gtfs_errors(fixture_network())
  expect_identical(names(e), c("file", "line", "type", "field", "id", "priority"))
  expect_identical(class(e), c("tbl_df", "tbl", "data.frame"))
  # the SPTrans sample repeats its agency and calendar rows
  expect_true(all(e$type == "DuplicateKeyError"))
  expect_true(all(e$priority == "MEDIUM"))
  expect_setequal(unique(e$file), c("agency", "calendar"))
  expect_type(e$line, "integer")
  expect_identical(get_gtfs_errors(fixture_path()), e)
  # no feeds: no errors, with the columns fread gives an empty file
  dir <- tempfile()
  dir.create(dir)
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), dir)
  none <- get_gtfs_errors(dir)
  expect_identical(nrow(none), 0L)
  expect_identical(names(none), names(e))
  expect_error(get_gtfs_errors(file.path(dir, "nope")), "does not exist")
  expect_error(get_gtfs_errors(1), "peabiru_network")
})

