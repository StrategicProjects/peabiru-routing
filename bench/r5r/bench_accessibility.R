# Accessibility baseline of r5r 2.5.0 (R5 7.6.0) for peabiru's parity checks.
#
# Usage (from the repository root; JAVA_HOME must point to a JDK 21):
#   Rscript bench/r5r/bench_accessibility.R poa spo
# Env: PEABIRU_ROOT  repository whose bench/data and bench/results are used
#                    (default: the one holding this script)
#      R5R_XMX       JVM max heap (default 3G)
#
# Runs r5r::accessibility() once per row of bench/r5r/accessibility_settings.csv
# with origins = destinations = the city's hexgrid (POA 1227, SPO 323 points,
# which carry the opportunity columns population, schools, jobs, healthcare).
# Networks are the ones bench_transit.R and bench_r5r.R built and cached:
#   network = transit  bench/data/r5r/<city>-transit/  (OSM + GTFS)
#   network = street   bench/data/r5r/<city>/          (OSM only)
# so build_network() just loads network.dat.
#
# Fixed parameters (the other baselines' ones): departure 2019-05-13 14:00
# America/Sao_Paulo, walk_speed 3.6, bike_speed 12, max_lts 2, max_rides 3,
# draws_per_minute 5, seed 42, all cores.
#
# Output: bench/results/r5r/<city>_ACCESS_<setting>.csv with r5r's columns
# id, opportunity, percentile, cutoff (absent for fixed_exponential),
# accessibility. fwrite() would keep only 15 significant digits, so the
# accessibility is written with sprintf("%.17g"), which round-trips every
# double exactly. Existing files are never overwritten.

xmx <- Sys.getenv("R5R_XMX", "3G")
options(java.parameters = paste0("-Xmx", xmx))

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
  file.path(getwd(), "bench", "r5r")
}
repo_root <- Sys.getenv("PEABIRU_ROOT", normalizePath(file.path(script_dir, "..", "..")))
data_root <- file.path(repo_root, "bench", "data", "r5r")
out_dir <- file.path(repo_root, "bench", "results", "r5r")
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)

settings <- fread(file.path(script_dir, "accessibility_settings.csv"),
                  colClasses = "character", na.strings = NULL)
split_num <- function(x) if (x == "") NULL else as.numeric(strsplit(x, ";")[[1]])
split_chr <- function(x) strsplit(x, ";")[[1]]

departure <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")

for (city in cities) {
  src_dir <- system.file("extdata", city, package = "r5r")
  points <- fread(file.path(src_dir, paste0(city, "_hexgrid.csv")),
                  colClasses = list(character = "id"))
  nets <- list()
  for (i in seq_len(nrow(settings))) {
    s <- settings[i]
    out <- file.path(out_dir, sprintf("%s_ACCESS_%s.csv", city, s$setting))
    if (file.exists(out)) {
      message(sprintf("%s %s: %s exists, skipped", city, s$setting, out))
      next
    }
    net_dir <- file.path(data_root, if (s$network == "transit") paste0(city, "-transit") else city)
    stopifnot(file.exists(file.path(net_dir, "network.dat")))
    if (is.null(nets[[s$network]])) {
      nets[[s$network]] <- build_network(net_dir, verbose = FALSE)
    }
    decay_value <- if (s$decay_value == "") NULL else as.numeric(s$decay_value)
    t0 <- proc.time()[["elapsed"]]
    acc <- accessibility(
      nets[[s$network]],
      origins = points,
      destinations = points,
      opportunities_colnames = split_chr(s$opportunities),
      mode = split_chr(s$mode),
      departure_datetime = departure,
      time_window = as.integer(s$time_window),
      percentiles = as.integer(split_num(s$percentiles)),
      decay_function = s$decay,
      cutoffs = split_num(s$cutoffs),
      decay_value = decay_value,
      max_walk_time = as.numeric(s$max_walk_time),
      max_trip_duration = as.integer(s$max_trip_duration),
      walk_speed = 3.6,
      bike_speed = 12,
      max_rides = 3,
      max_lts = 2,
      draws_per_minute = 5L,
      seed = 42L,
      progress = FALSE,
      verbose = FALSE
    )
    secs <- proc.time()[["elapsed"]] - t0
    acc[, accessibility := sprintf("%.17g", accessibility)]
    fwrite(acc, out)
    message(sprintf("%s %-24s %6d rows  %.2f s  -> %s", city, s$setting, nrow(acc), secs,
                    basename(out)))
  }
  # drop the network references; the JVM frees them on gc()
  rm(nets)
  gc(verbose = FALSE)
}
