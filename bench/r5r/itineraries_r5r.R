# Random origin-destination pairs and r5r's detailed_itineraries() between
# them as CSV, for bench/compare/itinerary_csv.py. Copy only the .pbf and
# .zip files of r5r's sample into the directory first (its extdata also has a
# .tif and a cached network.dat).
# Rscript bench/r5r/itineraries_r5r.R <r5r dir> <points.csv> <n pairs> <seed> <outprefix> <mode...> [key=value ...]
# writes <outprefix>_o.csv, <outprefix>_d.csv (paired) and <outprefix>_r5r.csv
if (!nzchar(Sys.getenv("JAVA_HOME"))) stop("set JAVA_HOME to a JDK 21")
options(java.parameters = "-Xmx6G")
a <- commandArgs(TRUE)
dir <- a[1]; pts_file <- a[2]; n <- as.integer(a[3]); seed <- as.integer(a[4]); pre <- a[5]
mode <- strsplit(a[6], ",")[[1]]
kv <- list()
for (x in a[-(1:6)]) { p <- strsplit(x, "=")[[1]]; kv[[p[1]]] <- p[2] }
num <- function(k, d) if (is.null(kv[[k]])) d else as.numeric(kv[[k]])
suppressMessages(library(r5r))
net <- build_network(dir, verbose = FALSE)
pts <- read.csv(pts_file)
set.seed(seed)
bb <- c(range(pts$lat), range(pts$lon))
rnd <- function(k) data.frame(lat = runif(k, bb[1], bb[2]), lon = runif(k, bb[3], bb[4]))
k1 <- floor(n * 0.6); k2 <- n - k1
o <- rbind(rnd(k1), pts[sample(nrow(pts), k2, TRUE), c("lat", "lon")])
d <- rbind(rnd(k1), pts[sample(nrow(pts), k2, TRUE), c("lat", "lon")])
# equal points, and points far off the network
eq <- sample(n, max(1, n %/% 50)); d[eq, ] <- o[eq, ]
off <- sample(n, max(1, n %/% 100)); o[off, "lat"] <- o[off, "lat"] + 0.5
o$id <- paste0("o", seq_len(n)); d$id <- paste0("d", seq_len(n))
w17 <- function(x, f) writeLines(c("id,lat,lon", sprintf("%s,%.17g,%.17g", x$id, x$lat, x$lon)), f)
w17(o, paste0(pre, "_o.csv")); w17(d, paste0(pre, "_d.csv"))
dt <- as.POSIXct(if (is.null(kv$time)) "2019-05-13 14:00:00" else kv$time, format = "%Y-%m-%d %H:%M:%S")
t0 <- Sys.time()
r <- detailed_itineraries(net, o, d, mode = mode, mode_egress = if (is.null(kv$egress)) "WALK" else kv$egress,
  departure_datetime = dt, time_window = num("tw", 10L), max_walk_time = num("max_walk", Inf),
  max_bike_time = num("max_bike", Inf), max_car_time = num("max_car", Inf),
  max_trip_duration = num("max_trip", 120L), walk_speed = num("walk_speed", 3.6),
  bike_speed = num("bike_speed", 12), max_rides = num("max_rides", 3), max_lts = num("max_lts", 2),
  shortest_path = TRUE, n_threads = 4, progress = FALSE)
message("r5r seconds: ", format(as.numeric(Sys.time() - t0, units = "secs")))
g <- sf::st_geometry(r)
wkt <- vapply(g, function(l) {
  m <- unclass(l)
  if (length(m) == 0) return("LINESTRING EMPTY")
  paste0("LINESTRING (", paste(sprintf("%.17g %.17g", m[, 1], m[, 2]), collapse = ", "), ")")
}, "")
r <- sf::st_drop_geometry(r)
out <- data.frame(from_id = r$from_id, to_id = r$to_id, option = r$option, departure_time = r$departure_time,
  total_duration = sprintf("%.17g", r$total_duration), total_distance = r$total_distance, segment = r$segment,
  mode = r$mode, segment_duration = sprintf("%.17g", r$segment_duration), wait = sprintf("%.17g", r$wait),
  distance = r$distance, route = r$route, geometry = wkt)
write.csv(out, paste0(pre, "_r5r.csv"), row.names = FALSE, quote = c(13))
message("rows ", nrow(out))
