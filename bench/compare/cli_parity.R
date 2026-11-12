# The command-line tool against the R package: every command of `peabiru`
# run on the same inputs as the R function it mirrors, on r5r's Porto
# Alegre sample (timetables, fares, elevation, scenarios) and the São Paulo
# sample of this package (frequency-based routes), and every table compared
# value for value: the CLI's CSV (or GeoJSON) read back and coerced to the
# column types of the R result, then identical(); lines and polygons by
# their coordinates; output_dir files byte for byte.
#
#   PEABIRU_BIN=<target/release/peabiru> R_LIBS=<lib with peabiru> \
#     Rscript bench/compare/cli_parity.R
#
# Needs r5r installed (for its sample data only; Java is not used), sf and
# data.table. Prints one line per comparison and exits with status 1 if any
# differs.

suppressMessages({
  library(peabiru)
  library(sf)
})
bin <- Sys.getenv("PEABIRU_BIN")
if (!nzchar(bin) || !file.exists(bin)) stop("set PEABIRU_BIN to the peabiru binary")
ext <- system.file("extdata", package = "r5r")
if (!nzchar(ext)) stop("needs r5r's sample data (install r5r)")
options(peabiru.output = "data.frame")
work <- tempfile("cli-parity"); dir.create(work)

# networks: Porto Alegre's .pbf and feeds, the same with its elevation
# raster, and the São Paulo sample
poa <- file.path(work, "poa"); dir.create(poa)
invisible(file.copy(list.files(file.path(ext, "poa"), pattern = "\\.(pbf|zip)$", full.names = TRUE), poa))
poa_tif <- file.path(work, "poa_tif"); dir.create(poa_tif)
invisible(file.copy(c(list.files(poa, full.names = TRUE), file.path(ext, "poa/poa_elevation.tif")), poa_tif))
spo <- system.file("extdata/spo", package = "peabiru")

set.seed(7)
hex <- read.csv(file.path(ext, "poa/poa_hexgrid.csv"), colClasses = c(id = "character"))
pp <- hex[sample(nrow(hex), 300), ]
# coordinates both readers parse to the same doubles (R's strtod is not
# correctly rounded for 17 significant digits)
pp$lon <- round(pp$lon, 7); pp$lat <- round(pp$lat, 7)
sp <- read.csv(file.path(spo, "spo_points.csv"), colClasses = c(id = "character"))
csv_of <- function(df, name) {
  f <- file.path(work, paste0(name, ".csv"))
  write.csv(df, f, row.names = FALSE)
  f
}
P <- list(
  poa = csv_of(pp, "poa_points"), poa_o = csv_of(pp[1:40, ], "poa_o"),
  poa_d = csv_of(pp[41:80, ], "poa_d"), poa_one = csv_of(pp[1:2, ], "poa_one"),
  spo = file.path(spo, "spo_points.csv"), spo_o = csv_of(sp[1:10, ], "spo_o"),
  spo_d = csv_of(sp[11:20, ], "spo_d")
)
t0 <- as.POSIXct("2019-05-13 14:00:00")
DEP <- c("--departure", "2019-05-13T14:00:00")
fares_zip <- file.path(ext, "poa/fares/fares_poa.zip")

results <- character()
record <- function(name, ok, detail = "") {
  results[name] <<- if (ok) "same" else "DIFFERS"
  cat(sprintf("%-42s %s %s\n", name, if (ok) "same" else "DIFFERS", detail))
}

# run the CLI; stdout to `out` (a file) unless the arguments write files
cli <- function(args, out = NULL) {
  err <- tempfile()
  status <- system2(bin, c(args, "--quiet"), stdout = if (is.null(out)) "" else out, stderr = err)
  if (!identical(status, 0L)) {
    stop("peabiru ", paste(args, collapse = " "), " failed: ", paste(readLines(err), collapse = "\n"))
  }
  out
}

# the CLI's CSV with the column types of `r`
read_like <- function(file, r) {
  x <- read.csv(file, colClasses = "character", na.strings = "", check.names = FALSE)
  for (c in names(r)) {
    if (!c %in% names(x)) next
    target <- class(r[[c]])[1]
    v <- x[[c]]
    x[[c]] <- switch(target,
      integer = as.integer(v),
      numeric = as.numeric(v),
      logical = as.logical(v),
      character = v,
      v
    )
  }
  x
}

norm_chr <- function(df) {
  for (c in names(df)) if (is.character(df[[c]])) df[[c]][is.na(df[[c]])] <- ""
  df
}

# R's decimal reader (as.numeric(), read.csv(), fread()) is not correctly
# rounded for 17 significant figures, so doubles that differ after it are
# checked by Python, which is: R's values written exactly (%.17g), the
# CLI's text as written, compared as parsed doubles.
exact_doubles <- function(r, text) {
  a <- tempfile(); b <- tempfile()
  writeLines(ifelse(is.na(r), "NA", sprintf("%.17g", r)), a)
  writeLines(ifelse(is.na(text), "NA", text), b)
  py <- "import sys\nA=open(sys.argv[1]).read().split()\nB=open(sys.argv[2]).read().split()\nprint(len(A)==len(B) and all(x==y if 'NA' in (x,y) else float(x)==float(y) for x,y in zip(A,B)))"
  s <- tempfile(fileext = ".py"); writeLines(py, s)
  identical(system2("python3", c(s, a, b), stdout = TRUE), "True")
}

same_table <- function(name, r, file, drop = character()) {
  if (inherits(r, "sf")) r <- sf::st_drop_geometry(r)
  r <- as.data.frame(r)
  r <- r[, setdiff(names(r), drop), drop = FALSE]
  x <- read_like(file, r)
  ok <- identical(names(x)[seq_along(names(r))], names(r))
  x <- x[, names(r), drop = FALSE]
  rownames(r) <- NULL; rownames(x) <- NULL
  attr(r, "row.names") <- seq_len(nrow(r)); attr(x, "row.names") <- seq_len(nrow(x))
  detail <- sprintf("(%d rows)", nrow(r))
  raw <- read.csv(file, colClasses = "character", na.strings = "", check.names = FALSE)
  bad <- names(r)[!vapply(names(r), function(c) {
    isTRUE(all.equal(norm_chr(r[c]), norm_chr(x[c]), tolerance = 0, check.attributes = FALSE)) ||
      (is.double(r[[c]]) && nrow(r) == nrow(raw) && exact_doubles(r[[c]], raw[[c]]))
  }, TRUE)]
  ok <- ok && nrow(r) == nrow(x) && length(bad) == 0
  if (!ok) {
    detail <- paste(detail, "columns:", paste(bad, collapse = ", "))
    if (Sys.getenv("CLI_PARITY_DEBUG") == "1" && length(bad)) {
      k <- which(!mapply(identical, norm_chr(r)[[bad[1]]], norm_chr(x)[[bad[1]]]))[1:3]
      print(list(r = r[k, bad[1]], cli = x[k, bad[1]], class = class(r[[bad[1]]])))
    }
  }
  record(name, ok, detail)
  invisible(ok)
}

same_coords <- function(a, b) {
  # x and y only: a route through a single linked stop is a one-point
  # LINESTRING in R and a POINT in the CLI's output (GeoJSON and GEOS have
  # no line of one point), at the same position
  xy <- function(s) lapply(s, function(g) unname(sf::st_coordinates(g))[, 1:2, drop = FALSE])
  length(a) == length(b) && identical(xy(a), xy(b))
}

net_poa <- build_network(poa)
net_poa_tif <- build_network(poa_tif)
net_spo <- build_network(spo)
cities <- list(
  poa = list(dir = poa, net = net_poa, pts = P$poa, o = P$poa_o, d = P$poa_d,
             transit = c("WALK", "TRANSIT")),
  spo = list(dir = spo, net = net_spo, pts = P$spo, o = P$spo_o, d = P$spo_d,
             transit = c("WALK", "TRANSIT"))
)
read_pts <- function(f) read.csv(f, colClasses = c(id = "character"))

for (city in names(cities)) {
  C <- cities[[city]]
  pts <- read_pts(C$pts); o <- read_pts(C$o); d <- read_pts(C$d)
  f <- function(x) file.path(work, paste0(city, "_", x, ".csv"))

  # matrices by street
  for (m in c("WALK", "BICYCLE", "CAR")) {
    r <- travel_time_matrix(C$net, pts, pts, mode = m, n_threads = 4)
    same_table(paste(city, "matrix", m), r,
               cli(c("matrix", C$dir, "--origins", C$pts, "--mode", m, "--threads", "4"), f(m)))
  }
  # walking with more origins than destinations: r5r swaps them
  r <- travel_time_matrix(C$net, pts, d, mode = "WALK", max_walk_time = 25)
  same_table(paste(city, "matrix WALK swapped"), r,
             cli(c("matrix", C$dir, "--origins", C$pts, "--destinations", C$d, "--mode", "WALK",
                   "--max-walk-time", "25"), f("walk_swap")))
  # by public transport
  r <- travel_time_matrix(C$net, pts, pts, mode = C$transit, departure_datetime = t0,
                          percentiles = c(25, 50, 75), time_window = 30, seed = 42)
  same_table(paste(city, "matrix transit p25-75 tw30"), r,
             cli(c("matrix", C$dir, "--origins", C$pts, "--mode", "WALK,TRANSIT", DEP,
                   "--percentiles", "25,50,75", "--time-window", "30", "--seed", "42"), f("transit")))
  r <- travel_time_matrix(C$net, o, d, mode = c("BICYCLE", "BUS"), mode_egress = "WALK",
                          departure_datetime = t0, max_bike_time = 15, max_trip_duration = 60,
                          seed = 1, draws_per_minute = 3)
  same_table(paste(city, "matrix bike+bus"), r,
             cli(c("matrix", C$dir, "--origins", C$o, "--destinations", C$d, "--mode", "BICYCLE,BUS",
                   DEP, "--max-bike-time", "15", "--max-trip-duration", "60", "--seed", "1",
                   "--draws-per-minute", "3"), f("bikebus")))
  # output_dir: the files byte for byte
  dr <- file.path(work, paste0(city, "_dir_r")); dc <- file.path(work, paste0(city, "_dir_cli"))
  dir.create(dr); dir.create(dc)
  travel_time_matrix(C$net, o, d, mode = C$transit, departure_datetime = t0, seed = 3,
                     percentiles = c(10, 90), output_dir = dr)
  cli(c("matrix", C$dir, "--origins", C$o, "--destinations", C$d, "--mode", "WALK,TRANSIT", DEP,
        "--seed", "3", "--percentiles", "10,90", "--output-dir", dc))
  fr <- sort(list.files(dr)); fc <- sort(list.files(dc))
  ok <- identical(fr, fc) && all(vapply(fr, function(x) identical(
    readBin(file.path(dr, x), "raw", 1e8), readBin(file.path(dc, x), "raw", 1e8)), TRUE))
  record(paste(city, "matrix output_dir files"), ok, sprintf("(%d files)", length(fr)))

  # expanded and arrival
  r <- expanded_travel_time_matrix(C$net, o, d, mode = C$transit, departure_datetime = t0,
                                   breakdown = TRUE, seed = 5, time_window = 5)
  same_table(paste(city, "expanded breakdown"), r,
             cli(c("expanded", C$dir, "--origins", C$o, "--destinations", C$d, "--mode", "WALK,TRANSIT",
                   DEP, "--breakdown", "--seed", "5", "--time-window", "5"), f("expanded")))
  r <- expanded_travel_time_matrix(C$net, o, d, mode = C$transit, departure_datetime = t0,
                                   max_trip_duration = 45, seed = 5)
  same_table(paste(city, "expanded"), r,
             cli(c("expanded", C$dir, "--origins", C$o, "--destinations", C$d, "--mode", "WALK,TRANSIT",
                   DEP, "--max-trip-duration", "45", "--seed", "5"), f("expanded_nb")))
  r <- arrival_travel_time_matrix(C$net, o, d, mode = C$transit,
                                  arrival_datetime = as.POSIXct("2019-05-13 15:00:00"),
                                  max_trip_duration = 60, breakdown = TRUE, seed = 2)
  same_table(paste(city, "arrival breakdown"), r,
             cli(c("arrival", C$dir, "--origins", C$o, "--destinations", C$d, "--mode", "WALK,TRANSIT",
                   "--arrival", "2019-05-13T15:00", "--max-trip-duration", "60", "--breakdown",
                   "--seed", "2"), f("arrival")))

  # accessibility
  opp <- if (city == "poa") c("jobs", "schools") else c("jobs", "schools")
  r <- accessibility(C$net, o, pts, opportunities_colnames = opp, mode = C$transit,
                     departure_datetime = t0, cutoffs = c(30, 60), seed = 42)
  same_table(paste(city, "accessibility transit step"), r,
             cli(c("accessibility", C$dir, "--origins", C$o, "--destinations", C$pts,
                   "--opportunities", paste(opp, collapse = ","), "--mode", "WALK,TRANSIT", DEP,
                   "--cutoffs", "30,60", "--seed", "42"), f("acc_transit")))
  r <- accessibility(C$net, pts, pts, opportunities_colnames = "jobs", mode = "BICYCLE",
                     decay_function = "logistic", decay_value = 5, cutoffs = 30)
  same_table(paste(city, "accessibility bicycle logistic"), r,
             cli(c("accessibility", C$dir, "--origins", C$pts, "--opportunities", "jobs",
                   "--mode", "BICYCLE", "--decay-function", "logistic", "--decay-value", "5",
                   "--cutoffs", "30"), f("acc_bike")))
  r <- accessibility(C$net, pts, pts, opportunities_colnames = "schools", mode = "WALK",
                     decay_function = "fixed_exponential", decay_value = 0.1)
  same_table(paste(city, "accessibility walk fixed_exponential"), r,
             cli(c("accessibility", C$dir, "--origins", C$pts, "--opportunities", "schools",
                   "--mode", "WALK", "--decay-function", "fixed_exponential", "--decay-value", "0.1"),
                 f("acc_walk")))

  # isochrones: polygons validated per origin and percentile as the R
  # package does (sf::st_make_valid()), then compared by coordinates
  one <- read_pts(P$poa_one)
  if (city == "spo") one <- sp[1:2, c("id", "lon", "lat")]
  onef <- csv_of(one, paste0(city, "_one"))
  for (case in list(
    list(name = "transit z10", r = list(mode = "TRANSIT", cutoffs = c(15, 30), zoom = 10,
                                        departure_datetime = t0, seed = 1),
         a = c("--mode", "TRANSIT", "--cutoffs", "15,30", "--zoom", "10", DEP, "--seed", "1")),
    list(name = "walk z11 p25,75", r = list(mode = "WALK", cutoffs = c(10, 20), zoom = 11,
                                             percentiles = c(25, 75)),
         a = c("--mode", "WALK", "--cutoffs", "10,20", "--zoom", "11", "--percentiles", "25,75"))
  )) {
    r <- do.call(isochrone, c(list(C$net, one), case$r))
    g <- cli(c("isochrone", C$dir, "--origins", onef, case$a),
             file.path(work, paste0(city, "_iso.geojson")))
    x <- sf::read_sf(g)
    grp <- paste(x$id, x$percentile)
    vg <- lapply(split(sf::st_geometry(x), factor(grp, levels = unique(grp))),
                 function(s) sf::st_make_valid(sf::st_sfc(s, crs = 4326)))
    xg <- do.call(c, unname(vg))
    ok <- identical(x$id, r$id) && identical(as.numeric(x$isochrone), r$isochrone) &&
      identical(x$percentile, r$percentile) && same_coords(xg, sf::st_geometry(r))
    record(paste(city, "isochrone", case$name), ok, sprintf("(%d polygons)", nrow(r)))
  }
  r <- isochrone(C$net, one, mode = "WALK", cutoffs = c(5, 10), polygon_output = FALSE)
  g <- cli(c("isochrone", C$dir, "--origins", onef, "--mode", "WALK", "--cutoffs", "5,10", "--lines",
             "--format", "csv"), f("iso_lines"))
  ok <- same_table(paste(city, "isochrone lines (attributes)"), r, g)
  x <- read.csv(g, colClasses = "character")
  record(paste(city, "isochrone lines (lines)"),
         same_coords(sf::st_as_sfc(x$geometry, crs = 4326), sf::st_geometry(r)))

  # itineraries
  r <- detailed_itineraries(C$net, o, d, mode = "WALK", departure_datetime = t0)
  g <- cli(c("itineraries", C$dir, "--origins", C$o, "--destinations", C$d, "--mode", "WALK", DEP), f("it_walk"))
  same_table(paste(city, "itineraries walk"), r, g)
  x <- read.csv(g, colClasses = "character")
  record(paste(city, "itineraries walk (lines)"),
         same_coords(sf::st_as_sfc(x$geometry, crs = 4326), sf::st_geometry(r)))
  r <- detailed_itineraries(C$net, o[1:5, ], d[1:5, ], mode = "CAR", all_to_all = TRUE, departure_datetime = t0,
                            osm_link_ids = TRUE)
  g <- cli(c("itineraries", C$dir, "--origins", csv_of(o[1:5, ], "o5"), "--destinations",
             csv_of(d[1:5, ], "d5"), "--mode", "CAR", "--all-to-all", "--osm-link-ids", DEP), f("it_car"))
  same_table(paste(city, "itineraries car all_to_all osm_link_ids"), r, g)

  # snap and the network
  r <- find_snap(C$net, pts, radius = 100, mode = "CAR")
  same_table(paste(city, "snap"), r,
             cli(c("snap", C$dir, "--points", C$pts, "--radius", "100", "--mode", "CAR"), f("snap")))
  sn <- street_network_to_sf(C$net)
  g <- cli(c("network", C$dir, "--layer", "edges", "--format", "csv"), f("edges"))
  same_table(paste(city, "network edges"), sn$edges, g)
  x <- read.csv(g, colClasses = "character")
  record(paste(city, "network edges (lines)"),
         same_coords(sf::st_as_sfc(x$geometry, crs = 4326), sf::st_geometry(sn$edges)))
  g <- cli(c("network", C$dir, "--layer", "vertices", "--format", "csv"), f("vertices"))
  v <- sn$vertices; xy <- sf::st_coordinates(v)
  v <- cbind(sf::st_drop_geometry(v), lon = xy[, 1], lat = xy[, 2])
  same_table(paste(city, "network vertices"), v, g)
  tn <- transit_network_to_sf(C$net)
  g <- cli(c("network", C$dir, "--layer", "stops", "--format", "csv"), f("stops"))
  st <- tn$stops; xy <- suppressWarnings(sf::st_coordinates(st))
  same_table(paste(city, "network stops"), sf::st_drop_geometry(st), g)
  g <- cli(c("network", C$dir, "--layer", "routes", "--format", "csv"), f("routes"))
  same_table(paste(city, "network routes"), tn$routes, g)
  x <- read.csv(g, colClasses = "character")
  # the CLI writes the lines as built; the R package (as r5r) validates
  # them when one is not valid
  xl <- sf::st_as_sfc(x$geometry, crs = 4326)
  if (any(!sf::st_is_valid(xl))) xl <- sf::st_make_valid(xl)
  record(paste(city, "network routes (lines)"), same_coords(xl, sf::st_geometry(tn$routes)))
  b <- street_network_bbox(C$net, output = "vector")
  g <- cli(c("network", C$dir, "--layer", "bbox", "--format", "csv"), f("bbox"))
  x <- read.csv(g)
  record(paste(city, "network bbox"), identical(unname(unlist(x[1, ])), unname(as.numeric(b))))

  # service dates and GTFS errors
  same_table(paste(city, "info service dates"),
             transform(check_transit_availability(C$net), date = as.character(date)),
             cli(c("info", C$dir, "--service-dates"), f("dates")))
  same_table(paste(city, "info dates range"),
             transform(check_transit_availability(C$net, start_date = "2019-05-10",
                                                   end_date = "2019-05-20"), date = as.character(date)),
             cli(c("info", C$dir, "--start-date", "2019-05-10", "--end-date", "2019-05-20"), f("range")))
  same_table(paste(city, "info gtfs errors"), get_gtfs_errors(C$dir),
             cli(c("info", C$dir, "--gtfs-errors"), f("errors")))
}

# Porto Alegre only: timetables (itineraries by transit), fares, elevation,
# scenarios
pts <- read_pts(P$poa); o <- read_pts(P$poa_o); d <- read_pts(P$poa_d)
f <- function(x) file.path(work, paste0("poa_", x, ".csv"))
fs <- read_fare_structure(fares_zip)

r <- detailed_itineraries(net_poa, o, d, mode = c("WALK", "TRANSIT"), departure_datetime = t0)
g <- cli(c("itineraries", poa, "--origins", P$poa_o, "--destinations", P$poa_d, "--mode",
           "WALK,TRANSIT", DEP), f("it_transit"))
same_table("poa itineraries transit", r, g)
x <- read.csv(g, colClasses = "character")
record("poa itineraries transit (lines)",
       same_coords(sf::st_as_sfc(x$geometry, crs = 4326), sf::st_geometry(r)))
r <- detailed_itineraries(net_poa, o[1:15, ], d[1:15, ], mode = c("WALK", "TRANSIT"),
                          departure_datetime = t0, shortest_path = FALSE, time_window = 5,
                          suboptimal_minutes = 5, drop_geometry = TRUE)
same_table("poa itineraries alternatives", r,
           cli(c("itineraries", poa, "--origins", csv_of(o[1:15, ], "o15"), "--destinations",
                 csv_of(d[1:15, ], "d15"), "--mode", "WALK,TRANSIT", DEP, "--shortest-path", "false",
                 "--time-window", "5", "--suboptimal-minutes", "5", "--drop-geometry"), f("it_alt")))
r <- detailed_itineraries(net_poa, o[1:15, ], d[1:15, ], mode = c("WALK", "TRANSIT"),
                          departure_datetime = t0, fare_structure = fs, drop_geometry = TRUE)
same_table("poa itineraries fares", r,
           cli(c("itineraries", poa, "--origins", csv_of(o[1:15, ], "o15"), "--destinations",
                 csv_of(d[1:15, ], "d15"), "--mode", "WALK,TRANSIT", DEP, "--fare-structure", fares_zip,
                 "--drop-geometry"), f("it_fare")))

r <- travel_time_matrix(net_poa, o, d, mode = c("WALK", "TRANSIT"), departure_datetime = t0,
                        fare_structure = fs, max_fare = 5, time_window = 5)
same_table("poa matrix fares", r,
           cli(c("matrix", poa, "--origins", P$poa_o, "--destinations", P$poa_d, "--mode", "WALK,TRANSIT",
                 DEP, "--fare-structure", fares_zip, "--max-fare", "5", "--time-window", "5"), f("ttm_fare")))
r <- accessibility(net_poa, o, pts, opportunities_colnames = "jobs", mode = c("WALK", "TRANSIT"),
                   departure_datetime = t0, fare_structure = fs, max_fare = 5, cutoffs = 45,
                   time_window = 5)
same_table("poa accessibility fares", r,
           cli(c("accessibility", poa, "--origins", P$poa_o, "--destinations", P$poa, "--opportunities",
                 "jobs", "--mode", "WALK,TRANSIT", DEP, "--fare-structure", fares_zip, "--max-fare", "5",
                 "--cutoffs", "45", "--time-window", "5"), f("acc_fare")))
r <- pareto_frontier(net_poa, o[1:10, ], d, mode = c("WALK", "TRANSIT"), departure_datetime = t0,
                     fare_structure = fs, fare_cutoffs = c(4.5, 4.8, 9, 10), time_window = 5)
same_table("poa pareto", r,
           cli(c("pareto", poa, "--origins", csv_of(o[1:10, ], "o10"), "--destinations", P$poa_d,
                 DEP, "--fare-structure", fares_zip, "--fare-cutoffs", "4.5,4.8,9,10",
                 "--time-window", "5"), f("pareto")))
r <- pareto_frontier(net_poa, o[1:10, ], d, mode = "WALK", fare_structure = fs, fare_cutoffs = c(1, 5))
same_table("poa pareto walk", r,
           cli(c("pareto", poa, "--origins", csv_of(o[1:10, ], "o10"), "--destinations", P$poa_d,
                 "--mode", "WALK", "--fare-structure", fares_zip, "--fare-cutoffs", "1,5"), f("pareto_walk")))

r <- trip_options(net_poa, o[1:20, ], d[1:20, ], mode = c("WALK", "TRANSIT"), departure_datetime = t0,
                  time_window = 5, fare_structure = fs, by_departure = TRUE, drop_geometry = TRUE)
g <- cli(c("options", poa, "--origins", csv_of(o[1:20, ], "o20"), "--destinations", csv_of(d[1:20, ], "d20"),
           DEP, "--time-window", "5", "--fare-structure", fares_zip, "--legs", f("opt_legs"),
           "--departures", f("opt_deps"), "--drop-geometry"), f("opt"))
same_table("poa options", r$options, g)
same_table("poa options legs", r$itineraries, f("opt_legs"))
same_table("poa options departures", r$departures, f("opt_deps"))
r2 <- trip_options(net_poa, o[1:20, ], d[1:20, ], mode = c("BICYCLE", "TRANSIT"), departure_datetime = t0,
                   time_window = 3, criteria = c("TIME", "TRANSFERS"), walk_resolution = 30,
                   itineraries = FALSE)
same_table("poa options bike, walk_resolution 30", r2$options,
           cli(c("options", poa, "--origins", csv_of(o[1:20, ], "o20"), "--destinations",
                 csv_of(d[1:20, ], "d20"), "--mode", "BICYCLE,TRANSIT", DEP, "--time-window", "3",
                 "--criteria", "TIME,TRANSFERS", "--walk-resolution", "30"), f("opt_bike")))

# fare structures: set up by the CLI, read back here
z <- file.path(work, "fares_cli.zip")
cli(c("fares", "setup", poa, "--base-fare", "4.8", "--by", "AGENCY_ID", "-o", z))
a <- setup_fare_structure(net_poa, base_fare = 4.8, by = "AGENCY_ID")
b <- read_fare_structure(z)
record("poa fares setup", isTRUE(all.equal(a, b, tolerance = 0)))

# elevation and scenarios
r <- travel_time_matrix(net_poa_tif, pts, pts, mode = "WALK")
same_table("poa_tif matrix WALK (TOBLER)", r,
           cli(c("matrix", poa_tif, "--origins", P$poa, "--mode", "WALK"), f("tif_walk")))
r <- travel_time_matrix(build_network(poa_tif, elevation = "MINETTI"), pts, pts, mode = "BICYCLE")
same_table("poa_tif matrix BICYCLE (MINETTI)", r,
           cli(c("matrix", poa_tif, "--origins", P$poa, "--mode", "BICYCLE", "--elevation", "MINETTI"),
               f("tif_bike")))
cong <- read.csv(file.path(ext, "poa/poa_osm_congestion.csv"))
r <- travel_time_matrix(net_poa, pts, pts, mode = "CAR", new_carspeeds = cong, carspeed_scale = 0.8)
same_table("poa matrix CAR new_carspeeds", r,
           cli(c("matrix", poa, "--origins", P$poa, "--mode", "CAR", "--new-carspeeds",
                 file.path(ext, "poa/poa_osm_congestion.csv"), "--carspeed-scale", "0.8"), f("cong")))
poly <- readRDS(file.path(ext, "poa/poa_poly_congestion.rds"))
pj <- file.path(work, "poly.geojson"); sf::st_write(poly, pj, quiet = TRUE)
r <- travel_time_matrix(net_poa, pts, pts, mode = "CAR", new_carspeeds = poly)
same_table("poa matrix CAR polygons", r,
           cli(c("matrix", poa, "--origins", P$poa, "--mode", "CAR", "--new-carspeeds", pj), f("poly")))
lts <- read.csv(file.path(ext, "poa/poa_osm_lts.csv"))
r <- accessibility(net_poa, pts, pts, opportunities_colnames = "jobs", mode = "BICYCLE",
                   new_lts = lts, cutoffs = 20)
same_table("poa accessibility BICYCLE new_lts", r,
           cli(c("accessibility", poa, "--origins", P$poa, "--opportunities", "jobs", "--mode", "BICYCLE",
                 "--new-lts", file.path(ext, "poa/poa_osm_lts.csv"), "--cutoffs", "20"), f("lts")))
ls_lts <- readRDS(file.path(ext, "poa/poa_ls_lts.rds"))
lj <- file.path(work, "lines.geojson"); sf::st_write(ls_lts, lj, quiet = TRUE)
r <- travel_time_matrix(net_poa, pts, pts, mode = "BICYCLE", new_lts = ls_lts)
same_table("poa matrix BICYCLE lts lines", r,
           cli(c("matrix", poa, "--origins", P$poa, "--mode", "BICYCLE", "--new-lts", lj), f("lines")))

n_diff <- sum(results != "same")
cat(sprintf("\n%d comparisons: %d same, %d differ\n", length(results), length(results) - n_diff, n_diff))
quit(status = if (n_diff > 0) 1 else 0)
