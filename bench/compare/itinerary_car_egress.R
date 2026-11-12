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
# pair whose searches cross one may depend on what ran before; peabiru
# returns what r5r gives on a freshly built network for the pair alone with
# one thread, and warns, listing the pairs whose result may differ. Here:
# the pairs peabiru does not list must be identical to r5r's call of all 300
# pairs; each listed pair is compared with r5r on a fresh network (loaded
# again from network.dat, nothing routed before) for that pair alone,
# n_threads = 1. Exits with status 1 if any of these differs.

# peabiru returns tibbles by default; this script works with data.frames.
options(peabiru.output = "data.frame")
args <- commandArgs(TRUE)
# a seed followed by "s" (e.g. 3s) runs on a car speed scenario: 400 random
# ways at 0.4 to 1.5 times their speed
seeds <- if (length(args)) args else as.character(1:4)
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
args_for <- function(o, d) c(list(origins = o, destinations = d, mode = c("WALK", "TRANSIT"),
                                  mode_egress = "CAR", departure_datetime = dep), scen)
# peabiru's itineraries and the pairs (row numbers) its warning lists
peabiru_run <- function(o, d) {
  listed <- integer(0)
  q <- withCallingHandlers(
    do.call(peabiru::detailed_itineraries, c(list(pb), args_for(o, d))),
    warning = function(w) {
      m <- conditionMessage(w)
      if (grepl("turn restriction with two or more via ways", m)) {
        p <- regmatches(m, regexpr("\\(pairs? [0-9, .]+\\)", m))
        listed <<- as.integer(regmatches(p, gregexpr("[0-9]+", p))[[1]])
        if (grepl("...", p, fixed = TRUE)) listed <<- NA_integer_
      }
      invokeRestart("muffleWarning")
    })
  list(q = q, listed = listed)
}
rows_of <- function(x, id) {
  x <- x[x$from_id == id, ]
  rownames(x) <- NULL
  x
}
ways <- unique(street_network_to_sf(pb)$edges$osm_id)
failed <- 0
total_listed <- 0
total_pairs <- 0
for (tag in seeds) {
  seed <- as.integer(sub("s$", "", tag))
  scen <- if (grepl("s$", tag)) {
    set.seed(1000 + seed)
    list(new_carspeeds = data.frame(osm_id = sample(ways, 400), max_speed = runif(400, 0.4, 1.5),
                                    speed_type = "scale"))
  } else list()
  set.seed(seed)
  n <- 300
  o <- pts[sample(nrow(pts), n, TRUE), ]
  d <- pts[sample(nrow(pts), n, TRUE), ]
  o$id <- paste0("o", seq_len(n))
  d$id <- paste0("d", seq_len(n))
  r <- plain(suppressWarnings(do.call(r5r::detailed_itineraries,
                                      c(list(r5), args_for(o, d), progress = FALSE))))
  run <- peabiru_run(o, d)
  listed <- run$listed
  if (anyNA(listed)) {
    # more than the warning shows: find them pair by pair
    listed <- which(vapply(seq_len(n), function(i) length(peabiru_run(o[i, ], d[i, ])$listed) > 0,
                           TRUE))
  }
  others <- setdiff(seq_len(n), listed)
  same <- vapply(others, function(i) identical(rows_of(run$q, o$id[i]), rows_of(r, o$id[i])), TRUE)
  # each listed pair alone, on a fresh r5r network, one thread
  fresh_same <- vapply(listed, function(i) {
    f <- suppressMessages(r5r::build_network(dir, verbose = FALSE))
    on.exit(r5r::stop_r5(f))
    a <- args_for(o[i, ], d[i, ])
    ri <- plain(suppressWarnings(do.call(r5r::detailed_itineraries,
                                         c(list(f), a, n_threads = 1, progress = FALSE))))
    qi <- suppressWarnings(do.call(peabiru::detailed_itineraries, c(list(pb), a)))
    identical(qi, ri) && identical(rows_of(run$q, o$id[i]), qi)
  }, TRUE)
  in_call <- vapply(listed, function(i) identical(rows_of(run$q, o$id[i]), rows_of(r, o$id[i])), TRUE)
  # the same call again in r5r's session: only listed pairs may change
  r2 <- plain(suppressWarnings(do.call(r5r::detailed_itineraries,
                                       c(list(r5), args_for(o, d), progress = FALSE))))
  moved <- vapply(seq_len(n), function(i) !identical(rows_of(r, o$id[i]), rows_of(r2, o$id[i])), TRUE)
  if (!all(same) || !all(fresh_same) || any(moved[others])) failed <- failed + 1
  total_listed <- total_listed + length(listed)
  total_pairs <- total_pairs + n
  message(sprintf(paste0("seed %s: %d rows; %d pairs not listed: %d identical; %d listed (%s): ",
                         "%d identical to r5r fresh, %d also equal in r5r's call of all pairs; ",
                         "r5r's second call changed %d listed and %d other pairs"),
                  tag, nrow(r), length(others), sum(same), length(listed),
                  paste(listed, collapse = ","), sum(fresh_same), sum(in_call),
                  sum(moved[listed]), sum(moved[others])))
}
message(sprintf("listed %d of %d pairs", total_listed, total_pairs))
r5r::stop_r5(r5)
if (failed > 0) quit(status = 1)
