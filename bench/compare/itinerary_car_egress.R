# Car egress itineraries on Porto Alegre city (1354 turn restrictions) against
# r5r 2.5.0, with identical() (sf included).
#
#   PEABIRU_R_LIB=<lib with peabiru> Rscript bench/compare/itinerary_car_egress.R [seeds...]
#
# Needs r5r, a JDK 21 (JAVA_HOME), bench/data/poa-city/poa_city.osm.pbf (the
# city of Porto Alegre cut with osmium from Geofabrik's sul extract; not in
# git) with points_700m.csv (a 700 m grid of id, lon, lat), and r5r's two
# Porto Alegre GTFS feeds (PEABIRU_POA_CITY overrides the directory). Each
# seed draws 300 random pairs of grid points; mode WALK + TRANSIT,
# mode_egress CAR, 2019-05-13 14:00.
#
# R5 reverses the via edges of a turn restriction with two or more of them
# in place on every check of a backward car search, so r5r's result for a
# pair whose searches cross one may depend on what ran before; peabiru then
# tries every order and refuses the pair when the itineraries differ. When a
# call is refused, the pairs are compared one by one and the refused ones
# listed. Exits with status 1 if any pair peabiru routes differs.

args <- commandArgs(TRUE)
seeds <- if (length(args)) as.integer(args) else 1:4
lib <- Sys.getenv("PEABIRU_R_LIB")
if (nzchar(lib)) .libPaths(c(lib, .libPaths()))
if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx6G")
suppressMessages({
  library(r5r)
  library(peabiru)
  library(sf)
})
city <- Sys.getenv("PEABIRU_POA_CITY", file.path("bench", "data", "poa-city"))
dir <- file.path(tempdir(), "itin-car-egress")
unlink(dir, recursive = TRUE)
dir.create(dir)
file.copy(file.path(city, "poa_city.osm.pbf"), dir)
file.copy(list.files(system.file("extdata/poa", package = "r5r"), "\\.zip$", full.names = TRUE), dir)
r5 <- suppressMessages(r5r::build_network(dir, verbose = FALSE))
pb <- peabiru::build_network(dir)
pts <- read.csv(file.path(city, "points_700m.csv"))
dep <- as.POSIXct("2019-05-13 14:00:00")
plain <- function(x) {
  class(x) <- setdiff(class(x), "data.table")
  attr(x, ".internal.selfref") <- NULL
  x
}
args_for <- function(o, d) list(origins = o, destinations = d, mode = c("WALK", "TRANSIT"),
                                mode_egress = "CAR", departure_datetime = dep)
failed <- 0
for (seed in seeds) {
  set.seed(seed)
  n <- 300
  o <- pts[sample(nrow(pts), n, TRUE), ]
  d <- pts[sample(nrow(pts), n, TRUE), ]
  o$id <- paste0("o", seq_len(n))
  d$id <- paste0("d", seq_len(n))
  r <- plain(suppressWarnings(do.call(r5r::detailed_itineraries,
                                      c(list(r5), args_for(o, d), progress = FALSE))))
  q <- tryCatch(do.call(peabiru::detailed_itineraries, c(list(pb), args_for(o, d))),
                error = function(e) e)
  if (!inherits(q, "error")) {
    ok <- identical(q, r)
    if (!ok) failed <- failed + 1
    message(sprintf("seed %d: %d rows  %s", seed, nrow(r), if (ok) "identical" else "DIFFERENT"))
    next
  }
  refused <- integer(0)
  same <- 0
  differ <- 0
  for (i in seq_len(n)) {
    qi <- tryCatch(do.call(peabiru::detailed_itineraries, c(list(pb), args_for(o[i, ], d[i, ]))),
                   error = function(e) e)
    if (inherits(qi, "error")) {
      refused <- c(refused, i)
      next
    }
    ri <- r[r$from_id == o$id[i], ]
    rownames(ri) <- NULL
    if (identical(qi, ri)) same <- same + 1 else differ <- differ + 1
  }
  if (differ > 0) failed <- failed + 1
  message(sprintf("seed %d: refused pairs %s; the other %d pairs: %d identical, %d different",
                  seed, paste(refused, collapse = ","), n - length(refused), same, differ))
}
r5r::stop_r5(r5)
if (failed > 0) quit(status = 1)
