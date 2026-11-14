# Results recorded on the sample network of the package, so that a change in
# what any routing function computes is noticed: counts and sums of whole
# minutes, and some values in full. Doubles are recorded as the text of
# their 17 significant digits, which identifies a double exactly; they are
# computed by the engine in the same way on every platform. Areas of
# isochrones are compared with a tolerance (the last bits of the latitudes
# follow the system's sinh() and atan()).

digits17 <- function(x) sprintf("%.17g", x)

routed <- function(fun, ...) {
  local_output("data.frame", frame = parent.frame())
  fun(fixture_network(), ..., n_threads = 2)
}

test_that("street travel times are the recorded ones", {
  pts <- fixture_points()
  at <- function(t, first) t$travel_time_p50[seq(first, nrow(t), length.out = 8)]
  walk <- routed(travel_time_matrix, pts, pts, mode = "WALK")
  expect_identical(nrow(walk), 729L)
  expect_identical(sum(walk$travel_time_p50), 14359L)
  expect_identical(at(walk, 5), c(13L, 31L, 20L, 10L, 23L, 14L, 26L, 0L))
  bike <- routed(travel_time_matrix, pts, pts, mode = "BICYCLE")
  expect_identical(c(nrow(bike), sum(bike$travel_time_p50)), c(729L, 8615L))
  expect_identical(at(bike, 5), c(7L, 17L, 11L, 4L, 15L, 8L, 11L, 2L))
  car <- routed(travel_time_matrix, pts, pts, mode = "CAR")
  expect_identical(c(nrow(car), sum(car$travel_time_p50)), c(729L, 5440L))
  expect_identical(at(car, 5), c(4L, 11L, 5L, 9L, 6L, 4L, 6L, 8L))
  # more origins than destinations: walked from the destinations
  swapped <- routed(travel_time_matrix, pts, pts[1:3, ], mode = "WALK")
  expect_identical(c(nrow(swapped), sum(swapped$travel_time_p50)), c(81L, 1840L))
})

test_that("public transport travel times are the recorded ones", {
  pts <- fixture_points()
  t <- routed(travel_time_matrix, pts, pts, mode = c("WALK", "TRANSIT"),
              departure_datetime = departure, percentiles = c(5, 50, 95), seed = 42)
  expect_identical(nrow(t), 729L)
  expect_identical(c(sum(t$travel_time_p05), sum(t$travel_time_p50), sum(t$travel_time_p95)),
                   c(12575L, 13146L, 13576L))
  expect_identical(t$travel_time_p50[seq(7, nrow(t), length.out = 8)],
                   c(14L, 9L, 16L, 19L, 16L, 14L, 23L, 0L))
  # bicycle to and from the buses
  b <- routed(travel_time_matrix, pts, pts, mode = c("BICYCLE", "BUS"), mode_egress = "BICYCLE",
              departure_datetime = departure, time_window = 5, max_bike_time = 15, seed = 7)
  expect_identical(c(nrow(b), sum(b$travel_time_p50)), c(729L, 8109L))
  # a shorter walk to the stops
  w <- routed(travel_time_matrix, pts, pts, mode = c("WALK", "TRANSIT"),
              departure_datetime = departure, max_walk_time = 10, seed = 42)
  expect_identical(c(nrow(w), sum(w$travel_time_p50)), c(672L, 16565L))
})

test_that("accessibility is the recorded one, to the last bit", {
  pts <- fixture_points()
  a <- routed(accessibility, pts, pts, opportunities_colnames = c("jobs", "schools"),
              mode = c("WALK", "TRANSIT"), departure_datetime = departure,
              cutoffs = c(20, 40), percentiles = c(5, 50), seed = 42)
  expect_identical(nrow(a), 216L)
  expect_identical(a$accessibility[c(1:8, 100, 150, 216)],
                   c(16948, 110736, 16948, 110736, 1, 7, 1, 7, 110736, 7, 7))
  a <- routed(accessibility, pts, pts, opportunities_colnames = "jobs",
              mode = c("WALK", "TRANSIT"), departure_datetime = departure,
              decay_function = "logistic", decay_value = 8, cutoffs = c(20, 40),
              max_trip_duration = 60, seed = 42)
  expect_identical(nrow(a), 54L)
  expect_identical(digits17(a$accessibility[c(1, 2, 20, 54)]),
                   c("30168.512501917452", "104015.40862171917", "108607.64672473272",
                     "109464.8260302628"))
  a <- routed(accessibility, pts, pts, opportunities_colnames = "jobs", mode = "BICYCLE",
              decay_function = "fixed_exponential", decay_value = 0.002)
  expect_identical(digits17(a$accessibility[c(1, 2, 20, 27)]),
                   c("24448.20227962112", "21875.710705600886", "52475.312164567928",
                     "40661.829520363797"))
  a <- routed(accessibility, pts, pts, opportunities_colnames = "jobs", mode = "WALK",
              decay_function = "linear", decay_value = 10, cutoffs = 15)
  expect_identical(digits17(a$accessibility[c(1, 2, 20, 27)]),
                   c("12217.809999999998", "14385.722499999998", "74628.126666666663",
                     "51919.139166666668"))
  a <- routed(accessibility, pts, pts, opportunities_colnames = "jobs", mode = "CAR",
              decay_function = "exponential", cutoffs = 10)
  expect_identical(digits17(a$accessibility[c(1, 2, 20, 27)]),
                   c("62514.940838627299", "65467.132766161711", "58939.338366800912",
                     "54160.888871551251"))
})

test_that("expanded and arrival matrices are the recorded ones", {
  pts <- fixture_points()
  e <- routed(expanded_travel_time_matrix, pts[1:5, ], pts, mode = c("WALK", "TRANSIT"),
              departure_datetime = departure, time_window = 3, breakdown = TRUE, seed = 42)
  expect_identical(nrow(e), 2025L)
  tenths <- function(x) as.integer(sum(round(x * 10)))
  expect_identical(c(tenths(e$total_time), tenths(e$wait_time), sum(e$n_rides)),
                   c(394880L, 18154L, 918L))
  routes <- table(e$routes)
  expect_identical(as.integer(routes[c("[WALK]", "METR\u00d4 L1", "METR\u00d4 L3",
                                       "METR\u00d4 L1|METR\u00d4 L3", "2002-10")]),
                   c(1225L, 414L, 212L, 110L, 38L))
  last <- e[nrow(e), ]
  expect_identical(last$departure_time, "14:02:00")
  expect_identical(last$draw_number, 5L)
  expect_identical(c(last$access_time, last$wait_time, last$ride_time, last$transfer_time,
                     last$egress_time, last$total_time), c(3.4, 5.8, 8.2, 0.6, 5, 23))
  expect_identical(last$routes, "METR\u00d4 L1|METR\u00d4 L3")
  a <- routed(arrival_travel_time_matrix, pts[1:5, ], pts, mode = c("WALK", "TRANSIT"),
              arrival_datetime = departure + 3600, max_trip_duration = 45, seed = 42)
  expect_identical(c(nrow(a), tenths(a$total_time)), c(135L, 25628L))
  expect_identical(a$departure_time[c(1, 30, 135)], c("14:59:00", "14:43:00", "14:39:00"))
  expect_identical(a$total_time[c(1, 30, 135)], c(0.4, 17, 19.4))
})

test_that("fare structures, their files and fare-limited results are the recorded ones", {
  pts <- fixture_points()
  local_output("data.frame")
  net <- fixture_network()
  fs <- setup_fare_structure(net, 4.4)
  expect_identical(fs$fares_per_type$type, c("SUBWAY", "BUS", "RAIL"))
  expect_identical(fs$fares_per_type$fare, rep(4.4, 3))
  expect_identical(paste(fs$fares_per_transfer$first_leg, fs$fares_per_transfer$second_leg),
                   c("BUS SUBWAY", "SUBWAY SUBWAY", "BUS BUS", "RAIL SUBWAY", "RAIL BUS",
                     "BUS RAIL", "RAIL RAIL", "SUBWAY RAIL", "SUBWAY BUS"))
  expect_identical(nrow(fs$fares_per_route), 19L)
  expect_identical(fs$fares_per_route$route_id[1:3], c("2002-10", "2105-10", "2161-10"))
  fs$fares_per_transfer$fare <- 6.6
  # the archive: the five files in this order, deflated, with these contents
  z <- tempfile(fileext = ".zip")
  write_fare_structure(fs, z)
  l <- utils::unzip(z, list = TRUE)
  expect_identical(l$Name, c("global_settings.csv", "fares_per_type.csv",
                             "fares_per_transfer.csv", "fares_per_route.csv",
                             "debug_settings.csv"))
  # lines end in LF, and in CR LF on Windows (one more byte for each of the
  # 4, 4, 10, 20 and 3 lines)
  crlf <- if (.Platform$OS.type == "windows") c(4L, 4L, 10L, 20L, 3L) else 0L
  expect_identical(as.integer(l$Length), c(82L, 153L, 158L, 1465L, 44L) + crlf)
  expect_lt(file.size(z), sum(l$Length))
  ex <- tempfile()
  utils::unzip(z, exdir = ex)
  text <- function(f) {
    x <- readBin(file.path(ex, f), "raw", 1e5)
    gsub("\r\n", "\n", rawToChar(x), fixed = TRUE)
  }
  expect_identical(text("global_settings.csv"), paste0(
    "setting,value\nmax_discounted_transfers,1\ntransfer_time_allowance,120\nfare_cap,Inf\n"
  ))
  expect_identical(text("fares_per_type.csv"), paste0(
    "type,unlimited_transfers,allow_same_route_transfer,use_route_fare,fare\n",
    "SUBWAY,FALSE,FALSE,FALSE,4.4\nBUS,FALSE,FALSE,FALSE,4.4\nRAIL,FALSE,FALSE,FALSE,4.4\n"
  ))
  expect_identical(text("debug_settings.csv"),
                   "setting,value\noutput_file,\"\"\ntrip_info,MODE\n")
  expect_identical(strsplit(text("fares_per_transfer.csv"), "\n")[[1]][1:3],
                   c("first_leg,second_leg,fare", "BUS,SUBWAY,6.6", "SUBWAY,SUBWAY,6.6"))
  # the same structure gives the same archive
  z2 <- tempfile(fileext = ".zip")
  write_fare_structure(read_fare_structure(z), z2)
  expect_identical(readBin(z2, "raw", 1e5), readBin(z, "raw", 1e5))

  # the sample network's routes are given by frequencies, which the fare
  # router does not ride: with fares the trips are the walks
  t <- travel_time_matrix(net, pts, pts, mode = c("WALK", "TRANSIT"),
                          departure_datetime = departure, fare_structure = fs, max_fare = 6.6,
                          draws_per_minute = 2, seed = 42, n_threads = 2)
  expect_identical(c(nrow(t), sum(t$travel_time_p50)), c(729L, 14359L))
  pf <- pareto_frontier(net, pts[1:10, ], pts, mode = c("WALK", "TRANSIT"),
                        departure_datetime = departure, fare_structure = fs,
                        fare_cutoffs = c(0, 4.4, 6.6, 8.8), seed = 42, n_threads = 2)
  expect_identical(c(nrow(pf), sum(pf$travel_time)), c(270L, 5348L))
  expect_identical(unique(pf$monetary_cost), 0)
  expect_identical(pf$travel_time[c(1, 2, 3, 100, 270)], c(0L, 7L, 10L, 19L, 26L))
})

test_that("snapping, the bounding box and the service calendar are the recorded ones", {
  pts <- fixture_points()
  local_output("data.frame")
  net <- fixture_network()
  p <- rbind(pts[1:4, c("id", "lon", "lat")], data.frame(id = "far", lon = -46.5, lat = -23.5))
  s <- find_snap(net, p, radius = 200, mode = "CAR")
  expect_identical(s$found, c(TRUE, TRUE, TRUE, TRUE, FALSE))
  expect_identical(digits17(s$snap_lat[1:4]),
                   c("-23.555916400000001", "-23.558185900000002", "-23.553251700000001",
                     "-23.554636899999998"))
  expect_identical(digits17(s$snap_lon[1:4]),
                   c("-46.629601800000003", "-46.6320409", "-46.628872299999998",
                     "-46.631989699999998"))
  expect_identical(digits17(s$distance[1:4]),
                   c("78.357651888525908", "53.681609820120777", "0.768439335690286",
                     "1.3820202133296"))
  expect_identical(digits17(unname(street_network_bbox(net, output = "vector"))),
                   c("-46.647713500000002", "-23.565341199999999", "-46.611300399999998",
                     "-23.537108499999999"))
  a <- check_transit_availability(net, dates = c("2019-05-13", "2019-05-19", "2007-12-31"))
  expect_identical(a$total_services, c(6L, 6L, 6L))
  expect_identical(a$active_services, c(3L, 3L, 0L))
  expect_identical(a$pct_active, c(0.5, 0.5, 0))
  all_dates <- check_transit_availability(net)
  expect_identical(nrow(all_dates), 4505L)
  expect_identical(as.character(all_dates$date[1:2]), c("2008-01-01", "2008-01-02"))
  e <- get_gtfs_errors(net)
  expect_identical(e$file, c("agency", rep("calendar", 6)))
  expect_identical(e$line, c(3L, 8:13))
  expect_identical(unique(e$type), "DuplicateKeyError")
  expect_identical(e$id, c("1", "USD", "U__", "US_", "_SD", "__D", "_S_"))
  expect_identical(unique(e$priority), "MEDIUM")
})

test_that("the street and transit networks are the recorded ones", {
  skip_if_not_installed("sf")
  local_output("data.frame")
  net <- fixture_network()
  st <- street_network_to_sf(net)
  expect_identical(c(nrow(st$vertices), nrow(st$edges)), c(1937L, 5222L))
  expect_identical(c(sum(st$edges$walk), sum(st$edges$car), sum(st$edges$bicycle)),
                   c(5098L, 1680L, 3287L))
  expect_identical(sum(as.integer(st$edges$bicycle_lts)), 13383L)
  expect_identical(digits17(st$edges$length[c(1, 100, 1000)]),
                   c("178.80500000000001", "80.962999999999994", "84.247"))
  expect_identical(digits17(st$edges$car_speed[c(1, 100, 1000)]),
                   c("50.004001617431641", "40.248001098632812", "50.004001617431641"))
  classes <- table(st$edges$street_class)
  expect_identical(names(classes), c("OTHER", "PRIMARY", "SECONDARY", "TERTIARY"))
  expect_identical(as.integer(classes), c(3524L, 376L, 658L, 664L))
  tn <- transit_network_to_sf(net)
  expect_identical(c(nrow(tn$stops), nrow(tn$routes), sum(tn$stops$linked_to_street)),
                   c(654L, 28L, 148L))
  expect_identical(sort(unique(tn$routes$mode)), c("BUS", "RAIL", "SUBWAY"))
})

test_that("scenarios give the recorded travel times", {
  pts <- fixture_points()
  half <- routed(travel_time_matrix, pts, pts, mode = "CAR", carspeed_scale = 0.5)
  expect_identical(c(nrow(half), sum(half$travel_time_p50)), c(729L, 7392L))
  skip_if_not_installed("sf")
  ways <- unique(street_network_to_sf(fixture_network())$edges$osm_id)
  lts <- data.frame(osm_id = ways[seq(1, length(ways), by = 3)], lts = 4L)
  bike <- routed(travel_time_matrix, pts, pts, mode = "BICYCLE", new_lts = lts)
  expect_identical(c(nrow(bike), sum(bike$travel_time_p50)), c(729L, 10085L))
  kmh <- data.frame(osm_id = ways[seq(2, length(ways), by = 2)], max_speed = 12,
                    speed_type = "km/h")
  car <- routed(travel_time_matrix, pts, pts, mode = "CAR", new_carspeeds = kmh)
  expect_identical(c(nrow(car), sum(car$travel_time_p50)), c(729L, 7739L))
})

test_that("slope costs give the recorded travel times", {
  pts <- fixture_points()
  local_output("data.frame")
  expected <- list(TOBLER = c(14875L, 8871L, 5440L), MINETTI = c(15356L, 9081L, 5440L))
  for (model in names(expected)) {
    dir <- tempfile("recorded-elev")
    dir.create(dir)
    file.copy(file.path(fixture_path(), c("spo_osm.pbf", "spo_gtfs.zip")), dir)
    file.copy(test_path("fixtures", "spo_hills.tif"), dir)
    n <- build_network(dir, elevation = model, n_threads = 2)
    sums <- vapply(c("WALK", "BICYCLE", "CAR"), function(mode) {
      sum(travel_time_matrix(n, pts, pts, mode = mode, n_threads = 2)$travel_time_p50)
    }, integer(1), USE.NAMES = FALSE)
    expect_identical(sums, expected[[model]], label = model)
    unlink(dir, recursive = TRUE)
  }
})

test_that("isochrones are the recorded ones", {
  skip_if_not_installed("sf")
  pts <- fixture_points()
  area <- function(x) as.numeric(sf::st_area(sf::st_set_crs(sf::st_geometry(x), NA)))
  iso <- routed(isochrone, pts[c(1, 14), ], mode = "WALK", cutoffs = c(0, 10, 20),
                departure_datetime = departure)
  expect_identical(nrow(iso), 4L)
  expect_equal(area(iso), c(0.00019838786558524712, 5.3278344024948463e-05,
                            0.00026454553202479904, 5.9604345539987012e-05),
               tolerance = 1e-9)
  tr <- routed(isochrone, pts[1, ], mode = "TRANSIT", cutoffs = c(0, 15, 30),
               departure_datetime = departure, percentiles = c(25, 50), seed = 42)
  expect_equal(area(tr), c(0.0005531243442595911, 0.00011243867556501712,
                           0.00051247323191486746, 0.00011243867556501712),
               tolerance = 1e-9)
  lines <- routed(isochrone, pts[1, ], mode = "CAR", cutoffs = c(0, 5, 10),
                  departure_datetime = departure, polygon_output = FALSE)
  expect_identical(nrow(lines), 4158L)
  expect_identical(as.integer(table(lines$isochrone)), c(1206L, 2952L))
  expect_identical(sum(lines$travel_time_p50), 26934L)
  expect_identical(as.integer(lines$edge_index[1:5]), c(18L, 19L, 54L, 55L, 112L))
})

test_that("itineraries are the recorded ones", {
  skip_if_not_installed("sf")
  pts <- fixture_points()
  expected <- list(
    WALK = list(c(14.7, 14.7, 14.4, 18.3, 16.6, 37.3), c(953L, 958L, 951L, 1317L, 1005L, 2663L),
                c(37L, 33L, 33L, 54L, 45L, 90L)),
    BICYCLE = list(c(9.1, 10.3, 7.3, 15, 14.2, 21.6), c(813L, 1136L, 981L, 1362L, 1318L, 2442L),
                   c(45L, 39L, 32L, 57L, 64L, 110L)),
    CAR = list(c(6, 2.8, 4.4, 5.6, 6.6, 7.9), c(1763L, 888L, 2197L, 2431L, 1763L, 2272L),
               c(60L, 29L, 75L, 81L, 66L, 78L))
  )
  for (mode in names(expected)) {
    d <- routed(detailed_itineraries, pts[1:6, ], pts[7:12, ], mode = mode,
                departure_datetime = departure)
    expect_identical(d$total_duration, expected[[mode]][[1]], label = paste(mode, "durations"))
    expect_identical(d$total_distance, expected[[mode]][[2]], label = paste(mode, "distances"))
    # the number of points of each line
    expect_identical(lengths(sf::st_geometry(d)) %/% 2L, expected[[mode]][[3]],
                     label = paste(mode, "points"))
  }
  d <- routed(detailed_itineraries, pts[2, ], pts[8, ], mode = "CAR",
              departure_datetime = departure, osm_link_ids = TRUE)
  expect_identical(d$osm_id_list, paste0(
    "[560266875, 16976549, 565942870, 16975352, 779662527, 8579169, 22999494, ",
    "153916345, 565520814, 134365461, 22999960]"
  ))
  expect_identical(d$edge_id_list, paste0(
    "[4424, 434, 4488, 402, 4936, 82, 84, 86, 88, 442, 1222, 4436, 1178, 5494, 458]"
  ))
})

test_that("the files of output_dir have the recorded bytes", {
  pts <- fixture_points()
  net <- fixture_network()
  eol <- if (.Platform$OS.type == "windows") "\r\n" else "\n"
  file_text <- function(dir, origin) {
    f <- file.path(dir, paste0("from_", origin, ".csv"))
    rawToChar(readBin(f, "raw", file.size(f)))
  }
  lines <- function(...) paste0(c(...), eol, collapse = "")
  new_dir <- function() {
    d <- tempfile("recorded-out-")
    dir.create(d)
    d
  }
  d <- new_dir()
  travel_time_matrix(net, pts[1:2, ], pts[1:4, ], mode = c("WALK", "TRANSIT"),
                     departure_datetime = departure, percentiles = c(5, 50), seed = 42,
                     output_dir = d, n_threads = 2)
  expect_identical(sort(list.files(d)), c("from_89a8100c007ffff.csv", "from_89a8100c00fffff.csv"))
  expect_identical(file_text(d, "89a8100c007ffff"), lines(
    "from_id,to_id,travel_time_p05,travel_time_p50",
    "89a8100c007ffff,89a8100c00fffff,7,7",
    "89a8100c007ffff,89a8100c007ffff,0,0",
    "89a8100c007ffff,89a8100c077ffff,17,17",
    "89a8100c007ffff,89a8100c03bffff,9,9"
  ))
  # doubles: the shortest text that reads back the same
  d <- new_dir()
  p <- pts
  p$jobs2 <- p$jobs / 3
  accessibility(net, p[1:2, ], p, opportunities_colnames = "jobs2", mode = "WALK",
                decay_function = "logistic", decay_value = 8, cutoffs = c(10, 20),
                output_dir = d, n_threads = 2)
  expect_identical(file_text(d, "89a8100c007ffff"), lines(
    "id,opportunity,percentile,cutoff,accessibility",
    "89a8100c007ffff,jobs2,50,10,3578.630282369264",
    "89a8100c007ffff,jobs2,50,20,9800.037347707485"
  ))
  d <- new_dir()
  expanded_travel_time_matrix(net, pts[1, ], pts[1:2, ], mode = c("WALK", "TRANSIT"),
                              departure_datetime = departure, time_window = 1,
                              draws_per_minute = 2, breakdown = TRUE, seed = 42,
                              output_dir = d, n_threads = 2)
  expect_identical(file_text(d, "89a8100c00fffff"), lines(
    paste0("from_id,to_id,departure_time,draw_number,access_time,wait_time,ride_time,",
           "transfer_time,egress_time,routes,n_rides,total_time"),
    "89a8100c00fffff,89a8100c00fffff,14:00:00,1,0.0,0.0,0.0,0.0,0.0,[WALK],0,0.4",
    "89a8100c00fffff,89a8100c00fffff,14:00:00,2,0.0,0.0,0.0,0.0,0.0,[WALK],0,0.4",
    "89a8100c00fffff,89a8100c007ffff,14:00:00,1,0.0,0.0,0.0,0.0,0.0,[WALK],0,7.3",
    "89a8100c00fffff,89a8100c007ffff,14:00:00,2,0.0,0.0,0.0,0.0,0.0,[WALK],0,7.3"
  ))
})
