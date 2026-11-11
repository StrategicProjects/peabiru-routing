# Fares in peabiru against r5r 2.5.0, with identical(): the fare structure
# functions, fare_structure and max_fare in travel_time_matrix() and
# accessibility(), pareto_frontier(), and detailed_itineraries() with fares
# and alternatives (shortest_path = FALSE, suboptimal_minutes).
#
#   PEABIRU_R_LIB=<lib with peabiru> Rscript bench/compare/fares_parity.R [poa] [spo] [city] [n_origins]
#
# Needs r5r, a JDK 21 (JAVA_HOME) and r5r's sample data; `city` is Porto
# Alegre inside its limits (bench/data/poa-city, with r5r's two Porto Alegre
# feeds). Each network's .pbf and GTFS .zip files are copied into a fresh
# directory (r5r's extdata also holds a .tif and a cached network.dat). The
# origins and destinations are random sample points, shuffled; Porto Alegre
# uses the fare structure r5r ships (fares_poa.zip) and edits of it, Sao
# Paulo one made with setup_fare_structure() (its feed is frequency-based:
# every run passes a seed). Exits with status 1 if any scenario differs.

args <- commandArgs(TRUE)
n_o <- suppressWarnings(as.integer(args[grepl("^[0-9]+$", args)]))
n_o <- if (length(n_o) == 1) n_o else 30L
cities <- intersect(args, c("poa", "spo", "city"))
if (length(cities) == 0) cities <- c("poa", "spo")
lib <- Sys.getenv("PEABIRU_R_LIB")
if (nzchar(lib)) .libPaths(c(lib, .libPaths()))
if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx8G")
suppressMessages({
  library(data.table)
  library(peabiru)
})
root <- normalizePath(file.path(dirname(sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE))), "..", ".."))

plain <- function(x) {
  if (is.data.frame(x)) {
    class(x) <- setdiff(class(x), "data.table")
    attr(x, ".internal.selfref") <- NULL
    if (ncol(x) == 0) x <- data.frame()
    return(x)
  }
  if (is.list(x)) return(lapply(x, plain))
  x
}

ok <- 0L; bad <- 0L; rows <- 0L
same <- function(name, a, b) {
  if (inherits(a, "error") || inherits(b, "error")) {
    both <- inherits(a, "error") && inherits(b, "error")
    if (both) ok <<- ok + 1L else bad <<- bad + 1L
    cat(sprintf("%-46s %s\n", name, if (both) "both errors" else "ONE ERROR"))
    if (!both) cat("  peabiru:", conditionMessage(a)[1], "\n  r5r:    ", conditionMessage(b)[1], "\n")
    return(invisible())
  }
  r <- identical(plain(a), plain(b))
  if (r) ok <<- ok + 1L else {
    bad <<- bad + 1L
    print(all.equal(plain(a), plain(b)))
  }
  rows <<- rows + NROW(a)
  cat(sprintf("%-46s %s (%d rows)\n", name, if (r) "identical" else "DIFFERENT", NROW(a)))
}
both <- function(name, fun, ...) {
  a <- tryCatch(getExportedValue("peabiru", fun)(pn, ...), error = identity)
  b <- tryCatch(getExportedValue("r5r", fun)(rn, ..., progress = FALSE), error = identity)
  same(name, a, b)
}

ext <- system.file("extdata", package = "r5r")
t14 <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")
t1430 <- as.POSIXct("2019-05-13 14:30:00", tz = "America/Sao_Paulo")
fp <- r5r::read_fare_structure(file.path(ext, "poa/fares/fares_poa.zip"))
edit <- function(s, expr) { s <- copy(s); eval(substitute(expr)); s }

for (city in cities) {
  w <- tempfile(paste0("fares-", city)); dir.create(w)
  if (city == "city") {
    file.copy(c(file.path(root, "bench/data/poa-city/poa_city.osm.pbf"),
                list.files(file.path(ext, "poa"), pattern = "\\.zip$", full.names = TRUE)), w)
    pts <- fread(file.path(root, "bench/data/poa-city/points_700m.csv"))
  } else {
    file.copy(list.files(file.path(ext, city), pattern = "\\.(pbf|zip)$", full.names = TRUE), w)
    pts <- fread(file.path(ext, city, paste0(city, "_hexgrid.csv")))
  }
  rn <- suppressMessages(r5r::build_network(w, verbose = FALSE))
  pn <- peabiru::build_network(w)
  set.seed(nchar(city) * 77)
  o <- pts[sample(nrow(pts), n_o)]
  d <- pts[sample(nrow(pts), min(nrow(pts), 400))]
  cat("==", city, nrow(o), "origins,", nrow(d), "destinations\n")

  # the fare structure functions
  for (by in c("MODE", "AGENCY_ID", "AGENCY_NAME", "GENERIC")) for (bf in c(4.8, 0, 10 / 3)) {
    same(paste(city, "setup", by, round(bf, 2)),
         tryCatch(setup_fare_structure(pn, bf, by = by), error = identity),
         tryCatch(r5r::setup_fare_structure(rn, bf, by = by), error = identity))
  }
  if (city %in% c("poa", "city")) {
    fs <- fp
    fs_cap <- edit(fp, s$fare_cap <- 6)
    fs_allow <- edit(fp, { s$transfer_time_allowance <- 15; s$max_discounted_transfers <- 2 })
    fs_unl <- edit(fp, { s$fares_per_type[type == "BUS", unlimited_transfers := TRUE]; s$fares_per_type[type == "RAIL", unlimited_transfers := FALSE] })
    fs_route <- edit(fp, { s$fares_per_type[type == "BUS", `:=`(use_route_fare = TRUE, allow_same_route_transfer = TRUE)]; s$fares_per_route[mode == "BUS", route_fare := 3 + (seq_len(.N) %% 4) * 0.55] })
  } else {
    fs <- r5r::setup_fare_structure(rn, 4.4, by = "MODE")
    fs_cap <- edit(fs, { s$fare_cap <- 7; s$fares_per_transfer$fare <- 6.6 })
    fs_allow <- edit(fs, { s$transfer_time_allowance <- 20; s$fares_per_transfer$fare <- 5.5 })
    fs_unl <- edit(fs, s$fares_per_type$unlimited_transfers <- TRUE)
    fs_route <- edit(fs, { s$fares_per_type$use_route_fare <- TRUE; s$fares_per_route$route_fare <- 2 + (seq_len(nrow(s$fares_per_route)) %% 5) })
  }
  zr <- tempfile(fileext = ".zip"); zp <- tempfile(fileext = ".zip")
  r5r::write_fare_structure(fs_route, zr); write_fare_structure(fs_route, zp)
  dr <- tempfile(); dp <- tempfile(); unzip(zr, exdir = dr); unzip(zp, exdir = dp)
  files <- sort(list.files(dr))
  same(paste(city, "write: the CSV bytes"),
       lapply(files, function(f) readBin(file.path(dp, f), "raw", 1e7)),
       lapply(files, function(f) readBin(file.path(dr, f), "raw", 1e7)))
  same(paste(city, "read r5r's file"), read_fare_structure(zr), r5r::read_fare_structure(zr))
  same(paste(city, "r5r reads peabiru's file"), r5r::read_fare_structure(zp), r5r::read_fare_structure(zr))

  sd <- if (city == "spo") 42L else NULL
  M <- c("WALK", "TRANSIT")
  both(paste(city, "ttm fares, no limit"), "travel_time_matrix", o, d, mode = M, departure_datetime = t14, fare_structure = fs, draws_per_minute = 1, seed = sd)
  both(paste(city, "ttm max_fare 4.8, 3 percentiles"), "travel_time_matrix", o, d, mode = M, departure_datetime = t14, fare_structure = fs, max_fare = 4.8, draws_per_minute = 2, seed = sd, percentiles = c(10, 50, 90))
  both(paste(city, "ttm max_fare 7.2, 14:30"), "travel_time_matrix", o, d, mode = M, departure_datetime = t1430, fare_structure = fs, max_fare = 7.2, time_window = 12, draws_per_minute = 3, seed = sd, max_trip_duration = 80)
  both(paste(city, "ttm bicycle access, max_fare 4.5"), "travel_time_matrix", o, d, mode = c("BICYCLE", "TRANSIT"), departure_datetime = t14, fare_structure = fs, max_fare = 4.5, time_window = 8, draws_per_minute = 1, seed = sd)
  both(paste(city, "ttm max_fare 0"), "travel_time_matrix", o, d, mode = M, departure_datetime = t14, fare_structure = fs, max_fare = 0, time_window = 5, draws_per_minute = 1, seed = sd)
  both(paste(city, "ttm fare cap"), "travel_time_matrix", o, d, mode = M, departure_datetime = t14, fare_structure = fs_cap, max_fare = 6, draws_per_minute = 1, seed = sd, max_rides = 4)
  both(paste(city, "ttm transfer allowance"), "travel_time_matrix", o, d, mode = M, departure_datetime = t14, fare_structure = fs_allow, max_fare = 8.37, draws_per_minute = 1, seed = sd)
  both(paste(city, "ttm unlimited transfers"), "travel_time_matrix", o, d, mode = M, departure_datetime = t14, fare_structure = fs_unl, max_fare = 4.8, draws_per_minute = 1, seed = sd)
  both(paste(city, "ttm route fares"), "travel_time_matrix", o, d, mode = M, departure_datetime = t14, fare_structure = fs_route, max_fare = 5.2, draws_per_minute = 1, seed = sd)
  both(paste(city, "ttm WALK with fares"), "travel_time_matrix", o, d, mode = "WALK", departure_datetime = t14, fare_structure = fs, max_fare = 5)
  both(paste(city, "ttm 200 draws"), "travel_time_matrix", o, d, mode = M, departure_datetime = t14, fare_structure = fs, time_window = 40, draws_per_minute = 5)
  missing <- edit(fs, s$fares_per_route <- s$fares_per_route[-1])
  both(paste(city, "ttm route missing from the structure"), "travel_time_matrix", o, d, mode = M, departure_datetime = t14, fare_structure = missing)
  d2 <- copy(d); d2$jobs <- round(runif(nrow(d2), 0, 50)); d2$schools <- rpois(nrow(d2), 2)
  both(paste(city, "accessibility, step"), "accessibility", o, d2, opportunities_colnames = c("jobs", "schools"), mode = M, departure_datetime = t14, fare_structure = fs, max_fare = 4.8, cutoffs = c(20, 45, 60), draws_per_minute = 1, seed = sd)
  both(paste(city, "accessibility, logistic"), "accessibility", o, d2, opportunities_colnames = "jobs", decay_function = "logistic", decay_value = 8, cutoffs = c(30, 50), mode = M, departure_datetime = t14, fare_structure = fs_cap, max_fare = 6, time_window = 6, draws_per_minute = 2, percentiles = c(25, 75), seed = sd)
  both(paste(city, "pareto"), "pareto_frontier", o[1:min(n_o, 15)], d, mode = M, departure_datetime = t14, fare_structure = fs, fare_cutoffs = c(4.5, 4.8, 9, 15), seed = sd)
  both(paste(city, "pareto, cutoffs out of order"), "pareto_frontier", o[1:min(n_o, 12)], d, mode = M, departure_datetime = t1430, fare_structure = fs_allow, fare_cutoffs = c(9.6, 0, 4.8, 8.37, 7.2, Inf), percentiles = c(5, 50, 95), time_window = 7, seed = sd, max_trip_duration = 90)
  both(paste(city, "pareto, WALK"), "pareto_frontier", o[1:5], d, mode = "WALK", departure_datetime = t14, fare_structure = fs, fare_cutoffs = c(1, 2))
  oo <- o[1:min(n_o, 25)]; dd <- d[1:min(n_o, 25)]
  both(paste(city, "itineraries, fares, fastest"), "detailed_itineraries", oo, dd, mode = M, departure_datetime = t14, fare_structure = fs, drop_geometry = TRUE)
  both(paste(city, "itineraries, fares, alternatives"), "detailed_itineraries", oo, dd, mode = M, departure_datetime = t14, fare_structure = fs, max_fare = 9, shortest_path = FALSE, drop_geometry = TRUE)
  both(paste(city, "itineraries, alternatives"), "detailed_itineraries", oo, dd, mode = M, departure_datetime = t14, shortest_path = FALSE, drop_geometry = TRUE)
  both(paste(city, "itineraries, suboptimal 5, geometry"), "detailed_itineraries", oo, dd, mode = M, departure_datetime = t1430, shortest_path = FALSE, suboptimal_minutes = 5, time_window = 15)
  both(paste(city, "itineraries, bicycle, suboptimal 10"), "detailed_itineraries", oo, dd, mode = c("BICYCLE", "TRANSIT"), departure_datetime = t14, shortest_path = FALSE, suboptimal_minutes = 10, max_bike_time = 15, drop_geometry = TRUE)
  both(paste(city, "itineraries, fare cap, geometry"), "detailed_itineraries", oo, dd, mode = M, departure_datetime = t14, fare_structure = fs_cap, shortest_path = FALSE)
}
cat(sprintf("\n%d identical (or both errors), %d different; %d rows compared\n", ok, bad, rows))
if (bad > 0) quit(status = 1)
