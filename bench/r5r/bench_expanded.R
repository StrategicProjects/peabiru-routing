# r5r 2.5.0 baselines for expanded_travel_time_matrix() and
# arrival_travel_time_matrix() with mode = c("WALK", "TRANSIT").
#
# Usage (from the repository root; JAVA_HOME must point to a JDK 21, and
# bench/r5r/bench_transit.R must have built bench/data/r5r/<city>-transit/):
#   Rscript bench/r5r/bench_expanded.R poa spo
#
# Each setting is run twice to check that r5r's output is deterministic.
# Outputs (bench/results/r5r/), never overwriting the bench_transit.R ones:
#   <city>_TRANSIT_expanded_nobd.csv  expanded, breakdown = FALSE, the
#                                     parameters of <city>_TRANSIT_expanded.csv
#   <city>_TRANSIT_expanded_v2.csv    expanded, breakdown = TRUE, departure
#                                     07:30:30, time_window 5, 3 draws/min,
#                                     max_trip 60, max_walk 15, max_rides 2,
#                                     seed 7
#   <city>_TRANSIT_arrival.csv        arrival at 16:00, breakdown = TRUE,
#                                     r5r defaults (max_trip 120), seed 42
#   <city>_TRANSIT_arrival_v2.csv     arrival at 08:00, breakdown = FALSE,
#                                     max_trip 60, max_walk 15, max_rides 2,
#                                     seed 7
#   <city>_TRANSIT_arrival_midnight.csv  arrival at 00:30 on Tuesday, so the
#                                     window starts at 22:30 on Monday and
#                                     departures after midnight read 24:MM:SS;
#                                     breakdown = TRUE, defaults, seed 42,
#                                     first 50 origins
#   expanded_determinism.csv          rerun comparison per output
# Expanded runs use the first 50 origins (file order) x all destinations;
# arrival runs the first N_ARRIVAL_ORIGINS (default 200) x all.
# SETTINGS (comma-separated names) runs only those settings; an existing
# output file is never overwritten (the script stops instead).

options(java.parameters = paste0("-Xmx", Sys.getenv("R5R_XMX", "3G")))

suppressPackageStartupMessages({
  library(r5r)
  library(data.table)
})

args <- commandArgs(trailingOnly = TRUE)
cities <- if (length(args) > 0) args else c("poa", "spo")
stopifnot(all(cities %in% c("poa", "spo")))

script_arg <- grep("^--file=", commandArgs(trailingOnly = FALSE), value = TRUE)
script_dir <- if (length(script_arg) > 0) {
  dirname(normalizePath(sub("^--file=", "", script_arg[1])))
} else {
  getwd()
}
repo_root <- normalizePath(file.path(script_dir, "..", ".."))
# PEABIRU_ROOT points elsewhere (e.g. the main checkout from a worktree).
repo_root <- Sys.getenv("PEABIRU_ROOT", repo_root)
data_root <- file.path(repo_root, "bench", "data", "r5r")
out_dir <- file.path(repo_root, "bench", "results", "r5r")
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)

n_threads <- as.integer(Sys.getenv("R5R_THREADS", "4"))
n_expanded_origins <- 50L
n_arrival_origins <- as.integer(Sys.getenv("N_ARRIVAL_ORIGINS", "200"))
tz <- "America/Sao_Paulo"
common <- list(mode = c("WALK", "TRANSIT"), walk_speed = 3.6,
               n_threads = n_threads, progress = FALSE, verbose = FALSE)
settings <- list(
  expanded_nobd = list(
    fun = expanded_travel_time_matrix, n = n_expanded_origins,
    args = list(departure_datetime = as.POSIXct("2019-05-13 14:00:00", tz = tz),
                time_window = 10L, breakdown = FALSE, max_walk_time = Inf,
                max_trip_duration = 120L, max_rides = 3, draws_per_minute = 5L,
                seed = 42L)),
  expanded_v2 = list(
    fun = expanded_travel_time_matrix, n = n_expanded_origins,
    args = list(departure_datetime = as.POSIXct("2019-05-13 07:30:30", tz = tz),
                time_window = 5L, breakdown = TRUE, max_walk_time = 15L,
                max_trip_duration = 60L, max_rides = 2, draws_per_minute = 3L,
                seed = 7L)),
  arrival = list(
    fun = arrival_travel_time_matrix, n = n_arrival_origins,
    args = list(arrival_datetime = as.POSIXct("2019-05-13 16:00:00", tz = tz),
                breakdown = TRUE, max_walk_time = Inf, max_trip_duration = 120L,
                max_rides = 3, draws_per_minute = 5L, seed = 42L)),
  arrival_v2 = list(
    fun = arrival_travel_time_matrix, n = n_arrival_origins,
    args = list(arrival_datetime = as.POSIXct("2019-05-13 08:00:00", tz = tz),
                breakdown = FALSE, max_walk_time = 15L, max_trip_duration = 60L,
                max_rides = 2, draws_per_minute = 5L, seed = 7L)),
  arrival_midnight = list(
    fun = arrival_travel_time_matrix, n = n_expanded_origins,
    args = list(arrival_datetime = as.POSIXct("2019-05-14 00:30:00", tz = tz),
                breakdown = TRUE, max_walk_time = Inf, max_trip_duration = 120L,
                max_rides = 3, draws_per_minute = 5L, seed = 42L))
)
only <- Sys.getenv("SETTINGS", "")
if (nzchar(only)) {
  only <- strsplit(only, ",")[[1]]
  stopifnot(all(only %in% names(settings)))
  settings <- settings[only]
}

for (city in cities) {
  message(sprintf("== %s ==", city))
  src_dir <- system.file("extdata", city, package = "r5r")
  points <- data.table::fread(file.path(src_dir, paste0(city, "_hexgrid.csv")),
                              colClasses = list(character = "id"))
  points <- points[, .(id, lon, lat)]
  net_dir <- file.path(data_root, paste0(city, "-transit"))
  stopifnot(file.exists(file.path(net_dir, "network.dat")))
  net <- build_network(net_dir, verbose = FALSE)

  for (name in names(settings)) {
    s <- settings[[name]]
    path <- file.path(out_dir, sprintf("%s_TRANSIT_%s.csv", city, name))
    stopifnot(!file.exists(path))
    orig <- points[seq_len(min(s$n, nrow(points)))]
    run <- function() {
      do.call(s$fun, c(list(r5r_network = net, origins = orig, destinations = points),
                       common, s$args))
    }
    t0 <- proc.time()[["elapsed"]]
    res <- run()
    secs <- proc.time()[["elapsed"]] - t0
    res2 <- run()
    same <- identical(res, res2)
    message(sprintf("%-14s %d x %d: %.2f s, rows %d, identical on rerun: %s",
                    name, nrow(orig), nrow(points), secs, nrow(res), same))
    data.table::fwrite(data.table(city = city, setting = name, n_origins = nrow(orig),
                                  rows = nrow(res), seconds = round(secs, 3),
                                  identical_on_rerun = same),
                       file.path(out_dir, "expanded_determinism.csv"),
                       append = file.exists(file.path(out_dir, "expanded_determinism.csv")))
    data.table::fwrite(res, path)
    rm(res, res2)
    gc(verbose = FALSE)
  }
  r5r::stop_r5(net)
  gc(verbose = FALSE)
}
