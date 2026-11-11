# Argument errors of peabiru against r5r 2.5.0: every exported function the
# two packages share, called with invalid values of each argument; the
# condition class and message must be the same. Exit status 1 on any
# difference not declared in `deliberate` below.
#
#   JAVA_HOME=<JDK 21> Rscript bench/compare/error_parity.R [function regex] [report.csv]
#
# Needs r5r 2.5.0 (with its Java), peabiru, sf. Calls that both packages
# accept are run on a three-point sample of Porto Alegre, so a value that is
# valid but expensive is not tried (see `skip_valid`).
options(java.parameters = "-Xmx4G", width = 80)
args <- commandArgs(TRUE)
only <- if (length(args) >= 1 && nzchar(args[1])) args[1] else "."
report <- if (length(args) >= 2) args[2] else NULL
suppressMessages({
  library(peabiru)
  library(sf)
})

ext <- system.file("extdata/poa", package = "r5r")
data_dir <- file.path(tempdir(), "error_parity_poa")
dir.create(data_dir, showWarnings = FALSE)
file.copy(file.path(ext, c("poa_osm.pbf", "poa_eptc.zip", "poa_trensurb.zip")), data_dir)
nets <- list(
  r5r = suppressMessages(r5r::build_network(data_dir, verbose = FALSE)),
  peabiru = suppressMessages(peabiru::build_network(data_dir))
)
pts <- read.csv(file.path(ext, "poa_hexgrid.csv"))[c(10, 300, 900), ]
pts$jobs <- c(1L, 2L, 3L)
dt <- as.POSIXct("2019-05-13 14:00:00")

# Values tried for every argument.
generic <- list(
  NULL_ = NULL, NA_ = NA, NA_real = NA_real_, chr = "a", chr2 = c("a", "b"),
  neg = -1, zero = 0, frac = 1.5, big = 1e10, inf = Inf, nan = NaN, lgl = TRUE,
  num2 = c(1, 2), empty = numeric(0), list = list(1), df = data.frame(a = 1)
)
# More, by kind of argument.
points_bad <- list(
  no_id = pts[, c("lat", "lon")], no_lat = pts[, c("id", "lon")],
  lat_chr = transform(pts, lat = as.character(lat)), lon_NA = transform(pts, lon = c(NA, lon[-1])),
  dup_id = transform(pts, id = "x"), id_factor = transform(pts, id = factor(id)),
  id_num = transform(pts, id = seq_len(3)), id_NA = transform(pts, id = c(NA, id[-1])),
  matrix = as.matrix(pts[, c("lat", "lon")]), zero_rows = pts[0, ],
  sf_line = st_sf(id = "l", geometry = st_sfc(st_linestring(rbind(c(-51.2, -30.0), c(-51.21, -30.01))), crs = 4326)),
  sf_utm = st_transform(st_as_sf(pts, coords = c("lon", "lat"), crs = 4326), 32722)
)
specific <- list(
  origins = points_bad, destinations = points_bad, points = points_bad,
  mode = list(tram = "TRAM", lower = "walk", two = c("WALK", "CAR"), transit_only = "TRANSIT",
              bus_only = "BUS", car_park = "CAR_PARK", walk_bike = c("WALK", "BICYCLE")),
  mode_egress = list(tram = "TRAM", two = c("WALK", "BICYCLE"), lower = "walk", car = "CAR"),
  departure_datetime = list(text = "2019-05-13 14:00:00", date = as.Date("2019-05-13"),
                            na = as.POSIXct(NA), two = c(dt, dt), lt = as.POSIXlt(dt)),
  arrival_datetime = list(text = "2019-05-13 14:00:00", date = as.Date("2019-05-13"),
                          na = as.POSIXct(NA), two = c(dt, dt)),
  percentiles = list(p0 = 0, p100 = 100, p101 = 101, dup = c(50, 50), six = c(1, 10, 20, 30, 40, 50)),
  cutoffs = list(c0 = 0, dup = c(30, 30), unsorted = c(60, 30), many = 1:13),
  decay_function = list(bad = "gauss", upper = "STEP", two = c("step", "linear")),
  decay_value = list(neg = -1),
  opportunities_colnames = list(missing_col = "schools", two = c("jobs", "nope"), num = 1),
  max_lts = list(l0 = 0, l5 = 5),
  zoom = list(z8 = 8, z13 = 13),
  polygon_output = list(chr = "yes"),
  output = list(bad = "polygon", upper = "SF", two = c("polygon", "sf")),
  dates = list(text = "2019-05-13", bad = "2019-02-30", two = c("2019-05-13", "x")),
  start_date = list(text = "x"), end_date = list(text = "x"),
  radius = list(neg = -1, big = 1e10),
  data_path = list(missing = file.path(tempdir(), "nope"), file = file.path(ext, "poa_osm.pbf")),
  elevation = list(bad = "FLAT", lower = "tobler")
)
# Arguments not tried, with why.
untried <- c(
  r5r_core = "deprecated alias of r5r_network",
  fare_structure = "fares are another job", max_fare = "fares are another job",
  new_carspeeds = "checked by the scenario battery", carspeed_scale = "checked by the scenario battery",
  new_lts = "checked by the scenario battery", output_dir = "writes files",
  temp_dir = "rebuilds the network", overwrite = "rebuilds the network",
  verbose = "no effect", progress = "no effect", n_threads = "no effect on results"
)
# Values that, being valid for an argument, would make an expensive call.
skip_valid <- c("time_window:big", "draws_per_minute:big", "max_rides:big",
                "max_trip_duration:big", "max_walk_time:big", "max_bike_time:big",
                "max_car_time:big", "sample_size:big", "radius:big", "cutoffs:big",
                "cutoffs:many", "zoom:big", "suboptimal_minutes:big")

base <- function(net) list(
  travel_time_matrix = list(r5r_network = net, origins = pts, destinations = pts,
    mode = c("WALK", "TRANSIT"), departure_datetime = dt, max_trip_duration = 30),
  expanded_travel_time_matrix = list(r5r_network = net, origins = pts, destinations = pts,
    mode = c("WALK", "TRANSIT"), departure_datetime = dt, max_trip_duration = 30),
  arrival_travel_time_matrix = list(r5r_network = net, origins = pts, destinations = pts,
    mode = c("WALK", "TRANSIT"), arrival_datetime = dt, max_trip_duration = 30),
  accessibility = list(r5r_network = net, origins = pts, destinations = pts,
    opportunities_colnames = "jobs", mode = "WALK", departure_datetime = dt, cutoffs = 20,
    max_trip_duration = 30),
  detailed_itineraries = list(r5r_network = net, origins = pts, destinations = pts,
    mode = "WALK", departure_datetime = dt, max_trip_duration = 30),
  isochrone = list(r5r_network = net, origins = pts[1, ], mode = "WALK", cutoffs = 10,
    departure_datetime = dt),
  find_snap = list(r5r_network = net, points = pts),
  street_network_bbox = list(r5r_network = net),
  street_network_to_sf = list(r5r_network = net),
  transit_network_to_sf = list(r5r_network = net),
  check_transit_availability = list(r5r_network = net, dates = "2019-05-13"),
  get_gtfs_errors = list(r5r_network = net),
  build_network = list(data_path = data_dir)
)

# Differences kept on purpose: "function:argument:value" (a regex) and why.
deliberate <- c(
  ".*:r5r_network:.*" = "peabiru's network has its own class, peabiru_network, which the message names",
  "get_gtfs_errors:r5r_network:.*" = "the same, and a folder that does not exist is an error of peabiru's own",
  "arrival_travel_time_matrix:arrival_datetime:(chr|chr2|list|df|text)" =
    "r5r fails subtracting max_trip_duration from it (an internal R error)",
  "(travel_time_matrix|expanded_travel_time_matrix|arrival_travel_time_matrix|accessibility|isochrone):percentiles:empty" =
    "r5r lets an empty vector through and fails in Java (or in R for isochrones)",
  "isochrone:polygon_output:(NA_|NA_real|nan|empty)" =
    "r5r takes it as FALSE and fails on an undefined object",
  "isochrone:origins:zero_rows" = "r5r fails on undefined columns (an internal R error)",
  "isochrone:zoom:frac" = "r5r passes a fractional zoom to R5",
  "find_snap:radius:(NA_|empty)" = "r5r lets it through and fails in Java",
  "expanded_travel_time_matrix:mode:(lower|NULL_)" =
    "peabiru does not compute an expanded matrix with a street mode alone (see the documentation)",
  "arrival_travel_time_matrix:mode:(lower|NULL_)" =
    "peabiru does not compute an arrival matrix with a street mode alone (see the documentation)"
)
# r5r raises its cli::cli_abort() errors as 'rlang' conditions; peabiru,
# which imports nothing, raises the same message as a simpleError
cli_class <- function(r) grepl("rlang_error", r)
# r5r fails in Java on values its checks let through; peabiru stops first
java_error <- function(msg) grepl("^java\\.", msg)

run <- function(pkg, fun, a) {
  f <- getExportedValue(pkg, fun)
  # peabiru can be interrupted: a valid but slow call is cut short and
  # counted as accepted
  if (pkg == "peabiru") setTimeLimit(elapsed = 60, transient = TRUE)
  on.exit(setTimeLimit(elapsed = Inf))
  tryCatch({
    suppressMessages(suppressWarnings(do.call(f, a)))
    list(ok = TRUE)
  }, error = function(e) {
    if (grepl("reached elapsed time limit", conditionMessage(e))) return(list(ok = TRUE))
    list(ok = FALSE, msg = conditionMessage(e), cls = paste(class(e), collapse = "/"))
  })
}

rows <- list()
res_p <- NULL
for (fun in grep(only, names(base(NULL)), value = TRUE)) {
  b <- list(r5r = base(nets$r5r)[[fun]], peabiru = base(nets$peabiru)[[fun]])
  argn <- setdiff(names(formals(getExportedValue("r5r", fun))), names(untried))
  for (an in argn) {
    vals <- c(generic, specific[[an]])
    for (vn in names(vals)) {
      key <- paste(fun, an, vn, sep = ":")
      res <- lapply(c("peabiru", "r5r"), function(pkg) {
        a <- b[[pkg]]
        if (an == "r5r_network" && vn == "NULL_") a["r5r_network"] <- list(NULL)
        else if (an == "r5r_network") a$r5r_network <- vals[[vn]]
        else a[an] <- list(vals[[vn]])
        if (pkg == "r5r" && key %in% skip_valid && isTRUE(res_p$ok)) return(list(ok = TRUE, skipped = TRUE))
        r <- run(pkg, fun, a)
        if (pkg == "peabiru") res_p <<- r
        r
      })
      names(res) <- c("peabiru", "r5r")
      p <- res$peabiru; r <- res$r5r
      same <- if (p$ok && r$ok) "both-ok" else if (!p$ok && !r$ok) {
        if (identical(p$msg, r$msg) && identical(p$cls, r$cls)) "same" else
          if (identical(p$msg, r$msg)) "class-differs" else "message-differs"
      } else if (p$ok) "r5r-error-only" else "peabiru-error-only"
      why <- names(deliberate)[vapply(names(deliberate), function(d) grepl(paste0("^", d, "$"), key), TRUE)]
      why <- if (length(why)) deliberate[[why[1]]] else ""
      if (!nzchar(why) && same == "class-differs" && cli_class(r$cls)) {
        why <- "same message; r5r's is an rlang condition (cli), peabiru's a simpleError"
      }
      if (!nzchar(why) && !p$ok && !r$ok && java_error(r$msg)) {
        why <- "r5r fails in Java on a value its checks let through; peabiru stops first"
      }
      rows[[key]] <- data.frame(fun = fun, arg = an, value = vn, outcome = same,
        deliberate = why,
        peabiru = if (p$ok) "" else p$msg, r5r = if (r$ok) "" else r$msg,
        peabiru_class = if (p$ok) "" else p$cls, r5r_class = if (r$ok) "" else r$cls)
    }
  }
  cat(fun, "done\n")
}
# Combinations of arguments that are invalid together.
combos <- list(
  "detailed_itineraries:osm_link_ids+drop_geometry" =
    list(fun = "detailed_itineraries", args = list(osm_link_ids = TRUE, drop_geometry = TRUE)),
  "check_transit_availability:dates+start_date" =
    list(fun = "check_transit_availability", args = list(start_date = "2019-05-13")),
  "check_transit_availability:start_after_end" =
    list(fun = "check_transit_availability",
         args = list(dates = NULL, start_date = "2019-05-14", end_date = "2019-05-13")),
  "accessibility:decay_value_with_step" =
    list(fun = "accessibility", args = list(decay_function = "step", decay_value = 2)),
  "accessibility:fixed_exponential_without_value" =
    list(fun = "accessibility", args = list(decay_function = "fixed_exponential", cutoffs = NULL)),
  "isochrone:lines_with_percentiles" =
    list(fun = "isochrone", args = list(polygon_output = FALSE, percentiles = 50))
)
for (key in grep(only, names(combos), value = TRUE)) {
  cmb <- combos[[key]]
  res <- lapply(c("peabiru", "r5r"), function(pkg) {
    a <- base(nets[[pkg]])[[cmb$fun]]
    for (n in names(cmb$args)) a[n] <- list(cmb$args[[n]])
    run(pkg, cmb$fun, a)
  })
  p <- res[[1]]; r <- res[[2]]
  same <- if (p$ok && r$ok) "both-ok" else if (!p$ok && !r$ok) {
    if (identical(p$msg, r$msg) && identical(p$cls, r$cls)) "same" else
      if (identical(p$msg, r$msg)) "class-differs" else "message-differs"
  } else if (p$ok) "r5r-error-only" else "peabiru-error-only"
  why <- if (same == "class-differs" && cli_class(r$cls)) {
    "same message; r5r's is an rlang condition (cli), peabiru's a simpleError"
  } else ""
  parts <- strsplit(key, ":", fixed = TRUE)[[1]]
  rows[[key]] <- data.frame(fun = parts[1], arg = parts[2], value = "combination",
    outcome = same, deliberate = why,
    peabiru = if (p$ok) "" else p$msg, r5r = if (r$ok) "" else r$msg,
    peabiru_class = if (p$ok) "" else p$cls, r5r_class = if (r$ok) "" else r$cls)
}
out <- do.call(rbind, rows)
rownames(out) <- NULL
if (!is.null(report)) write.csv(out, report, row.names = FALSE)
bad <- out[!out$outcome %in% c("same", "both-ok") & out$deliberate == "", ]
cat(sprintf("%d calls: %d same error, %d both accepted, %d deliberate, %d unexpected\n",
            nrow(out), sum(out$outcome == "same"), sum(out$outcome == "both-ok"),
            sum(!out$outcome %in% c("same", "both-ok") & out$deliberate != ""), nrow(bad)))
for (i in seq_len(nrow(bad))) {
  with(bad[i, ], cat(sprintf("\n%s(%s = %s): %s\n  peabiru [%s]: %s\n  r5r     [%s]: %s\n",
    fun, arg, value, outcome, peabiru_class, peabiru, r5r_class, r5r)))
}
r5r::stop_r5()
quit(status = if (nrow(bad)) 1L else 0L)
