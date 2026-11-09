# Street-only r5r 2.5.0 run on any network directory and point set.
#
# Usage (JAVA_HOME must point to a JDK 21):
#   Rscript bench/r5r/bench_custom.R <name> <network_dir> <points.csv> [MODE ...]
#
# <network_dir> must hold only the .osm.pbf (no GTFS, no .tif) so the network
# is street-only. Builds the network (timed), then for each mode runs one
# warm-up and one timed all-to-all travel_time_matrix() with all cores.
# Writes bench/results/r5r/<name>_<MODE>.csv and appends to
# bench/results/r5r/custom_timings.csv.

options(java.parameters = paste0("-Xmx", Sys.getenv("R5R_XMX", "8G")))
suppressPackageStartupMessages({
  library(r5r)
  library(data.table)
})

args <- commandArgs(trailingOnly = TRUE)
stopifnot(length(args) >= 3)
name <- args[1]
net_dir <- normalizePath(args[2])
points <- fread(args[3], colClasses = list(character = "id"))[, .(id, lon, lat)]
modes <- if (length(args) > 3) args[-(1:3)] else c("CAR", "WALK", "BICYCLE")
out_dir <- file.path("bench", "results", "r5r")
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)

elapsed <- function(expr) {
  t0 <- proc.time()[["elapsed"]]
  force(expr)
  proc.time()[["elapsed"]] - t0
}
log_time <- function(mode, phase, secs) {
  fwrite(data.table(name = name, mode = mode, phase = phase, seconds = round(secs, 3),
                    n_points = nrow(points), n_threads = parallel::detectCores()),
         file.path(out_dir, "custom_timings.csv"),
         append = file.exists(file.path(out_dir, "custom_timings.csv")))
}

invisible(r5r::download_r5(quiet = TRUE))
unlink(file.path(net_dir, "network.dat"))
net <- NULL
t_build <- elapsed(net <- build_network(net_dir, verbose = FALSE, overwrite = TRUE))
message(sprintf("%s build: %.2f s", name, t_build))
log_time(NA, "build", t_build)

departure <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")
for (mode in modes) {
  call_ttm <- function() {
    travel_time_matrix(net, origins = points, destinations = points, mode = mode,
                       departure_datetime = departure, walk_speed = 3.6, bike_speed = 12,
                       max_lts = 2, max_trip_duration = 120L, seed = 42L,
                       progress = FALSE, verbose = FALSE)
  }
  invisible(call_ttm())
  ttm <- NULL
  secs <- elapsed(ttm <- call_ttm())
  message(sprintf("%s %s: %.2f s, %d pairs", name, mode, secs, nrow(ttm)))
  log_time(mode, "route", secs)
  fwrite(ttm[, .(from_id, to_id, travel_time_p50)],
         file.path(out_dir, sprintf("%s_%s.csv", name, mode)))
}
r5r::stop_r5(net)
