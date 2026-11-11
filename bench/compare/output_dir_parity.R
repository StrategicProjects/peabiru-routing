# Parity of the files written with output_dir by the peabiru R package and by
# r5r 2.5.0: the same calls through peabiru:: and r5r::, each writing into an
# empty directory of its own; the two directories must list the same file
# names and every file must be byte-identical. Any difference (a file only
# on one side, or the first differing line of a file) is reported and makes
# the script stop with an error. Each scenario also checks that the files
# hold the rows of peabiru's in-memory result (before the NA rules of the R
# wrapper, which the files do not have).
#
# Usage (from the repository root; r5r needs a JDK 21):
#   JAVA_HOME=/path/to/jdk21 Rscript bench/compare/output_dir_parity.R [poa] [spo]
#
# The peabiru package must be installed (R_LIBS can point to a private
# library). Environment:
#   N_THREADS   threads for both packages (default 4)
#   R5R_XMX     JVM heap (default 6G)
#   SCENARIOS   comma-separated scenario name prefixes to run (default all)
#   KEEP        a directory to keep the written files in (default: removed)

xmx <- Sys.getenv("R5R_XMX", "6G")
options(java.parameters = paste0("-Xmx", xmx))
suppressPackageStartupMessages({
  library(r5r)
  library(peabiru)
})

args <- commandArgs(trailingOnly = TRUE)
cities <- if (length(args) > 0) args else c("poa", "spo")
stopifnot(all(cities %in% c("poa", "spo")))
n_threads <- as.integer(Sys.getenv("N_THREADS", "4"))
only <- Sys.getenv("SCENARIOS", "")
only <- if (nzchar(only)) strsplit(only, ",")[[1]] else character(0)
wanted <- function(name) length(only) == 0 || any(startsWith(name, only))

tz <- "America/Sao_Paulo"
departure <- as.POSIXct("2019-05-13 14:00:00", tz = tz)
keep <- Sys.getenv("KEEP", "")
work <- if (nzchar(keep)) keep else file.path(tempdir(), "output_dir_parity")
unlink(work, recursive = TRUE)
dir.create(work, recursive = TRUE)

failures <- character(0)
n_files <- 0L
n_bytes <- 0

# The first difference between two directories of CSV files, or NULL.
compare_dirs <- function(a, b) {
  fa <- sort(list.files(a, all.files = TRUE, no.. = TRUE))
  fb <- sort(list.files(b, all.files = TRUE, no.. = TRUE))
  if (!identical(fa, fb)) {
    return(sprintf("files only in peabiru: [%s]; only in r5r: [%s]",
                   paste(head(setdiff(fa, fb), 5), collapse = ", "),
                   paste(head(setdiff(fb, fa), 5), collapse = ", ")))
  }
  for (f in fa) {
    x <- readBin(file.path(a, f), "raw", file.size(file.path(a, f)))
    y <- readBin(file.path(b, f), "raw", file.size(file.path(b, f)))
    if (!identical(x, y)) {
      lx <- strsplit(rawToChar(x), "\n", fixed = TRUE)[[1]]
      ly <- strsplit(rawToChar(y), "\n", fixed = TRUE)[[1]]
      k <- which(lx[seq_len(min(length(lx), length(ly)))] != ly[seq_len(min(length(lx), length(ly)))])[1]
      if (is.na(k)) k <- min(length(lx), length(ly)) + 1L
      return(sprintf("%s differs (%d vs %d bytes), line %d: peabiru '%s', r5r '%s'",
                     f, length(x), length(y), k, lx[k], ly[k]))
    }
    n_files <<- n_files + 1L
    n_bytes <<- n_bytes + length(x)
  }
  NULL
}

# The files read back as one data.frame of strings, in origin order.
read_dir <- function(dir, origin_ids) {
  files <- file.path(dir, paste0("from_", origin_ids, ".csv"))
  files <- files[file.exists(files)]
  parts <- lapply(files, utils::read.csv, colClasses = "character",
                  na.strings = character(0), check.names = FALSE)
  do.call(rbind, parts)
}

check <- function(city, name, fun_name, net, args, mem_check = TRUE) {
  if (!wanted(name)) return(invisible())
  label <- sprintf("%s/%s", city, name)
  d_pb <- file.path(work, city, name, "peabiru")
  d_r5 <- file.path(work, city, name, "r5r")
  dir.create(d_pb, recursive = TRUE)
  dir.create(d_r5, recursive = TRUE)
  t0 <- proc.time()[["elapsed"]]
  ret_pb <- do.call(getExportedValue("peabiru", fun_name),
                    c(list(net$pb), args, n_threads = n_threads, output_dir = d_pb))
  t_pb <- proc.time()[["elapsed"]] - t0
  t0 <- proc.time()[["elapsed"]]
  ret_r5 <- do.call(getExportedValue("r5r", fun_name),
                    c(list(net$r5), args, n_threads = n_threads, progress = FALSE,
                      output_dir = d_r5))
  t_r5 <- proc.time()[["elapsed"]] - t0
  why <- NULL
  if (!identical(ret_pb, d_pb) || !identical(ret_r5, d_r5)) {
    why <- "the return value is not output_dir"
  }
  if (is.null(why)) why <- compare_dirs(d_pb, d_r5)
  nf <- length(list.files(d_r5))
  message(sprintf("%-28s files %5d  peabiru %6.2f s  r5r %6.2f s  identical %s",
                  label, nf, t_pb, t_r5, is.null(why)))
  if (is.null(why) && mem_check) {
    # the files against peabiru's in-memory result: the same rows, except
    # that the R wrapper's NA rule (times over max_trip_duration) is not
    # applied to the files
    mem <- do.call(getExportedValue("peabiru", fun_name),
                   c(list(net$pb), args, n_threads = n_threads))
    disk <- read_dir(d_pb, unique(args$origins$id))
    if (is.null(disk)) disk <- mem[0, ]
    if (!identical(sort(names(disk)) , sort(c(names(mem), if (!"cutoff" %in% names(mem) && fun_name == "accessibility") "cutoff")))) {
      why <- "the files' columns are not those of the in-memory result"
    } else if (nrow(disk) != nrow(mem)) {
      why <- sprintf("the files hold %d rows, the in-memory result %d", nrow(disk), nrow(mem))
    }
  }
  if (!is.null(why)) {
    message("    ", why)
    failures <<- c(failures, paste0(label, ": ", why))
  }
  invisible()
}

copy_network <- function(city) {
  src <- system.file("extdata", city, package = "r5r")
  dir <- file.path(work, paste0(city, "-network"))
  dir.create(dir)
  files <- c(file.path(src, paste0(city, "_osm.pbf")),
             list.files(src, pattern = "\\.zip$", full.names = TRUE))
  stopifnot(all(file.copy(files, dir)))
  dir
}

for (city in cities) {
  message(sprintf("== %s ==", city))
  points <- read.csv(
    system.file("extdata", city, paste0(city, "_hexgrid.csv"), package = "r5r"),
    colClasses = c(id = "character")
  )
  if (!"jobs" %in% names(points)) points$jobs <- points$population
  if (!"schools" %in% names(points)) points$schools <- points$population %% 7
  dir <- copy_network(city)
  net <- list(pb = peabiru::build_network(dir, n_threads = n_threads),
              r5 = suppressMessages(r5r::build_network(dir)))
  transit <- c("WALK", "TRANSIT")
  pct5 <- c(5L, 25L, 50L, 75L, 95L)
  first <- function(n) points[seq_len(min(n, nrow(points))), ]

  # a few awkward ids, a point with no coordinates and one far from the
  # network
  odd <- first(40)
  odd$id[1:6] <- c("a,b", "q\"x", "line\nbreak", "", "x y", "ãç")
  odd$lat[7] <- NA
  odd$lon[8] <- odd$lon[8] + 5

  for (m in c("WALK", "BICYCLE", "CAR")) {
    check(city, paste0("ttm_", m), "travel_time_matrix", net,
          list(origins = points, destinations = points, mode = m))
  }
  # more origins than destinations: in memory both packages route WALK
  # from the destinations (and the two directions can differ), the files are
  # routed from the origins, so the rows are not those of the in-memory
  # result
  check(city, "ttm_WALK_swap", "travel_time_matrix", net,
        list(origins = points, destinations = first(100), mode = "WALK",
             percentiles = c(10L, 90L), max_trip_duration = 45L),
        mem_check = FALSE)
  check(city, "ttm_CAR_pct", "travel_time_matrix", net,
        list(origins = first(300), destinations = points, mode = "CAR",
             percentiles = c(75L, 25L), max_trip_duration = 20L))
  check(city, "ttm_transit", "travel_time_matrix", net,
        list(origins = points, destinations = points, mode = transit,
             departure_datetime = departure, percentiles = pct5, seed = 42L))
  check(city, "ttm_transit_tw1", "travel_time_matrix", net,
        list(origins = points, destinations = points, mode = transit,
             departure_datetime = departure, time_window = 1L,
             max_walk_time = 15L, max_trip_duration = 50L, seed = 42L))
  check(city, "ttm_odd_ids", "travel_time_matrix", net,
        list(origins = odd, destinations = odd, mode = transit,
             departure_datetime = departure, percentiles = c(25L, 75L), seed = 1L),
        mem_check = FALSE)
  check(city, "ttm_odd_ids_walk", "travel_time_matrix", net,
        list(origins = odd, destinations = odd, mode = "WALK"), mem_check = FALSE)

  check(city, "expanded", "expanded_travel_time_matrix", net,
        list(origins = first(30), destinations = points, mode = transit,
             departure_datetime = departure, time_window = 5L, seed = 42L))
  check(city, "expanded_breakdown", "expanded_travel_time_matrix", net,
        list(origins = first(30), destinations = points, mode = transit,
             departure_datetime = departure, time_window = 3L, breakdown = TRUE,
             max_trip_duration = 60L, seed = 42L))
  check(city, "expanded_odd_ids", "expanded_travel_time_matrix", net,
        list(origins = odd, destinations = odd, mode = transit,
             departure_datetime = departure, time_window = 2L, breakdown = TRUE,
             seed = 3L), mem_check = FALSE)
  check(city, "arrival", "arrival_travel_time_matrix", net,
        list(origins = first(30), destinations = points, mode = transit,
             arrival_datetime = departure + 3600, max_trip_duration = 60L, seed = 42L))
  check(city, "arrival_breakdown", "arrival_travel_time_matrix", net,
        list(origins = first(30), destinations = points, mode = transit,
             arrival_datetime = departure + 5400, breakdown = TRUE, seed = 42L))

  check(city, "access_step", "accessibility", net,
        list(origins = points, destinations = points,
             opportunities_colnames = c("jobs", "schools"), mode = transit,
             departure_datetime = departure, cutoffs = c(40L, 20L),
             percentiles = c(25L, 50L), seed = 42L))
  check(city, "access_logistic", "accessibility", net,
        list(origins = points, destinations = points, opportunities_colnames = "jobs",
             mode = transit, departure_datetime = departure,
             decay_function = "logistic", decay_value = 8, cutoffs = c(20L, 40L),
             max_trip_duration = 60L, seed = 42L))
  check(city, "access_fixed_exp", "accessibility", net,
        list(origins = points, destinations = points, opportunities_colnames = "jobs",
             mode = "BICYCLE", decay_function = "fixed_exponential",
             decay_value = 0.002))
  check(city, "access_linear_car", "accessibility", net,
        list(origins = points, destinations = points, opportunities_colnames = "jobs",
             mode = "CAR", decay_function = "linear", decay_value = 10,
             cutoffs = c(15L, 30L)))
  check(city, "access_odd_ids", "accessibility", net,
        list(origins = odd, destinations = odd, opportunities_colnames = "jobs",
             mode = "WALK", decay_function = "exponential", cutoffs = 30L),
        mem_check = FALSE)
}

message(sprintf("\n%d files compared, %.1f MB, all byte-identical unless listed below",
                n_files, n_bytes / 1e6))
if (length(failures) > 0) {
  message(paste0("FAIL ", failures, collapse = "\n"))
  stop(length(failures), " scenario(s) differ", call. = FALSE)
}
if (!nzchar(keep)) unlink(work, recursive = TRUE)
message("all identical")
