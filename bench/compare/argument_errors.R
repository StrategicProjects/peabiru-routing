# The argument errors of peabiru's R package (which bench/compare/error_parity.R
# compares with r5r's), written as the shared list of cases the Python and
# JavaScript tests check their messages against (python/tests/
# test_argument_errors.py, js/tests/argument_errors.test.mjs), so the three
# packages cannot drift apart.
#
#   PEABIRU_R_LIB=<lib with peabiru> Rscript bench/compare/argument_errors.R [out.json]
#
# Each exported function below is called on the sample network of
# the Python package (python/peabiru/data/spo, which the three test suites
# use) with one argument replaced by one of the values below, which each
# language can write; the cases R refuses are written with R's message and
# condition class (default out: bench/compare/argument_errors.json).
# peabiru returns tibbles by default; this script works with data.frames.
options(peabiru.output = "data.frame")
args <- commandArgs(TRUE)
here <- dirname(sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE)))
out <- if (length(args)) args[1] else file.path(here, "argument_errors.json")
lib <- Sys.getenv("PEABIRU_R_LIB")
if (nzchar(lib)) .libPaths(c(lib, .libPaths()))
suppressMessages(library(peabiru))
spo <- normalizePath(file.path(here, "..", "..", "python", "peabiru", "data", "spo"))
dir <- file.path(tempdir(), "argument_errors_spo")
dir.create(dir, showWarnings = FALSE)
file.copy(list.files(spo, "\\.(pbf|zip)$", full.names = TRUE), dir)
net <- suppressMessages(build_network(dir))
pts <- read.csv(file.path(spo, "spo_points.csv"))[1:3, ]
dt <- as.POSIXct("2019-05-13 14:00:00")
fs <- setup_fare_structure(net, 4.4)

# The values, by the names the tests know them by; each language writes
# them in its own terms (NULL: None / null; a character vector of two: a
# list / an array; numeric(0): an empty list / array).
values <- list(
  NULL_ = NULL, chr = "a", chr2 = c("a", "b"), neg = -1, zero = 0, frac = 1.5,
  big = 1e10, inf = Inf, nan = NaN, lgl = TRUE, num2 = c(1, 2), empty = numeric(0)
)
base <- list(
  travel_time_matrix = list(origins = pts, destinations = pts,
    mode = c("WALK", "TRANSIT"), departure_datetime = dt, max_trip_duration = 30),
  expanded_travel_time_matrix = list(origins = pts, destinations = pts,
    mode = c("WALK", "TRANSIT"), departure_datetime = dt, max_trip_duration = 30),
  arrival_travel_time_matrix = list(origins = pts, destinations = pts,
    mode = c("WALK", "TRANSIT"), arrival_datetime = dt, max_trip_duration = 30),
  accessibility = list(origins = pts, destinations = pts,
    opportunities_colnames = "jobs", mode = "WALK", departure_datetime = dt, cutoffs = 20,
    max_trip_duration = 30),
  detailed_itineraries = list(origins = pts, destinations = pts,
    mode = "WALK", departure_datetime = dt, max_trip_duration = 30),
  isochrone = list(origins = pts[1, ], mode = "WALK", cutoffs = 10, departure_datetime = dt),
  find_snap = list(points = pts),
  check_transit_availability = list(dates = "2019-05-13"),
  setup_fare_structure = list(base_fare = 4.8),
  pareto_frontier = list(origins = pts, destinations = pts,
    mode = c("WALK", "TRANSIT"), departure_datetime = dt, max_trip_duration = 30,
    fare_structure = fs, fare_cutoffs = c(0, 4.8, 9.6))
)
# Arguments not tried: points and data frames (each language takes its own
# kinds), files, and those without effect.
untried <- c("network", "origins", "destinations", "points", "new_carspeeds",
             "new_lts", "carspeed_scale", "output_dir", "verbose", "progress", "n_threads",
             "fare_structure", "debug_path")
# Valid values that would make an expensive call.
skip <- c("time_window:big", "draws_per_minute:big", "max_rides:big", "max_trip_duration:big",
          "max_walk_time:big", "max_bike_time:big", "max_car_time:big",
          "radius:big", "cutoffs:big", "suboptimal_minutes:big", "zoom:big")
cases <- list()
for (fun in names(base)) {
  f <- getExportedValue("peabiru", fun)
  for (an in setdiff(names(formals(f)), untried)) {
    for (vn in names(values)) {
      key <- paste(fun, an, vn, sep = ":")
      if (key %in% skip) next
      a <- c(list(network = net), base[[fun]])
      a[an] <- list(values[[vn]])
      setTimeLimit(elapsed = 30, transient = TRUE)
      res <- tryCatch({
        suppressMessages(suppressWarnings(do.call(f, a)))
        NULL
      }, error = function(e) {
        if (grepl("reached elapsed time limit", conditionMessage(e))) NULL else e
      })
      setTimeLimit(elapsed = Inf)
      if (is.null(res)) next
      cases[[key]] <- list(fun = fun, arg = an, value = vn, message = conditionMessage(res))
    }
  }
  cat(fun, "done\n")
}
names(cases) <- NULL

# How each language words R's message: regular expressions (the same syntax
# in Python and JavaScript) applied in order to R's text. cli's bullets and
# line breaks go, `code` is 'quoted', and types and values are named in the
# language's terms. JavaScript also writes the arguments in camelCase (the
# test turns every snake_case argument name it knows).
cli_rules <- list(
  c("(^|\\n)[!✖ℹ•✔] ", "$1"),
  c("\\s*\\n\\s*", " "),
  c("`NULL`", "NULL"),
  c("`([^`]*)`", "'$1'")
)
type_rules <- function(map) {
  lapply(names(map), function(r) {
    c(paste0("(type|not|or|class) '", gsub(".", "\\.", r, fixed = TRUE), "'"),
      paste0("$1 '", map[[r]], "'"))
  })
}
rules <- list(
  python = c(cli_rules, type_rules(c(
    "logical flag" = "bool flag", logical = "bool", character = "str", double = "float",
    integer = "int", POSIXct = "datetime", Date = "date", data.frame = "DataFrame", "NULL" = "None"
  )), list(
    c("\\bNULL\\b", "None"), c("POSIXct", "datetime"), c("as Inf\\.", "as math.inf."), c("'TRUE'", "'True'"),
    c("'FALSE'", "'False'"), c("\\bTRUE\\b", "True"), c("\\bFALSE\\b", "False")
  )),
  js = c(cli_rules, type_rules(c(
    "logical flag" = "boolean flag", logical = "boolean", character = "string",
    double = "number", integer = "number", numeric = "number", POSIXct = "Date",
    data.frame = "object", list = "Array", "NULL" = "null"
  )), list(
    c("\\bNULL\\b", "null"), c("POSIXct", "Date"), c("as Inf\\.", "as Infinity."), c("'TRUE'", "'true'"),
    c("'FALSE'", "'false'"), c("\\bTRUE\\b", "true"), c("\\bFALSE\\b", "false")
  ))
)
# Cases worded differently on purpose ("fun:arg:value" regular
# expressions), with why; the documentation of each package lists them.
coerced <- paste0("R turns numbers and logicals into text (toupper()) and reports the text ",
                  "as not a mode; Python and JavaScript report the value's type")
both <- list(
  ".*:mode:(neg|zero|frac|big|inf|nan|lgl|num2)" = coerced,
  ".*:mode_egress:(neg|zero|frac|big|inf|nan|lgl|num2)" = coerced,
  "setup_fare_structure:by:(neg|zero|frac|big|inf|nan|lgl|num2)" = coerced,
  "arrival_travel_time_matrix:arrival_datetime:.*" = paste0(
    "R checks arrival_datetime - max_trip_duration as 'departure_datetime' (a difftime for ",
    "a number); Python and JavaScript check arrival_datetime itself"),
  "pareto_frontier:fare_cutoffs:NULL_" = paste0(
    "in R a missing argument and NULL differ; in Python and JavaScript None and undefined ",
    "are the default, which says that fare_cutoffs is required"),
  "detailed_itineraries:all_to_all:.*" = paste0(
    "R names the internal argument 'unique_ids'; Python and JavaScript name ",
    "'all_to_all'")
)
deliberate <- list(python = both, js = c(both, list(
  ".*:departure_datetime:chr" = paste0(
    "JavaScript also takes a string 'YYYY-MM-DD HH:MM:SS' (there is no time zone-free ",
    "date and time type), so a string that is not one says what it must be")
)))
writeLines(jsonlite::toJSON(list(rules = rules, deliberate = deliberate, cases = cases),
                            auto_unbox = TRUE, pretty = TRUE), out)
cat(length(cases), "cases written to", out, "\n")
