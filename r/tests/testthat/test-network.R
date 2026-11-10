test_that("a network is built from a directory with a .pbf and GTFS", {
  network <- fixture_network()
  expect_s3_class(network, "peabiru_network")
  expect_identical(network$elevation, "NONE")
  out <- capture.output(print(network))
  expect_match(out[1], "<peabiru_network>")
  expect_true(any(grepl("1 feed\\(s\\) \\(spo_gtfs\\)", out)))
})

test_that("a street-only network has no transit", {
  dir <- tempfile("net")
  dir.create(dir)
  on.exit(unlink(dir, recursive = TRUE))
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), dir)
  network <- build_network(dir, n_threads = 2)
  expect_true(any(grepl("transit:   none", capture.output(print(network)))))
  pts <- fixture_points()
  expect_error(
    travel_time_matrix(network, pts, pts, mode = "TRANSIT",
                       departure_datetime = departure, n_threads = 2),
    "no transit services available"
  )
  ttm <- travel_time_matrix(network, pts, pts, mode = "CAR", n_threads = 2)
  expect_gt(nrow(ttm), 0)
})

test_that("bad data paths are rejected", {
  expect_error(build_network(tempfile("missing")), "does not exist")
  empty <- tempfile("empty")
  dir.create(empty)
  on.exit(unlink(empty, recursive = TRUE))
  expect_error(build_network(empty), "OSM .pbf file is required")
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), file.path(empty, "a.pbf"))
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), file.path(empty, "b.pbf"))
  expect_error(build_network(empty), "more than one .pbf")
  expect_error(build_network(fixture_path(), elevation = "FLAT"), "Invalid value")
})

test_that("elevation needs elevation = 'NONE' when there is a raster", {
  dir <- tempfile("net")
  dir.create(dir)
  on.exit(unlink(dir, recursive = TRUE))
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), dir)
  writeBin(raw(10), file.path(dir, "elevation.tif"))
  expect_error(build_network(dir),
               "Elevation is not supported by peabiru yet, and .* has the raster 'elevation.tif'")
  network <- build_network(dir, elevation = "NONE", n_threads = 2)
  expect_true(network$has_tif)
})

test_that("a file whose name merely contains 'tif' does not block the build", {
  # r5r's grep(".tif") is a regular expression, so 'identificacao.csv'
  # matches; r5r keeps elevation = "TOBLER" and R5 finds no raster.
  dir <- tempfile("net")
  dir.create(dir)
  on.exit(unlink(dir, recursive = TRUE))
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), dir)
  writeLines("zone", file.path(dir, "identificacao_zonas.csv"))
  network <- build_network(dir, n_threads = 2)
  expect_s3_class(network, "peabiru_network")
  expect_identical(network$elevation, "NONE")
  # the swap of origins and destinations stays off, as in r5r
  expect_true(network$has_tif)
})

test_that("a network that was saved and reloaded is refused", {
  network <- fixture_network()
  path <- tempfile(fileext = ".rds")
  on.exit(unlink(path))
  saveRDS(network, path)
  reloaded <- readRDS(path)
  pts <- fixture_points()
  expect_error(travel_time_matrix(reloaded, pts, pts, n_threads = 2), "no longer valid")
  expect_true(any(grepl("no longer valid", capture.output(print(reloaded)))))
})

# A zip file of stored (uncompressed) entries, written without a zip
# program; the engine does not check CRCs, so they are left at zero.
write_stored_zip <- function(path, files) {
  u16 <- function(x) writeBin(as.integer(x), raw(), size = 2, endian = "little")
  u32 <- function(x) writeBin(as.integer(x), raw(), size = 4, endian = "little")
  body <- raw()
  cd <- raw()
  for (name in names(files)) {
    data <- charToRaw(files[[name]])
    nm <- charToRaw(name)
    offset <- length(body)
    sizes <- c(u32(length(data)), u32(length(data)))
    body <- c(body, u32(0x04034b50), u16(20), u16(0), u16(0), raw(8), sizes,
              u16(length(nm)), u16(0), nm, data)
    cd <- c(cd, u32(0x02014b50), u16(20), u16(20), u16(0), u16(0), raw(8),
            sizes, u16(length(nm)), raw(12), u32(offset), nm)
  }
  end <- c(u32(0x06054b50), raw(4), u16(length(files)), u16(length(files)),
           u32(length(cd)), u32(length(body)), u16(0))
  writeBin(c(body, cd, end), path)
}

# A small feed around the sample streets; each test breaks it in one way.
small_feed <- list(
  "agency.txt" = "agency_id,agency_name,agency_url,agency_timezone\nA,A,http://a,America/Sao_Paulo\n",
  "stops.txt" = "stop_id,stop_lat,stop_lon\nA,-23.5563,-46.6290\nB,-23.5577,-46.6321\n",
  "routes.txt" = "route_id,route_type\nR,3\n",
  "trips.txt" = "route_id,service_id,trip_id\nR,S,T1\n",
  "stop_times.txt" = paste0(
    "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n",
    "T1,14:00:00,14:00:00,A,1\nT1,14:05:00,14:05:00,B,2\n"
  ),
  "calendar.txt" = paste0(
    "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,",
    "start_date,end_date\nS,1,1,1,1,1,1,1,20190101,20191231\n"
  )
)

# A data directory with the sample streets and `feed` as bus.zip.
feed_dir <- function(feed) {
  dir <- tempfile("net")
  dir.create(dir)
  file.copy(file.path(fixture_path(), "spo_osm.pbf"), dir)
  write_stored_zip(file.path(dir, "bus.zip"), feed)
  dir
}

refuses <- "r5r refuses to build a network from this feed."

test_that("a feed with its tables in a subfolder gives a warning", {
  feed <- small_feed
  names(feed) <- paste0("gtfs/", names(feed))
  dir <- feed_dir(feed)
  on.exit(unlink(dir, recursive = TRUE))
  expect_warning(
    network <- build_network(dir, n_threads = 2),
    paste0("GTFS feed 'bus.zip': its tables are in a subfolder of the .zip ('gtfs/'). ",
           refuses),
    fixed = TRUE
  )
  expect_true(any(grepl(" 1 patterns", capture.output(print(network)))))
})

test_that("a feed without agency.txt gives a warning", {
  dir <- feed_dir(small_feed[names(small_feed) != "agency.txt"])
  on.exit(unlink(dir, recursive = TRUE))
  expect_warning(
    build_network(dir, n_threads = 2),
    paste("GTFS feed 'bus.zip': it has no agency.txt.", refuses),
    fixed = TRUE
  )
})

test_that("trips that refer to a missing stop or route give a warning", {
  feed <- small_feed
  feed[["trips.txt"]] <- paste0(feed[["trips.txt"]], "R,S,T2\nX,S,T3\n")
  feed[["stop_times.txt"]] <- paste0(
    feed[["stop_times.txt"]],
    "T2,14:00:00,14:00:00,A,1\nT2,14:05:00,14:05:00,Z,2\n",
    "T3,14:00:00,14:00:00,A,1\nT3,14:05:00,14:05:00,B,2\n"
  )
  dir <- feed_dir(feed)
  on.exit(unlink(dir, recursive = TRUE))
  expect_warning(
    build_network(dir, n_threads = 2),
    paste("GTFS feed 'bus.zip': 2 trips left out (1 refers to a stop that does not exist,",
          "1 to a route that does not exist).", refuses),
    fixed = TRUE
  )
})

test_that("clean feeds give no warning", {
  expect_no_warning(build_network(fixture_path(), n_threads = 2))
  dir <- feed_dir(small_feed)
  on.exit(unlink(dir, recursive = TRUE))
  expect_no_warning(build_network(dir, n_threads = 2))
})
