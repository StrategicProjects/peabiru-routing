# The parity tests compare peabiru with r5r running on the same machine.
# Where r5r cannot run (no r5r, sf or data.table, no sample data, no Java 21
# or R5 jar) they are skipped, and they are skipped on CRAN. When the
# environment variable PEABIRU_REQUIRE_R5R is "true" (the r5r-parity CI
# workflow) each of these is an error instead, so that a broken setup cannot
# pass as a run of skipped tests.
require_r5r <- function() identical(Sys.getenv("PEABIRU_REQUIRE_R5R"), "true")

skip_without_r5r <- function(condition, message) {
  if (!isTRUE(condition)) return(invisible(TRUE))
  if (require_r5r()) stop(message, " (PEABIRU_REQUIRE_R5R is \"true\")", call. = FALSE)
  skip(message)
}

# The r5r sample directory `city` (inst/extdata of r5r), checking that r5r
# and the other packages are there; skips (or fails) otherwise.
r5r_sample <- function(city = "spo", packages = character()) {
  if (require_r5r()) {
    skip_without_r5r(!identical(Sys.getenv("NOT_CRAN"), "true"), "NOT_CRAN is not \"true\"")
  } else {
    skip_on_cran()
  }
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
