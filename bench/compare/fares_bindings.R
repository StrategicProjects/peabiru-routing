# Fares in the Python and npm packages against r5r 2.5.0: the scenarios of
# fares_py.py and fares_js.mjs (a matrix and accessibility within a budget,
# a Pareto frontier, alternative itineraries with fares, a fare structure
# file) on r5r's Porto Alegre sample with its fare structure, compared
# value for value through CSV files (the CSV bytes of the structure file).
#
#   PYTHON=<python with peabiru> Rscript bench/compare/fares_bindings.R
#
# Needs r5r, a JDK 21 (JAVA_HOME), node and the npm package built
# (js/build.sh). Exits with status 1 if any scenario differs.

# peabiru returns tibbles by default; this script works with data.frames.
options(peabiru.output = "data.frame")
if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx6G")
suppressMessages(library(data.table))
root <- normalizePath(file.path(dirname(sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE))), "..", ".."))
python <- Sys.getenv("PYTHON", "python3")
ext <- system.file("extdata", package = "r5r")
w <- tempfile("fares-bindings"); dir.create(w)
invisible(file.copy(list.files(file.path(ext, "poa"), pattern = "\\.(pbf|zip)$", full.names = TRUE), w))
zp <- file.path(ext, "poa/fares/fares_poa.zip")
pts <- fread(file.path(ext, "poa/poa_hexgrid.csv"))
set.seed(7)
pts <- pts[sample(nrow(pts), 300)]
pts$jobs <- round(runif(nrow(pts), 0, 40))
pf <- tempfile(fileext = ".csv")
fwrite(pts[, .(id, lon, lat, jobs)], pf)

rn <- suppressMessages(r5r::build_network(w, verbose = FALSE))
fs <- r5r::read_fare_structure(zp)
o <- pts[1:15]; d <- pts
t0 <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")
M <- c("WALK", "TRANSIT")
ref <- list(
  ttm = r5r::travel_time_matrix(rn, o, d, mode = M, departure_datetime = t0, fare_structure = fs, max_fare = 4.8,
                                draws_per_minute = 2, percentiles = c(10, 50, 90), progress = FALSE),
  access = r5r::accessibility(rn, o, d, opportunities_colnames = "jobs", mode = M, departure_datetime = t0,
                              fare_structure = fs, max_fare = 7.2, cutoffs = c(20, 45), draws_per_minute = 1, progress = FALSE),
  pareto = r5r::pareto_frontier(rn, o, d, mode = M, departure_datetime = t0, fare_structure = fs,
                                fare_cutoffs = c(4.5, 4.8, 9, 15), progress = FALSE),
  itineraries = r5r::detailed_itineraries(rn, o, d[16:30], mode = M, departure_datetime = t0, fare_structure = fs,
                                          shortest_path = FALSE, drop_geometry = TRUE, progress = FALSE)
)
setup <- tempfile(fileext = ".zip")
r5r::write_fare_structure(r5r::setup_fare_structure(rn, 4.8, by = "AGENCY_NAME"), setup)

norm <- function(x) {
  x <- as.data.table(x)
  for (j in names(x)) {
    v <- x[[j]]
    if (is.factor(v) || inherits(v, "POSIXt")) v <- as.character(v)
    if (is.character(v)) v[is.na(v)] <- ""
    if (is.numeric(v)) v <- as.double(v)
    set(x, j = j, value = v)
  }
  x
}
csv_bytes <- function(zip) {
  d <- tempfile(); utils::unzip(zip, exdir = d)
  f <- sort(list.files(d))
  setNames(lapply(f, function(x) readBin(file.path(d, x), "raw", 1e7)), f)
}

ok <- 0L; bad <- 0L
for (b in c("python", "js")) {
  out <- tempfile(paste0("fares-", b))
  cmd <- if (b == "python") c(python, file.path(root, "bench/compare/fares_py.py"))
         else c("node", file.path(root, "bench/compare/fares_js.mjs"))
  # the feeds in the order R5 loads them: the file system's (ls -f)
  feeds <- grep("\\.zip$", system2("ls", c("-f", shQuote(w)), stdout = TRUE), value = TRUE)
  st <- system2(cmd[1], c(cmd[-1], shQuote(w), shQuote(zp), shQuote(pf), shQuote(out)),
                env = paste0("FEEDS=", paste(feeds, collapse = ",")))
  if (st != 0) { bad <- bad + 1L; cat(b, "failed to run\n"); next }
  for (k in names(ref)) {
    got <- norm(fread(file.path(out, paste0(k, ".csv")), colClasses = list(character = intersect(c("from_id", "to_id", "id", "route", "departure_time", "mode"), names(ref[[k]])))))
    want <- norm(ref[[k]])
    r <- isTRUE(all.equal(got, want, check.attributes = FALSE, tolerance = 0))
    if (r) ok <- ok + 1L else { bad <- bad + 1L; print(all.equal(got, want, check.attributes = FALSE, tolerance = 0)) }
    cat(sprintf("%-6s %-12s %s (%d rows)\n", b, k, if (r) "same" else "DIFFERENT", nrow(want)))
  }
  r <- identical(csv_bytes(file.path(out, "setup.zip")), csv_bytes(setup))
  if (r) ok <- ok + 1L else bad <- bad + 1L
  cat(sprintf("%-6s %-12s %s\n", b, "setup.zip", if (r) "same CSV bytes" else "DIFFERENT"))
}
cat(sprintf("\n%d same, %d different\n", ok, bad))
if (bad > 0) quit(status = 1)
