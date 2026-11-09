# Street-only baseline benchmark of r5r 2.5.0 (R5 7.6.0).
#
# Usage (from the repository root; JAVA_HOME must point to a JDK 21):
#   Rscript bench/r5r/bench_r5r.R poa      # one city
#   Rscript bench/r5r/bench_r5r.R poa spo  # several cities
# Normally run through bench/r5r/run_r5r.sh, which also records peak RSS.
#
# For each city it copies ONLY the OSM .pbf from r5r's sample data into a
# fresh directory under bench/data/r5r/<city>/ (no GTFS, no elevation raster),
# times build_network(), then times travel_time_matrix() all-to-all on the
# hexgrid points for CAR, WALK and BICYCLE with n_threads = 1 and with
# n_threads = all cores. Each timed routing call gets one warm-up run and is
# then repeated N_REPS times; the median is kept.
#
# Outputs (bench/results/r5r/):
#   <city>_<mode>.csv  from_id,to_id,travel_time_p50 (all-cores run)
#   timings.csv        city,mode,n_threads,phase,seconds,n_pairs,n_non_na
#   timings_raw.csv    every individual timed repetition
#   versions.txt       R, r5r, R5 jar and Java versions, machine

# JVM heap must be set before r5r/rJava starts the JVM.
xmx <- Sys.getenv("R5R_XMX", "4G")
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

# Benchmark parameters (r5r defaults written out explicitly).
modes <- c("CAR", "WALK", "BICYCLE")
n_cores <- parallel::detectCores(logical = TRUE)
thread_settings <- c(1L, n_cores)
n_reps <- 3L
departure <- as.POSIXct("13-05-2019 14:00:00", format = "%d-%m-%Y %H:%M:%S",
                        tz = "America/Sao_Paulo")
route_args <- list(
  departure_datetime = departure,
  walk_speed = 3.6,
  bike_speed = 12,
  max_lts = 2,
  max_trip_duration = 120L,
  seed = 42L,
  progress = FALSE,
  verbose = FALSE
)

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

for (city in cities) {
  message(sprintf("== %s ==", city))
  src_dir <- system.file("extdata", city, package = "r5r")
  pbf <- file.path(src_dir, paste0(city, "_osm.pbf"))
  points <- data.table::fread(file.path(src_dir, paste0(city, "_hexgrid.csv")),
                              colClasses = list(character = "id"))
  points <- points[, .(id, lon, lat)]
  n_pairs <- nrow(points)^2

  # Fresh street-only network directory: only the .pbf, so build_network()
  # finds no GTFS and no elevation raster.
  net_dir <- file.path(data_root, city)
  unlink(net_dir, recursive = TRUE)
  dir.create(net_dir, recursive = TRUE)
  stopifnot(file.copy(pbf, net_dir))
  stopifnot(!file.exists(file.path(net_dir, "network.dat")))

  # Build (includes R5 class loading in this JVM and writing network.dat).
  net <- NULL
  t_build <- elapsed(net <- build_network(net_dir, verbose = FALSE,
                                          overwrite = TRUE))
  stopifnot(!is.null(net))
  message(sprintf("build: %.2f s", t_build))
  append_csv(data.table(city = city, mode = NA_character_, n_threads = NA_integer_,
                        phase = "build", seconds = round(t_build, 3),
                        n_pairs = NA_integer_, n_non_na = NA_integer_),
             file.path(out_dir, "timings.csv"))

  for (mode in modes) {
    for (nt in thread_settings) {
      call_ttm <- function() {
        do.call(travel_time_matrix,
                c(list(r5r_network = net, origins = points,
                       destinations = points, mode = mode, n_threads = nt),
                  route_args))
      }
      invisible(call_ttm())  # warm-up
      secs <- numeric(n_reps)
      ttm <- NULL
      for (i in seq_len(n_reps)) {
        secs[i] <- elapsed(ttm <- call_ttm())
      }
      n_non_na <- sum(!is.na(ttm$travel_time_p50))
      med <- stats::median(secs)
      message(sprintf("%-8s threads=%2d median %.2f s (reps: %s), rows %d, non-NA %d",
                      mode, nt, med, paste(sprintf("%.2f", secs), collapse = " "),
                      nrow(ttm), n_non_na))
      append_csv(data.table(city = city, mode = mode, n_threads = nt,
                            phase = "route", seconds = round(med, 3),
                            n_pairs = n_pairs, n_non_na = n_non_na),
                 file.path(out_dir, "timings.csv"))
      append_csv(data.table(city = city, mode = mode, n_threads = nt,
                            rep = seq_len(n_reps), seconds = round(secs, 3)),
                 file.path(out_dir, "timings_raw.csv"))
      # Keep the matrix from the all-cores run (same result as single-thread
      # for street modes, which are deterministic).
      if (nt == max(thread_settings)) {
        data.table::fwrite(ttm[, .(from_id, to_id, travel_time_p50)],
                           file.path(out_dir, sprintf("%s_%s.csv", city, mode)))
      }
      rm(ttm)
      gc(verbose = FALSE)
    }
  }
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
  paste("perf cores:", sysctl("hw.perflevel0.physicalcpu"),
        "/ efficiency cores:", sysctl("hw.perflevel1.physicalcpu")),
  paste("RAM (GiB):", as.numeric(sysctl("hw.memsize")) / 2^30),
  paste("macOS:", paste(system2("sw_vers", "-productVersion", stdout = TRUE)))
), file.path(out_dir, "versions.txt"))
