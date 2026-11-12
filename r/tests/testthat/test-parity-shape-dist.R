# Rides along GTFS shapes cut by shape_dist_traveled (R5 locates the stops
# by those distances when the shape's points and the stop times of the
# pattern's first trip all have them), against r5r on the Porto Alegre
# sample with distances added to its feeds. bench/compare/shape_dist_parity.R
# covers kilometres, mixed units, gaps and broken distances.

# Copy of `feed` (a GTFS zip) in `dir` with shape_dist_traveled in metres in
# shapes.txt and stop_times.txt: along each shape, and for each stop the
# distance of the shape point nearest to it.
add_shape_dist <- function(feed, dir) {
  tmp <- tempfile()
  utils::unzip(feed, exdir = tmp)
  rd <- function(f) utils::read.csv(file.path(tmp, f), colClasses = "character")
  shapes <- rd("shapes.txt")
  shapes <- shapes[order(shapes$shape_id, as.integer(shapes$shape_pt_sequence)), ]
  lat <- as.numeric(shapes$shape_pt_lat) * pi / 180
  lon <- as.numeric(shapes$shape_pt_lon) * pi / 180
  n <- nrow(shapes)
  hav <- c(0, 2 * 6371008.8 * asin(sqrt(sin(diff(lat) / 2)^2 +
                                          cos(lat[-n]) * cos(lat[-1]) * sin(diff(lon) / 2)^2)))
  hav[!duplicated(shapes$shape_id)] <- 0
  shapes$shape_dist_traveled <- sprintf("%.3f", ave(hav, shapes$shape_id, FUN = cumsum))
  stops <- rd("stops.txt")
  trips <- rd("trips.txt")
  st <- rd("stop_times.txt")
  shape_of <- trips$shape_id[match(st$trip_id, trips$trip_id)]
  s <- match(st$stop_id, stops$stop_id)
  slat <- as.numeric(stops$stop_lat[s])
  slon <- as.numeric(stops$stop_lon[s])
  dist <- rep("", nrow(st))
  for (id in unique(shape_of)) {
    i <- which(shape_of == id)
    p <- which(shapes$shape_id == id)
    if (!length(p)) next
    near <- vapply(i, function(k) {
      p[which.min((as.numeric(shapes$shape_pt_lat[p]) - slat[k])^2 +
                    (as.numeric(shapes$shape_pt_lon[p]) - slon[k])^2)]
    }, 1L)
    dist[i] <- shapes$shape_dist_traveled[near]
  }
  st$shape_dist_traveled <- dist
  wr <- function(x, f) utils::write.csv(x, file.path(tmp, f), row.names = FALSE, na = "")
  wr(shapes, "shapes.txt")
  wr(st, "stop_times.txt")
  out <- normalizePath(file.path(dir, basename(feed)), mustWork = FALSE)
  old <- setwd(tmp)
  on.exit(setwd(old))
  utils::zip(out, list.files(), flags = "-q")
  out
}

test_that("rides along shapes cut by shape_dist_traveled equal r5r's", {
  skip_if_not_installed("sf")
  src <- r5r_sample("poa", packages = "sf") # helper-r5r.R
  skip_if(!nzchar(Sys.which("zip")), "no zip program")
  dir <- tempfile()
  dir.create(dir)
  file.copy(file.path(src, "poa_osm.pbf"), dir)
  for (f in c("poa_eptc.zip", "poa_trensurb.zip")) add_shape_dist(file.path(src, f), dir)
  r5 <- r5r_network_or_skip(dir)
  pb <- build_network(dir)
  pts <- read.csv(file.path(src, "poa_hexgrid.csv"), colClasses = c(id = "character"))
  set.seed(11)
  o <- pts[sample(nrow(pts), 40), ]
  d <- pts[sample(nrow(pts), 40), ]
  departure <- as.POSIXct("2019-05-13 14:00:00")
  for (a in list(list(), list(osm_link_ids = TRUE),
                 list(shortest_path = FALSE, origins = o[1:12, ], destinations = d[1:12, ]))) {
    args <- list(origins = o, destinations = d, mode = c("WALK", "TRANSIT"),
                 departure_datetime = departure)
    args[names(a)] <- a
    x <- do.call(detailed_itineraries, c(list(pb), args, n_threads = 2))
    y <- do.call(r5r::detailed_itineraries, c(list(r5), args, n_threads = 2, progress = FALSE))
    class(y) <- setdiff(class(y), "data.table")
    attr(y, ".internal.selfref") <- NULL
    expect_true(any(x$mode == "BUS"))
    expect_identical(x, y)
  }
  # transit_network_to_sf() draws whole shapes: the distances play no part
  y <- lapply(r5r::transit_network_to_sf(r5), function(t) {
    class(t) <- setdiff(class(t), "data.table")
    attr(t, ".internal.selfref") <- NULL
    t
  })
  expect_identical(transit_network_to_sf(pb), y)
})
