# Public-transport baseline benchmark of r5r 2.5.0 (R5 7.6.0).
#
# Usage (from the repository root; JAVA_HOME must point to a JDK 21):
#   Rscript bench/r5r/bench_transit.R poa      # one city
#   Rscript bench/r5r/bench_transit.R poa spo  # several cities
# Normally run through bench/r5r/run_transit.sh, which also records peak RSS.
#
# For each city it copies the OSM .pbf and the GTFS zips (but NOT the
# elevation raster) from r5r's sample data into a fresh directory
# bench/data/r5r/<city>-transit/, times build_network(), and then runs
# all-to-all on the hexgrid points with mode = c("WALK", "TRANSIT"):
#
#   TRANSIT   travel_time_matrix, percentiles 5/25/50/75/95, time_window 10
#   tw1       the same with time_window = 1 (a single departure minute)
#   mwt15     the same with max_walk_time = 15
#   expanded  expanded_travel_time_matrix (breakdown = TRUE) for the first
#             50 origins x all destinations (per-departure-minute oracle)
#   WALK      walk-only travel_time_matrix on the same transit network, used
#             only for the transit-vs-walk sanity check
#
# Each travel_time_matrix setting gets one all-cores warm-up, N_REPS timed
# all-cores runs (median kept) and one timed single-thread run. The outputs
# of all those runs are compared to check determinism (same seed, across
# repetitions and across thread counts). The TRANSIT setting is then run once
# more with seed 43 to measure how much the result depends on the seed.
#
# Outputs (bench/results/r5r/):
#   <city>_TRANSIT{,_tw1,_mwt15}.csv  from_id,to_id,travel_time_p05..p95
#   <city>_TRANSIT_expanded.csv       expanded matrix, breakdown columns
#   <city>_WALK_transitnet.csv        walk-only reference on the same network
#   transit_timings.csv      city,setting,n_threads,phase,seconds,n_pairs,n_non_na
#   transit_timings_raw.csv  every individual timed repetition
#   transit_determinism.csv  pairwise output comparisons (see compare_ttm)
#   transit_sanity.csv       reachability, percentile ordering, transit vs walk
#   transit_versions.txt     R, r5r, R5 jar and Java versions, machine

# JVM heap must be set before r5r/rJava starts the JVM.
xmx <- Sys.getenv("R5R_XMX", "6G")
options(java.parameters = paste0("-Xmx", xmx))

suppressPackageStartupMessages({
  library(r5r)
  library(data.table)
})

args <- commandArgs(trailingOnly = TRUE)
cities <- if (length(args) > 0) args else c("poa", "spo")
stopifnot(all(cities %in% c("poa", "spo")))

# Locate the repository root from this script's path (bench/r5r/..).
script_arg <- grep("^--file=", commandArgs(trailingOnly = FALSE), value = TRUE)
script_dir <- if (length(script_arg) > 0) {
  dirname(normalizePath(sub("^--file=", "", script_arg[1])))
} else {
  getwd()
}
repo_root <- normalizePath(file.path(script_dir, "..", ".."))
data_root <- file.path(repo_root, "bench", "data", "r5r")
out_dir <- file.path(repo_root, "bench", "results", "r5r")
dir.create(data_root, recursive = TRUE, showWarnings = FALSE)
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)

# GTFS feeds per city. POA: scheduled (calendar 2019-04-15..2019-07-15, plus
# Trensurb 2019-03-01..2019-12-31). SPO: frequency-based (frequencies.txt),
# calendar 2008-01-01..2020-05-01 for every weekday, so the same Monday
# 2019-05-13 is valid for both cities.
gtfs <- list(poa = c("poa_eptc.zip", "poa_trensurb.zip"), spo = "spo.zip")

# Benchmark parameters: r5r defaults, written out explicitly.
n_cores <- parallel::detectCores(logical = TRUE)
n_reps <- 3L
n_expanded_origins <- 50L
departure <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")
base_args <- list(
  mode = c("WALK", "TRANSIT"),
  departure_datetime = departure,
  time_window = 10L,
  max_walk_time = Inf,
  max_trip_duration = 120L,
  walk_speed = 3.6,
  max_rides = 3,
  draws_per_minute = 5L,
  seed = 42L,
  progress = FALSE,
  verbose = FALSE
)
percentiles <- c(5L, 25L, 50L, 75L, 95L)
settings <- list(
  TRANSIT = list(),
  tw1 = list(time_window = 1L),
  mwt15 = list(max_walk_time = 15L)
)
out_name <- function(city, setting) {
  if (setting == "TRANSIT") sprintf("%s_TRANSIT.csv", city)
  else sprintf("%s_TRANSIT_%s.csv", city, setting)
}

# Make sure the R5 jar is cached so that the download never lands inside a
# timed section.
jar <- r5r::download_r5(quiet = TRUE)
stopifnot(!is.null(jar), file.exists(jar))

append_csv <- function(dt, path) {
  data.table::fwrite(dt, path, append = file.exists(path))
}

elapsed <- function(expr) {
  t0 <- proc.time()[["elapsed"]]
  force(expr)
  proc.time()[["elapsed"]] - t0
}

tt_cols <- function(dt) grep("^travel_time_p", names(dt), value = TRUE)

# Compare two travel-time matrices pair by pair. r5r omits unreachable pairs,
# so a pair present in only one of them counts as changed. Returns the number
# of pairs in the union, the number of changed pairs and the mean / max
# absolute difference of the median over pairs present in both.
compare_ttm <- function(a, b) {
  cols <- tt_cols(a)
  stopifnot(identical(cols, tt_cols(b)))
  m <- merge(a[, c("from_id", "to_id", cols), with = FALSE],
             b[, c("from_id", "to_id", cols), with = FALSE],
             by = c("from_id", "to_id"), all = TRUE, suffixes = c(".a", ".b"))
  changed <- rep(FALSE, nrow(m))
  for (cc in cols) {
    x <- m[[paste0(cc, ".a")]]
    y <- m[[paste0(cc, ".b")]]
    changed <- changed | xor(is.na(x), is.na(y)) | (!is.na(x) & !is.na(y) & x != y)
  }
  d50 <- abs(m[["travel_time_p50.a"]] - m[["travel_time_p50.b"]])
  data.table(n_union = nrow(m), n_changed = sum(changed),
             share_changed = round(sum(changed) / nrow(m), 6),
             n_p50_changed = sum(d50 > 0, na.rm = TRUE),
             mean_abs_diff_p50 = round(mean(d50, na.rm = TRUE), 4),
             max_abs_diff_p50 = suppressWarnings(max(d50, na.rm = TRUE)))
}

for (city in cities) {
  message(sprintf("== %s ==", city))
  src_dir <- system.file("extdata", city, package = "r5r")
  points <- data.table::fread(file.path(src_dir, paste0(city, "_hexgrid.csv")),
                              colClasses = list(character = "id"))
  points <- points[, .(id, lon, lat)]
  n_pairs <- nrow(points)^2

  # Fresh transit network directory: .pbf + GTFS zips, no elevation raster.
  net_dir <- file.path(data_root, paste0(city, "-transit"))
  unlink(net_dir, recursive = TRUE)
  dir.create(net_dir, recursive = TRUE)
  stopifnot(file.copy(file.path(src_dir, c(paste0(city, "_osm.pbf"), gtfs[[city]])),
                      net_dir))
  stopifnot(!file.exists(file.path(net_dir, "network.dat")),
            length(list.files(net_dir, "\\.tif$")) == 0)

  # Build (includes R5 class loading in this JVM, GTFS loading and writing
  # network.dat).
  net <- NULL
  t_build <- elapsed(net <- build_network(net_dir, verbose = FALSE,
                                          overwrite = TRUE))
  stopifnot(!is.null(net))
  message(sprintf("build: %.2f s", t_build))
  append_csv(data.table(city = city, setting = NA_character_, n_threads = NA_integer_,
                        phase = "build", seconds = round(t_build, 3),
                        n_pairs = NA_integer_, n_non_na = NA_integer_),
             file.path(out_dir, "transit_timings.csv"))

  log_route <- function(setting, nt, secs, n_non_na, np = n_pairs) {
    append_csv(data.table(city = city, setting = setting, n_threads = nt,
                          phase = "route", seconds = round(stats::median(secs), 3),
                          n_pairs = np, n_non_na = n_non_na),
               file.path(out_dir, "transit_timings.csv"))
    append_csv(data.table(city = city, setting = setting, n_threads = nt,
                          rep = seq_along(secs), seconds = round(secs, 3)),
               file.path(out_dir, "transit_timings_raw.csv"))
  }
  log_cmp <- function(setting, what, cmp) {
    message(sprintf("  %-8s %-26s changed %d / %d pairs (%.4f%%)", setting, what,
                    cmp$n_changed, cmp$n_union, 100 * cmp$share_changed))
    append_csv(cbind(data.table(city = city, setting = setting, comparison = what), cmp),
               file.path(out_dir, "transit_determinism.csv"))
  }

  results <- list()
  for (setting in names(settings)) {
    call_ttm <- function(nt, seed = 42L) {
      a <- utils::modifyList(base_args, settings[[setting]])
      a$seed <- seed
      do.call(travel_time_matrix,
              c(list(r5r_network = net, origins = points, destinations = points,
                     percentiles = percentiles, n_threads = nt), a))
    }
    invisible(call_ttm(n_cores))  # warm-up
    runs <- vector("list", n_reps)
    secs <- numeric(n_reps)
    for (i in seq_len(n_reps)) secs[i] <- elapsed(runs[[i]] <- call_ttm(n_cores))
    ttm <- runs[[1]]
    n_non_na <- sum(!is.na(ttm$travel_time_p50))
    message(sprintf("%-8s threads=%2d median %.2f s (reps: %s), rows %d, non-NA p50 %d",
                    setting, n_cores, stats::median(secs),
                    paste(sprintf("%.2f", secs), collapse = " "), nrow(ttm), n_non_na))
    log_route(setting, n_cores, secs, n_non_na)

    single <- NULL
    t1 <- elapsed(single <- call_ttm(1L))
    message(sprintf("%-8s threads= 1 %.2f s, rows %d", setting, t1, nrow(single)))
    log_route(setting, 1L, t1, sum(!is.na(single$travel_time_p50)))

    for (i in seq_len(n_reps)[-1]) {
      log_cmp(setting, sprintf("seed42 rep1 vs rep%d", i), compare_ttm(ttm, runs[[i]]))
    }
    log_cmp(setting, sprintf("seed42 %d vs 1 threads", n_cores), compare_ttm(ttm, single))

    if (setting == "TRANSIT") {
      t43 <- elapsed(ttm43 <- call_ttm(n_cores, seed = 43L))
      message(sprintf("%-8s seed 43, threads=%2d %.2f s", setting, n_cores, t43))
      log_cmp(setting, "seed42 vs seed43", compare_ttm(ttm, ttm43))
      rm(ttm43)
    }

    data.table::fwrite(ttm[, c("from_id", "to_id", tt_cols(ttm)), with = FALSE],
                       file.path(out_dir, out_name(city, setting)))
    results[[setting]] <- ttm
    rm(runs, single)
    gc(verbose = FALSE)
  }

  # Expanded matrix: first 50 origins (file order) x all destinations, run
  # twice with all cores to check it is deterministic too.
  orig50 <- points[seq_len(n_expanded_origins)]
  call_ettm <- function() {
    do.call(expanded_travel_time_matrix,
            c(list(r5r_network = net, origins = orig50, destinations = points,
                   breakdown = TRUE, n_threads = n_cores), base_args))
  }
  ettm <- NULL
  t_e <- elapsed(ettm <- call_ettm())
  ettm2 <- call_ettm()
  key <- names(ettm)
  same_e <- isTRUE(all.equal(setorderv(copy(ettm), key), setorderv(copy(ettm2), key),
                             check.attributes = FALSE))
  message(sprintf("expanded %d x %d: %.2f s, rows %d, identical on rerun: %s",
                  n_expanded_origins, nrow(points), t_e, nrow(ettm), same_e))
  log_route("expanded", n_cores, t_e, nrow(ettm), np = n_expanded_origins * nrow(points))
  append_csv(data.table(city = city, setting = "expanded",
                        comparison = "seed42 run1 vs run2 (sorted rows)",
                        n_union = max(nrow(ettm), nrow(ettm2)),
                        n_changed = if (same_e) 0L else NA_integer_,
                        share_changed = if (same_e) 0 else NA_real_,
                        n_p50_changed = NA_integer_, mean_abs_diff_p50 = NA_real_,
                        max_abs_diff_p50 = NA_real_),
             file.path(out_dir, "transit_determinism.csv"))
  data.table::fwrite(ettm, file.path(out_dir, sprintf("%s_TRANSIT_expanded.csv", city)))

  # Walk-only reference on the same network (same snapping), for the sanity
  # check only.
  walk <- NULL
  t_w <- elapsed(walk <- travel_time_matrix(
    net, origins = points, destinations = points, mode = "WALK",
    departure_datetime = departure, max_trip_duration = 120L, walk_speed = 3.6,
    n_threads = n_cores, progress = FALSE, verbose = FALSE))
  log_route("WALK", n_cores, t_w, sum(!is.na(walk$travel_time_p50)))
  data.table::fwrite(walk[, .(from_id, to_id, travel_time_p50)],
                     file.path(out_dir, sprintf("%s_WALK_transitnet.csv", city)))

  # Sanity checks.
  for (setting in names(results)) {
    r <- results[[setting]]
    cols <- tt_cols(r)
    v <- as.matrix(r[, ..cols])
    ok <- stats::complete.cases(v)
    ordered <- apply(v[ok, , drop = FALSE], 1, function(x) all(diff(x) >= 0))
    # Percentile columns can be NA individually (p95 beyond 120 min).
    n_by_pct <- vapply(cols, function(cc) sum(!is.na(r[[cc]])), numeric(1))
    w <- merge(r[, .(from_id, to_id, tp50 = travel_time_p50)],
               walk[, .(from_id, to_id, wp50 = travel_time_p50)],
               by = c("from_id", "to_id"), all = TRUE)
    both <- w[!is.na(tp50) & !is.na(wp50)]
    s <- data.table(
      city = city, setting = setting, n_pairs = n_pairs, rows = nrow(r),
      share_reach_p05 = round(n_by_pct[[1]] / n_pairs, 4),
      share_reach_p50 = round(n_by_pct[["travel_time_p50"]] / n_pairs, 4),
      share_reach_p95 = round(n_by_pct[[length(cols)]] / n_pairs, 4),
      share_reach_walk = round(sum(!is.na(walk$travel_time_p50)) / n_pairs, 4),
      n_all_pcts = sum(ok), share_pcts_ordered = round(mean(ordered), 6),
      n_walk_only_reached = w[is.na(tp50) & !is.na(wp50), .N],
      share_transit_le_walk = round(both[, mean(tp50 <= wp50)], 6),
      share_transit_lt_walk = round(both[, mean(tp50 < wp50)], 6),
      mean_p50_both = round(both[, mean(tp50)], 2),
      mean_walk_both = round(both[, mean(wp50)], 2),
      mean_p05 = round(mean(r$travel_time_p05, na.rm = TRUE), 2),
      mean_p50 = round(mean(r$travel_time_p50, na.rm = TRUE), 2),
      mean_p95 = round(mean(r$travel_time_p95, na.rm = TRUE), 2)
    )
    message(sprintf("sanity %s/%s: reach p50 %.1f%% (walk %.1f%%), pct ordered %.4f, transit<=walk %.4f",
                    city, setting, 100 * s$share_reach_p50, 100 * s$share_reach_walk,
                    s$share_pcts_ordered, s$share_transit_le_walk))
    append_csv(s, file.path(out_dir, "transit_sanity.csv"))
  }

  rm(results, walk, ettm, ettm2)
  r5r::stop_r5(net)  # also removes `net` from this environment
  gc(verbose = FALSE)
}

# Versions and machine.
sysctl <- function(key) {
  tryCatch(system2("sysctl", c("-n", key), stdout = TRUE), error = function(e) NA)
}
java_version <- rJava::.jcall("java/lang/System", "S", "getProperty", "java.version")
java_vendor <- rJava::.jcall("java/lang/System", "S", "getProperty", "java.vendor")
writeLines(c(
  paste("R:", R.version.string),
  paste("r5r:", as.character(utils::packageVersion("r5r"))),
  paste("R5 jar:", basename(jar)),
  paste("Java:", java_version, "(", java_vendor, ")"),
  paste("JAVA_HOME:", Sys.getenv("JAVA_HOME")),
  paste("JVM max heap:", xmx),
  paste("hw.model:", sysctl("hw.model")),
  paste("cpu:", sysctl("machdep.cpu.brand_string")),
  paste("cores (logical, used as n_threads):", n_cores),
  paste("RAM (GiB):", as.numeric(sysctl("hw.memsize")) / 2^30),
  paste("macOS:", paste(system2("sw_vers", "-productVersion", stdout = TRUE)))
), file.path(out_dir, "transit_versions.txt"))
