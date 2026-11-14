# The data of the Recife metropolitan region example (site/figures/rmr.R, the
# R article recife-metropolitan and the Python page of the same name): the
# street network, the grid with the population of the Censo 2022, the public
# schools and the primary health care units of the 14 municipalities, as
# prepared by site/figures/pernambuco-data.R (prepare_city("rmr")), and the
# GTFS feed of Grande Recife Consórcio de Transporte Metropolitano.
#
#   Rscript site/figures/rmr-data.R
#
# The feed is published by the consortium at
# https://www.granderecife.pe.gov.br/gtfs/gtfs.zip, an address that always
# serves the current file; the figures were computed with feed_version
# v2026.10.26 (sha256 below). It is kept, under the name of its version, in
# <cache>/raw/, and never copied into the repository. A file found there is
# used as it is; otherwise it is downloaded, and its version read from
# feed_info.txt: another version is another network, and gives other numbers.
#
#   <cache>/raw/granderecife-<version>.zip   the feed
#   <cache>/network/                         rmr.osm.pbf (a link to the cut
#                                            of pernambuco-data.R) and the feed
#   <cache>/results/                         what rmr.R computes, and its log
#
# The cache is PEABIRU_RMR_CACHE (default:
# tools::R_user_dir("peabiru", "cache")/rmr); the streets, grid and units come
# from the cache of pernambuco-data.R (PEABIRU_PE_CACHE).

# prepare_city() and fetch() come from pernambuco-data.R: sourced here when
# this file is run by Rscript, and by the caller otherwise
if (!exists("prepare_city")) {
  this <- sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE))
  if (!length(this)) stop("source site/figures/pernambuco-data.R before this file")
  source(file.path(dirname(this), "pernambuco-data.R"))
}

rmr_feed <- list(
  url = "https://www.granderecife.pe.gov.br/gtfs/gtfs.zip",
  version = "v2026.10.26",
  sha256 = "1b0657fb8f6bcb709c4bfb17c4eeb0c4af6325c3f70c68c1e534c3046c6a941f"
)

rmr_cache <- function() {
  d <- Sys.getenv("PEABIRU_RMR_CACHE",
                  file.path(tools::R_user_dir("peabiru", "cache"), "rmr"))
  dir.create(file.path(d, "raw"), recursive = TRUE, showWarnings = FALSE)
  d
}

# One table of a GTFS feed, every column as text.
feed_table <- function(feed, name, select = NULL) {
  if (requireNamespace("data.table", quietly = TRUE)) {
    as.data.frame(data.table::fread(cmd = sprintf("unzip -p '%s' %s.txt", feed, name),
                                    colClasses = "character", select = select,
                                    showProgress = FALSE, encoding = "UTF-8"))
  } else {
    t <- utils::read.csv(unz(feed, paste0(name, ".txt")), colClasses = "character",
                         encoding = "UTF-8")
    if (is.null(select)) t else t[, select]
  }
}

# The feed in the cache: the file of the version the figures were computed
# with if it is there, or else the consortium's current file.
rmr_feed_file <- function(cache = rmr_cache()) {
  raw <- file.path(cache, "raw")
  pinned <- file.path(raw, sprintf("granderecife-%s.zip", rmr_feed$version))
  if (file.exists(pinned)) {
    sha <- unname(tools::sha256sum(pinned))
    if (sha != rmr_feed$sha256) warning(basename(pinned), ": sha256 ", sha, ", not the file the figures were computed with")
    return(pinned)
  }
  tmp <- fetch(rmr_feed$url, file.path(raw, "gtfs-download.zip"))
  version <- feed_table(tmp, "feed_info")$feed_version[1]
  dest <- file.path(raw, sprintf("granderecife-%s.zip", version))
  file.rename(tmp, dest)
  if (version != rmr_feed$version) {
    warning("the consortium now serves feed_version ", version, "; the figures were computed with ",
            rmr_feed$version, ": the numbers will differ")
  }
  dest
}

# The directory build_network() reads: the street cut of the 14
# municipalities with 3 km around them, and the feed.
prepare_rmr <- function(cache = rmr_cache(), pe = pe_cache()) {
  streets <- prepare_city("rmr", pe)
  feed <- rmr_feed_file(cache)
  net <- file.path(cache, "network")
  dir.create(net, showWarnings = FALSE)
  for (f in c(file.path(streets, "network", "rmr.osm.pbf"), feed)) {
    to <- file.path(net, basename(f))
    if (!file.exists(to) && !suppressWarnings(file.link(f, to))) file.copy(f, to)
  }
  # one feed only: a newer download would otherwise be read together with the old
  old <- setdiff(list.files(net, pattern = "\\.zip$"), basename(feed))
  if (length(old)) stop("more than one feed in ", net, ": ", paste(old, collapse = ", "))
  dir.create(file.path(cache, "results"), showWarnings = FALSE)
  invisible(list(cache = cache, network = net, streets = streets, feed = feed))
}

# What the feed has: counts by table, routes by mode, fares by price.
rmr_feed_summary <- function(feed) {
  routes <- feed_table(feed, "routes")
  stops <- feed_table(feed, "stops")
  trips <- feed_table(feed, "trips", select = c("route_id", "service_id", "trip_id"))
  info <- feed_table(feed, "feed_info")
  fares <- merge(feed_table(feed, "fare_rules"), feed_table(feed, "fare_attributes"), by = "fare_id")
  files <- utils::unzip(feed, list = TRUE)$Name
  mode <- c("0" = "light rail (VLT)", "1" = "metro", "3" = "bus")
  list(version = info$feed_version, start = info$feed_start_date, end = info$feed_end_date,
       publisher = info$feed_publisher_name,
       agencies = nrow(feed_table(feed, "agency")), routes = nrow(routes),
       by_mode = table(mode[routes$route_type]), stops = nrow(stops),
       stations = sum(stops$location_type == "1"), trips = nrow(trips),
       frequencies = "frequencies.txt" %in% files,
       fare_prices = table(sprintf("R$ %.2f", as.numeric(fares$price))),
       transfer_rules = sum(!is.na(suppressWarnings(as.numeric(fares$transfers)))),
       files = files)
}

# Scheduled speed of every trip that runs on a date: the length of its shape
# between its first and last stop (shape_dist_traveled, metres) where the
# feed gives it, over the time between the first departure and the last
# arrival. One row per trip: route, mode, hour of departure, km, minutes.
rmr_trip_speeds <- function(feed, date = as.Date("2026-11-10")) {
  cal <- feed_table(feed, "calendar")
  day <- c("sunday", "monday", "tuesday", "wednesday", "thursday", "friday",
           "saturday")[as.POSIXlt(date)$wday + 1]
  ymd <- format(date, "%Y%m%d")
  on <- cal$service_id[cal[[day]] == "1" & cal$start_date <= ymd & cal$end_date >= ymd]
  ex <- feed_table(feed, "calendar_dates")
  ex <- ex[ex$date == ymd, ]
  on <- union(setdiff(on, ex$service_id[ex$exception_type == "2"]), ex$service_id[ex$exception_type == "1"])
  trips <- feed_table(feed, "trips", select = c("route_id", "service_id", "trip_id", "shape_id"))
  trips <- trips[trips$service_id %in% on, ]
  st <- data.table::fread(cmd = sprintf("unzip -p '%s' stop_times.txt", feed), showProgress = FALSE,
                          select = c("trip_id", "arrival_time", "departure_time", "stop_id", "stop_sequence"),
                          colClasses = list(character = c("trip_id", "arrival_time", "departure_time", "stop_id")))
  st <- st[st$trip_id %in% trips$trip_id, ]
  secs <- function(x) {
    p <- data.table::tstrsplit(x, ":", fixed = TRUE)
    as.integer(p[[1]]) * 3600L + as.integer(p[[2]]) * 60L + as.integer(p[[3]])
  }
  st$arr <- secs(st$arrival_time)
  st$dep <- secs(st$departure_time)
  stops <- feed_table(feed, "stops", select = c("stop_id", "stop_lat", "stop_lon"))
  k <- match(st$stop_id, stops$stop_id)
  st$lat <- as.numeric(stops$stop_lat[k])
  st$lon <- as.numeric(stops$stop_lon[k])
  data.table::setorder(st, trip_id, stop_sequence)
  # the distance between consecutive stops, in a straight line (haversine):
  # a lower bound of the distance run, the same at every hour
  rad <- pi / 180
  hav <- function(lat1, lon1, lat2, lon2) {
    a <- sin((lat2 - lat1) * rad / 2)^2 + cos(lat1 * rad) * cos(lat2 * rad) * sin((lon2 - lon1) * rad / 2)^2
    2 * 6371008.8 * asin(sqrt(a))
  }
  st[, hop := c(0, hav(lat[-.N], lon[-.N], lat[-1], lon[-1])), by = trip_id]
  per <- st[, list(start = dep[1], end = arr[.N], stops = .N, straight_km = sum(hop) / 1000), by = trip_id]
  # the length of the trip's shape, where it has one
  sh <- data.table::fread(cmd = sprintf("unzip -p '%s' shapes.txt", feed), showProgress = FALSE,
                          select = c("shape_id", "shape_dist_traveled"),
                          colClasses = list(character = "shape_id"))
  len <- sh[, list(shape_km = max(shape_dist_traveled) / 1000), by = shape_id]
  out <- merge(as.data.frame(per), trips, by = "trip_id")
  out$shape_km <- len$shape_km[match(out$shape_id, len$shape_id)]
  routes <- feed_table(feed, "routes", select = c("route_id", "route_type"))
  out$mode <- c("0" = "VLT", "1" = "Metro", "3" = "Bus")[routes$route_type[match(out$route_id, routes$route_id)]]
  out$minutes <- (out$end - out$start) / 60
  out$hour <- out$start %/% 3600
  out
}

if (sys.nframe() == 0L) {
  p <- prepare_rmr()
  message("prepared ", p$network)
  str(rmr_feed_summary(p$feed))
}
