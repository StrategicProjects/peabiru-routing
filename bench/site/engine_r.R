# One benchmark task with an R engine (peabiru or r5r), called by
# bench/site/run.py. Both packages take the same calls: peabiru mirrors r5r's
# functions and arguments. Prints one JSON line:
#   {"build_s": .., "times": [..], "rows": .., "version": ".."}
#
# Rscript bench/site/engine_r.R --engine peabiru-r|r5r --city poa --dir DIR
#   --points CSV --task ttm_transit --threads 10 --reps 3 --params JSON
#   [--result out.csv]

# peabiru returns tibbles by default; these scripts work with data.frames.
options(peabiru.output = "data.frame")
args <- commandArgs(trailingOnly = TRUE)
opt <- function(name) {
  i <- match(paste0("--", name), args)
  if (is.na(i)) stop("missing --", name)
  args[[i + 1]]
}
engine <- opt("engine")
task <- opt("task")
threads <- as.integer(opt("threads"))
reps <- as.integer(opt("reps"))
P <- jsonlite::fromJSON(opt("params"))

if (engine == "r5r") {
  options(java.parameters = paste0("-Xmx", Sys.getenv("R5R_XMX", "6G")))
  suppressPackageStartupMessages(library(r5r))
  pkg <- asNamespace("r5r")
  version <- paste0("r5r ", utils::packageVersion("r5r"), " (R5 ",
                    sub("^r5-v([0-9.]+)-all\\.jar$", "\\1",
                        basename(r5r::download_r5(quiet = TRUE))), ")")
} else {
  if (!requireNamespace("peabiru", quietly = TRUE)) {
    cat(jsonlite::toJSON(list(skip = "the peabiru R package is not installed"),
                         auto_unbox = TRUE), "\n")
    quit(status = 0)
  }
  pkg <- asNamespace("peabiru")
  version <- paste0("peabiru ", utils::packageVersion("peabiru"),
                    " (core ", peabiru::core_version(), ")")
}
f <- function(name) get(name, envir = pkg)

points <- utils::read.csv(opt("points"), colClasses = c(id = "character"))
departure <- as.POSIXct(paste(P$date, P$time), tz = P$tz)
inf_or <- function(x) if (identical(x, "Inf")) Inf else as.numeric(x)

elapsed <- function(expr) {
  t0 <- proc.time()[["elapsed"]]
  force(expr)
  proc.time()[["elapsed"]] - t0
}

network <- NULL
build_args <- list(opt("dir"), verbose = FALSE)
if (engine == "r5r") build_args$overwrite <- TRUE else build_args$n_threads <- threads
build_s <- elapsed(network <- do.call(f("build_network"), build_args))

# the network goes first, unnamed: the argument is `r5r_network` in r5r and
# `network` in peabiru
common <- list(
  network, origins = points, destinations = points,
  departure_datetime = departure, max_trip_duration = P$max_trip_duration,
  walk_speed = P$walk_speed, bike_speed = P$bike_speed, max_lts = P$max_lts,
  n_threads = threads, progress = FALSE, verbose = FALSE
)
transit <- c(common, list(
  mode = c("WALK", "TRANSIT"), time_window = P$time_window,
  max_walk_time = inf_or(P$max_walk_time), max_rides = P$max_rides,
  draws_per_minute = P$draws_per_minute, seed = P$seed
))

call <- switch(task,
  ttm_walk = function() do.call(f("travel_time_matrix"), c(common, list(mode = "WALK"))),
  ttm_bicycle = function() do.call(f("travel_time_matrix"), c(common, list(mode = "BICYCLE"))),
  ttm_car = function() do.call(f("travel_time_matrix"), c(common, list(mode = "CAR"))),
  ttm_transit = function() do.call(f("travel_time_matrix"),
                                   c(transit, list(percentiles = P$percentiles))),
  accessibility = function() do.call(f("accessibility"), c(
    transit[names(transit) != "max_trip_duration"],
    list(opportunities_colnames = P$accessibility$opportunities,
         decay_function = P$accessibility$decay_function,
         cutoffs = P$accessibility$cutoffs, percentiles = P$percentiles,
         max_trip_duration = P$max_trip_duration))),
  expanded = function() {
    a <- transit
    a$origins <- points[seq_len(P$expanded$origins), ]
    do.call(f("expanded_travel_time_matrix"), c(a, list(breakdown = isTRUE(P$expanded$breakdown))))
  },
  stop("unknown task ", task)
)

res <- call() # warm-up
rows <- nrow(res)
times <- numeric(reps)
for (i in seq_len(reps)) times[i] <- elapsed(res <- call())
# --result: the matrix as from_id,to_id,travel_time for bench/site/run.py's
# comparison with peabiru (travel-time matrix tasks only)
i_res <- match("--result", args)
if (!is.na(i_res) && startsWith(task, "ttm_")) {
  tt <- grep("^travel_time", names(res), value = TRUE)[1]
  keep <- !is.na(res[[tt]])
  utils::write.csv(data.frame(from_id = res$from_id[keep], to_id = res$to_id[keep],
                              travel_time = res[[tt]][keep]),
                   args[[i_res + 1]], row.names = FALSE, quote = FALSE)
}
cat(jsonlite::toJSON(list(build_s = build_s, times = I(times), rows = rows, version = version),
                     auto_unbox = TRUE, digits = NA), "\n")
