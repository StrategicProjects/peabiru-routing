# trip_options() in R, Python and JavaScript on the same inputs: the
# scenarios below (Porto Alegre with r5r's fare structure, all criteria and
# by departure; bicycle access with two criteria; fare and walking with a
# fare limit; Sao Paulo's frequency-based routes) are run with the R package
# here, with trip_options_py.py and trip_options_js.mjs, and every table is
# compared value for value through CSV files. trip_options() has no r5r
# counterpart, so the three bindings are compared with each other.
#
#   PYTHON=<python with peabiru> Rscript bench/compare/trip_options_bindings.R
#
# Needs r5r's sample data (the r5r package installed, for its extdata), node
# and the npm package built (js/build.sh). Exits with status 1 if any table
# differs. JS_THREADS (default 3) runs the npm package with a pool of
# workers.

suppressMessages(library(peabiru))
root <- normalizePath(file.path(dirname(sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE))), "..", ".."))
python <- Sys.getenv("PYTHON", "python3")
ext <- system.file("extdata", package = "r5r")
if (!nzchar(ext)) stop("needs r5r's sample data (install r5r)")
poa <- tempfile("trip-options-poa"); dir.create(poa)
invisible(file.copy(list.files(file.path(ext, "poa"), pattern = "\\.(pbf|zip)$", full.names = TRUE), poa))
spo <- system.file("extdata/spo", package = "peabiru")
zp <- file.path(ext, "poa/fares/fares_poa.zip")
set.seed(3)
pp <- read.csv(file.path(ext, "poa/poa_hexgrid.csv"))
pp <- pp[sample(nrow(pp), 60), c("id", "lon", "lat")]
sp <- read.csv(file.path(spo, "spo_points.csv"), colClasses = c(id = "character"))
sp <- sp[c(1:20), c("id", "lon", "lat")]
# unquoted, for the plain reader of trip_options_js.mjs
ppf <- tempfile(fileext = ".csv"); write.csv(pp, ppf, row.names = FALSE, quote = FALSE)
spf <- tempfile(fileext = ".csv"); write.csv(sp, spf, row.names = FALSE, quote = FALSE)

out_r <- tempfile("trip-options-r"); dir.create(out_r)
save <- function(name, r) {
  write.csv(r$options, file.path(out_r, paste0(name, "_options.csv")), row.names = FALSE)
  if (!is.null(r$itineraries)) {
    write.csv(r$itineraries, file.path(out_r, paste0(name, "_legs.csv")), row.names = FALSE)
  }
  if (!is.null(r$departures)) {
    write.csv(r$departures, file.path(out_r, paste0(name, "_departures.csv")), row.names = FALSE)
  }
}
t0 <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")
M <- c("WALK", "TRANSIT")
net <- build_network(poa)
fs <- read_fare_structure(zp)
o <- pp[1:30, ]; d <- pp[31:60, ]
save("poa_fares", trip_options(net, o, d, mode = M, departure_datetime = t0, time_window = 5L,
                               fare_structure = fs, by_departure = TRUE, drop_geometry = TRUE))
save("poa_bike", trip_options(net, o, d, mode = c("BICYCLE", "TRANSIT"), departure_datetime = t0,
                              time_window = 3L, criteria = c("TIME", "TRANSFERS"), max_rides = 2,
                              drop_geometry = TRUE))
save("poa_cheap", trip_options(net, o, d, mode = M, departure_datetime = t0, time_window = 2L,
                               criteria = c("FARE", "WALK"), fare_structure = fs, max_fare = 7.2,
                               itineraries = FALSE, by_departure = TRUE))
net <- build_network(spo)
save("spo", trip_options(net, sp[1:10, ], sp[11:20, ], mode = M, departure_datetime = t0,
                         time_window = 5L, by_departure = TRUE, drop_geometry = TRUE))

out_py <- tempfile("trip-options-py")
out_js <- tempfile("trip-options-js")
rc <- system2(python, c(file.path(root, "bench/compare/trip_options_py.py"), poa, zp, ppf, spo, spf, out_py))
if (rc != 0) stop("the Python scenarios failed")
feeds <- paste(basename(list.files(poa, pattern = "\\.zip$")), collapse = ",")
rc <- system2("node", c(file.path(root, "bench/compare/trip_options_js.mjs"), poa, zp, ppf, spo, spf, out_js,
                        Sys.getenv("JS_THREADS", "3")), env = paste0("FEEDS=", feeds))
if (rc != 0) stop("the JavaScript scenarios failed")

read <- function(f) {
  if (!file.exists(f) || file.size(f) == 0) return(data.frame())
  x <- read.csv(f, colClasses = "character", na.strings = character())
  x
}
bad <- 0
for (f in sort(list.files(out_r))) {
  ref <- read(file.path(out_r, f))
  for (lang in c("py", "js")) {
    got <- read(file.path(if (lang == "py") out_py else out_js, f))
    # numbers as numbers (each writer prints them its own way)
    same <- identical(names(ref), names(got)) && nrow(ref) == nrow(got) && all(vapply(names(ref), function(j) {
      a <- ref[[j]]; b <- got[[j]]
      na <- suppressWarnings(as.numeric(a)); nb <- suppressWarnings(as.numeric(b))
      if (!anyNA(na) && !anyNA(nb)) identical(na, nb) else identical(a, b)
    }, logical(1)))
    cat(sprintf("%-28s %-3s %s (%d rows)\n", f, lang, if (same) "identical" else "DIFFERS", nrow(ref)))
    if (!same) bad <- bad + 1
  }
}
if (bad > 0) quit(status = 1)
