# Rides along GTFS shapes cut by shape_dist_traveled: detailed_itineraries()
# (plain, osm_link_ids, a public transport mode, alternatives, fares,
# output_dir with geometry) and transit_network_to_sf() of peabiru against
# r5r 2.5.0, with identical() (output_dir: the files byte for byte).
#
#   PEABIRU_R_LIB=<lib with peabiru> Rscript bench/compare/shape_dist_parity.R [variant...]
#
# Needs r5r, sf, a JDK 21 (JAVA_HOME) and python3. The feeds are r5r's two
# Porto Alegre feeds with distances added by shape_dist_feeds.py (variants:
# m, km, mixed, shapes_only, stops_only, gaps, errors, nan, words, bad; all
# by default). N_PAIRS (default 150) random pairs of hexagon centres,
# 2019-05-13 14:00, one thread. Exits with status 1 if anything differs.
# peabiru returns tibbles by default; this script works with data.frames.
options(peabiru.output = "data.frame")
variants <- commandArgs(TRUE)
lib <- Sys.getenv("PEABIRU_R_LIB")
if (nzchar(lib)) .libPaths(c(lib, .libPaths()))
if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx6G")
suppressMessages({ library(r5r); library(peabiru); library(sf) })
n <- as.integer(Sys.getenv("N_PAIRS", "150"))
plain <- function(x) {
  class(x) <- setdiff(class(x), "data.table")
  attr(x, ".internal.selfref") <- NULL
  x
}
src <- system.file("extdata/poa", package = "r5r")
feeds <- file.path(tempdir(), "shape-dist-feeds")
here <- dirname(sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE)))
st <- system2("python3", c(shQuote(file.path(here, "shape_dist_feeds.py")), shQuote(src),
                           shQuote(feeds), variants))
if (st != 0) stop("shape_dist_feeds.py failed")
if (!length(variants)) variants <- list.files(feeds)
pts <- read.csv(file.path(src, "poa_hexgrid.csv"))
dep <- as.POSIXct("2019-05-13 14:00:00")
total <- 0; bad <- 0; rows <- 0
report <- function(v, tag, r, q) {
  total <<- total + 1
  if (inherits(r, "error") || inherits(q, "error")) {
    same <- inherits(r, "error") && inherits(q, "error")
    message(sprintf("%-12s %-14s r5r: %s | peabiru: %s", v, tag,
                    if (inherits(r, "error")) conditionMessage(r) else "ok",
                    if (inherits(q, "error")) conditionMessage(q) else "ok"))
    if (!same) bad <<- bad + 1
    return(invisible())
  }
  ok <- identical(q, r)
  rows <<- rows + nrow(r)
  if (!ok) {
    bad <<- bad + 1
    saveRDS(list(r = r, q = q), file.path(tempdir(), paste0("diff-", v, "-", tag, ".rds")))
  }
  message(sprintf("%-12s %-14s %5d rows %s", v, tag, nrow(r), if (ok) "identical" else "DIFFERENT"))
}
for (v in variants) {
  dir <- file.path(tempdir(), paste0("sdt-", v))
  unlink(dir, recursive = TRUE); dir.create(dir)
  file.copy(file.path(src, "poa_osm.pbf"), dir)
  file.copy(list.files(file.path(feeds, v), "\\.zip$", full.names = TRUE), dir)
  r5 <- tryCatch(suppressMessages(r5r::build_network(dir, verbose = FALSE)), error = function(e) e)
  pb <- tryCatch(peabiru::build_network(dir), error = function(e) e)
  if (inherits(r5, "error") || inherits(pb, "error")) { report(v, "build", r5, pb); next }
  set.seed(42)
  o <- pts[sample(nrow(pts), n, TRUE), ]; d <- pts[sample(nrow(pts), n, TRUE), ]
  o$id <- paste0("o", seq_len(n)); d$id <- paste0("d", seq_len(n))
  base <- list(origins = o, destinations = d, mode = c("WALK", "TRANSIT"),
               departure_datetime = dep, max_trip_duration = 120L, n_threads = 1)
  run <- function(tag, extra = list()) {
    a <- base; a[names(extra)] <- extra
    r <- tryCatch(plain(suppressWarnings(do.call(r5r::detailed_itineraries,
                                                 c(list(r5), a, progress = FALSE)))),
                  error = function(e) e)
    q <- tryCatch(suppressWarnings(do.call(peabiru::detailed_itineraries, c(list(pb), a))),
                  error = function(e) e)
    report(v, tag, r, q)
  }
  run("plain")
  run("links", list(osm_link_ids = TRUE))
  run("bus", list(mode = c("WALK", "BUS"), max_rides = 2))
  run("alt", list(shortest_path = FALSE, origins = o[1:40, ], destinations = d[1:40, ]))
  fs <- r5r::setup_fare_structure(r5, base_fare = 4.8, by = "MODE")
  run("fares", list(shortest_path = FALSE, fare_structure = fs, max_fare = 10,
                    origins = o[1:40, ], destinations = d[1:40, ]))
  # output_dir with geometry: files byte for byte
  od1 <- file.path(tempdir(), paste0("od-r-", v)); od2 <- file.path(tempdir(), paste0("od-p-", v))
  unlink(c(od1, od2), recursive = TRUE); dir.create(od1); dir.create(od2)
  a <- base; a$origins <- o[1:60, ]; a$destinations <- d[1:60, ]
  r <- tryCatch(suppressWarnings(do.call(r5r::detailed_itineraries, c(list(r5), a, output_dir = od1, progress = FALSE))), error = function(e) e)
  q <- tryCatch(suppressWarnings(do.call(peabiru::detailed_itineraries, c(list(pb), a, output_dir = od2))), error = function(e) e)
  f1 <- sort(list.files(od1)); f2 <- sort(list.files(od2))
  same <- identical(f1, f2) && all(vapply(f1, function(f) identical(
    readBin(file.path(od1, f), "raw", 1e8), readBin(file.path(od2, f), "raw", 1e8)), TRUE))
  total <- total + 1
  if (!same || inherits(q, "error")) bad <- bad + 1
  message(sprintf("%-12s %-14s %d files %s", v, "output_dir", length(f1), if (same && !inherits(q, "error")) "identical" else "DIFFERENT"))
  # transit network
  r <- tryCatch(r5r::transit_network_to_sf(r5), error = function(e) e)
  q <- tryCatch(peabiru::transit_network_to_sf(pb), error = function(e) e)
  if (!inherits(r, "error")) r <- lapply(r, plain)
  ok <- identical(r, q); total <- total + 1; if (!ok) bad <- bad + 1
  message(sprintf("%-12s %-14s %s", v, "transit_sf", if (ok) "identical" else "DIFFERENT"))
  rm(r5); gc()
}
message(sprintf("TOTAL %d comparisons, %d rows, %d differ", total, rows, bad))
quit(status = if (bad) 1 else 0)
