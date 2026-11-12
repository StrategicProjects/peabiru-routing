# detailed_itineraries()' McRAPTOR fallback against r5r 2.5.0: r5r plans a
# pair again with McRAPTOR when the path FastRaptorWorker found cannot be
# read back (FastDetailedItineraryPlanner; it logs "FastRaptor itinerary
# from ... failed ...; routing it with McRAPTOR"). r5r's test switch, the
# Java system property r5r.failFastRaptor, fails every FastRaptor trip, and
# peabiru's option peabiru.fail_fast_raptor does the same: with both set,
# every pair that reaches stops takes the fallback, compared with
# identical() (sf included; output_dir byte for byte).
#
#   PEABIRU_R_LIB=<lib> PEABIRU_BIN=<peabiru CLI> Rscript bench/compare/itinerary_fallback.R [n_pairs]
#
# Needs r5r, sf, a JDK 21 (JAVA_HOME). Porto Alegre's sample and, when
# PEABIRU_POA_CITY names a directory with poa_city.osm.pbf and
# points_700m.csv, the whole city with car egress. Also counts the pairs
# that take the fallback without the switch (peabiru: the CLI's report; r5r:
# its log, in this script's output). Exits with status 1 if anything differs.
# peabiru returns tibbles by default; this script works with data.frames.
options(peabiru.output = "data.frame")
args <- commandArgs(TRUE)
n <- if (length(args)) as.integer(args[1]) else 200L
lib <- Sys.getenv("PEABIRU_R_LIB")
if (nzchar(lib)) .libPaths(c(lib, .libPaths()))
if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx6G")
suppressMessages({ library(r5r); library(peabiru); library(sf) })
plain <- function(x) {
  class(x) <- setdiff(class(x), "data.table")
  attr(x, ".internal.selfref") <- NULL
  x
}
dep <- as.POSIXct("2019-05-13 14:00:00")
bin <- Sys.getenv("PEABIRU_BIN")
fail_switch <- function(on) {
  rJava::.jcall("java/lang/System", "S", "setProperty", "r5r.failFastRaptor", tolower(on))
  options(peabiru.fail_fast_raptor = on)
}
failed <- 0
cities <- list(poa = list(src = system.file("extdata/poa", package = "r5r"), pbf = "poa_osm.pbf",
                          points = "poa_hexgrid.csv"))
city <- Sys.getenv("PEABIRU_POA_CITY")
if (nzchar(city)) cities$city <- list(src = city, pbf = "poa_city.osm.pbf", points = "points_700m.csv")
for (name in names(cities)) {
  cfg <- cities[[name]]
  dir <- file.path(tempdir(), paste0("fallback-", name))
  unlink(dir, recursive = TRUE)
  dir.create(dir)
  file.copy(file.path(cfg$src, cfg$pbf), dir)
  file.copy(list.files(system.file("extdata/poa", package = "r5r"), "\\.zip$", full.names = TRUE), dir)
  r5 <- suppressMessages(r5r::build_network(dir, verbose = FALSE))
  pb <- peabiru::build_network(dir)
  pts <- read.csv(file.path(cfg$src, cfg$points))
  set.seed(5)
  o <- pts[sample(nrow(pts), n, TRUE), ]
  d <- pts[sample(nrow(pts), n, TRUE), ]
  o$id <- paste0("o", seq_len(n))
  d$id <- paste0("d", seq_len(n))
  # without the switch: how many pairs take the fallback
  if (nzchar(bin)) {
    po <- file.path(dir, "o.csv"); pd <- file.path(dir, "d.csv")
    write.csv(o[c("id", "lon", "lat")], po, row.names = FALSE)
    write.csv(d[c("id", "lon", "lat")], pd, row.names = FALSE)
    err <- system2(bin, c("itineraries", dir, po, pd, file.path(dir, "out.csv"), "--transit", "TRANSIT",
                          "--date", "2019-05-13", "--time", "14:00:00"), stdout = FALSE, stderr = TRUE)
    hit <- grep("planned again with McRAPTOR", err, value = TRUE)
    message(sprintf("%s: peabiru's CLI, no switch: %s", name, if (length(hit)) hit else "0 pairs take the fallback"))
  }
  message(sprintf("%s: r5r without the switch (count the 'routing it with McRAPTOR' lines below)", name))
  invisible(suppressWarnings(r5r::detailed_itineraries(r5, o, d, mode = c("WALK", "TRANSIT"),
    departure_datetime = dep, n_threads = 1, progress = FALSE)))
  message(sprintf("%s: end of r5r's run without the switch", name))
  scenarios <- list(
    plain = list(),
    links = list(osm_link_ids = TRUE),
    tw = list(time_window = 30, max_rides = 2),
    bus = list(mode = c("WALK", "BUS"), max_walk_time = 15),
    bike = list(mode = c("BICYCLE", "TRANSIT"), max_bike_time = 20)
  )
  if (name == "city") scenarios$car_egress <- list(mode_egress = "CAR")
  fail_switch(TRUE)
  for (s in names(scenarios)) {
    a <- c(list(origins = o, destinations = d, mode = c("WALK", "TRANSIT"), departure_datetime = dep),
           scenarios[[s]])
    a <- a[!duplicated(names(a), fromLast = TRUE)]
    r <- tryCatch(plain(suppressWarnings(do.call(r5r::detailed_itineraries,
                                                 c(list(r5), a, n_threads = 1, progress = FALSE)))),
                  error = function(e) e)
    q <- tryCatch(suppressWarnings(do.call(peabiru::detailed_itineraries, c(list(pb), a))),
                  error = function(e) e)
    ok <- if (inherits(r, "error") || inherits(q, "error")) {
      inherits(r, "error") && inherits(q, "error")
    } else identical(q, r)
    if (!ok) {
      failed <- failed + 1
      saveRDS(list(r = r, q = q), file.path(tempdir(), paste0("fallback-", name, "-", s, ".rds")))
    }
    message(sprintf("%s %-10s switch on: %s rows %s", name, s,
                    if (inherits(r, "error")) conditionMessage(r) else nrow(r),
                    if (ok) "identical" else "DIFFERENT"))
  }
  od <- file.path(tempdir(), paste0("fallback-od-", name, c("-r", "-p")))
  unlink(od, recursive = TRUE)
  for (x in od) dir.create(x)
  a <- list(origins = o[1:50, ], destinations = d[1:50, ], mode = c("WALK", "TRANSIT"), departure_datetime = dep)
  suppressWarnings(do.call(r5r::detailed_itineraries, c(list(r5), a, output_dir = od[1], progress = FALSE)))
  suppressWarnings(do.call(peabiru::detailed_itineraries, c(list(pb), a, output_dir = od[2])))
  f <- list.files(od[1])
  same <- identical(f, list.files(od[2])) && all(vapply(f, function(x) identical(
    readBin(file.path(od[1], x), "raw", 1e8), readBin(file.path(od[2], x), "raw", 1e8)), TRUE))
  if (!same) failed <- failed + 1
  message(sprintf("%s output_dir   switch on: %d files %s", name, length(f), if (same) "identical" else "DIFFERENT"))
  fail_switch(FALSE)
  r5r::stop_r5(r5)
}
quit(status = if (failed) 1 else 0)
