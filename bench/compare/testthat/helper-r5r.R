# The tests of this directory compare the installed peabiru with r5r running
# on the same machine. They are repository tooling, not part of the R
# package: tools/r5r_parity.R runs them (the step of
# .github/workflows/r5r-parity.yml, on Linux, Windows and macOS).
#
# Where r5r cannot run (no r5r, sf or data.table, no sample data, no Java 21
# or R5 jar) a test is skipped. When the environment variable
# PEABIRU_REQUIRE_R5R is "true" (the workflow) each of these is an error
# instead, so that a broken setup cannot pass as a run of skipped tests.
require_r5r <- function() identical(Sys.getenv("PEABIRU_REQUIRE_R5R"), "true")

skip_without_r5r <- function(condition, message) {
  if (!isTRUE(condition)) return(invisible(TRUE))
  if (require_r5r()) stop(message, " (PEABIRU_REQUIRE_R5R is \"true\")", call. = FALSE)
  skip(message)
}

# The r5r sample directory `city` (inst/extdata of r5r), checking that r5r
# and the other packages are there; skips (or fails) otherwise.
r5r_sample <- function(city = "spo", packages = character()) {
  for (p in c("r5r", packages)) {
    skip_without_r5r(!requireNamespace(p, quietly = TRUE), paste0("{", p, "} is not installed"))
  }
  if (require_r5r()) {
    v <- as.character(utils::packageVersion("r5r"))
    skip_without_r5r(v != "2.5.0", paste0("r5r ", v, " found; the parity tests need r5r 2.5.0"))
  }
  src <- system.file("extdata", city, package = "r5r")
  pbf <- file.path(src, paste0(city, "_osm.pbf"))
  skip_without_r5r(!file.exists(pbf), "r5r sample data not found")
  src
}

# r5r's network on `dir`, or a skip (an error) when r5r cannot build it.
r5r_network_or_skip <- function(dir) {
  if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx2G")
  why <- NULL
  r5 <- tryCatch(
    suppressMessages(r5r::build_network(dir)),
    error = function(e) {
      why <<- conditionMessage(e)
      NULL
    }
  )
  skip_without_r5r(is.null(r5), paste0(
    "r5r could not build a network (Java 21 or the R5 jar missing?): ", why
  ))
  r5
}

# The comparisons are made under options(peabiru.output = "data.table"),
# with which peabiru's results have the classes of r5r's, so that
# identical() covers class and attributes. data.table's ".internal.selfref"
# is a pointer to the table itself, never identical between two tables: it
# is left out (in every table of a list).
no_selfref <- function(x) {
  if (is.list(x) && !is.data.frame(x)) return(lapply(x, no_selfref))
  attr(x, ".internal.selfref") <- NULL
  x
}
expect_identical_r5r <- function(pb, r5) {
  expect_identical(no_selfref(pb), no_selfref(r5))
}

# ---- what the package's own tests give through their helpers ---------------

# The small network shipped with peabiru, and the rasters of its tests
# (r/tests/testthat/fixtures, read from the checkout).
fixture_path <- function() system.file("extdata/spo", package = "peabiru")

fixture_points <- function() {
  read.csv(file.path(fixture_path(), "spo_points.csv"),
           colClasses = c(id = "character"))
}

package_fixture <- function(file) {
  dir <- Sys.getenv("PEABIRU_R_FIXTURES")
  if (!nzchar(dir)) {
    dir <- file.path(testthat::test_path(), "..", "..", "..", "r", "tests", "testthat", "fixtures")
  }
  path <- file.path(dir, file)
  if (!file.exists(path)) {
    stop("fixture '", file, "' not found in '", dir, "'; run from a checkout of the ",
         "repository or set PEABIRU_R_FIXTURES", call. = FALSE)
  }
  normalizePath(path)
}

# options(peabiru.output = kind) until the calling test (or function) ends.
local_output <- function(kind, frame = parent.frame()) {
  old <- options(peabiru.output = kind)
  do.call(on.exit, list(bquote(options(.(old))), add = TRUE, after = FALSE), envir = frame)
  invisible(old)
}

# options(peabiru.isochrone_repair = how) until the calling test (or function) ends.
local_isochrone_repair <- function(how, frame = parent.frame()) {
  old <- options(peabiru.isochrone_repair = how)
  do.call(on.exit, list(bquote(options(.(old))), add = TRUE, after = FALSE), envir = frame)
  invisible(old)
}

departure <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")
