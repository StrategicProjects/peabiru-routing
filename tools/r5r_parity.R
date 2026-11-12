# Compares the installed peabiru with r5r on this machine: the step of
# .github/workflows/r5r-parity.yml, also usable locally (from the repository
# root, with peabiru installed):
#
#   NOT_CRAN=true PEABIRU_REQUIRE_R5R=true Rscript tools/r5r_parity.R
#
# It runs two test directories against the installed package:
#
# 1. bench/compare/testthat, the comparisons with r5r (results compared with
#    identical(), files byte for byte, and the functions' formals against a
#    snapshot of r5r's). They are not part of the R package. With
#    PEABIRU_REQUIRE_R5R=true their helpers (helper-r5r.R) turn a missing
#    r5r, sample data, Java or R5 jar into errors instead of skips (and say
#    what is missing); on its own, without the variable, this script still
#    fails if any of these tests was skipped or if one of the files did not
#    run.
# 2. r/tests/testthat, the package's own tests, here with sf and data.table
#    installed (the main CI job runs them without).

if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx4G")
if (!identical(Sys.getenv("NOT_CRAN"), "true")) stop("set NOT_CRAN=true")

library(peabiru)
cat("peabiru", as.character(packageVersion("peabiru")),
    "| r5r", if (requireNamespace("r5r", quietly = TRUE)) as.character(packageVersion("r5r")) else "missing",
    "| isoband", if (requireNamespace("isoband", quietly = TRUE)) as.character(packageVersion("isoband")) else "missing",
    "|", R.version.string, "|", R.version$platform, "\n")

parity_dir <- file.path("bench", "compare", "testthat")
# the rasters of the package's tests, used by the elevation comparisons
Sys.setenv(PEABIRU_R_FIXTURES = normalizePath(file.path("r", "tests", "testthat", "fixtures")))

# the files that must run, every test of them (none skipped)
files <- sort(list.files(parity_dir, pattern = "^test-.*\\.R$"))
expected <- c("test-formals.R", "test-parity-elevation.R", "test-parity-network.R",
              "test-parity-output_dir.R", "test-parity-shape-dist.R", "test-parity.R")
# this one needs a `zip` program to rewrite the feeds, and skips without one
may_skip <- if (nzchar(Sys.which("zip"))) character() else "test-parity-shape-dist.R"

cat("\n== comparisons with r5r (", parity_dir, ") ==\n", sep = "")
p <- as.data.frame(testthat::test_dir(
  parity_dir, package = "peabiru", load_package = "installed",
  reporter = "summary", stop_on_failure = FALSE
))
cat("\n== the package's tests (r/tests/testthat) ==\n")
r <- as.data.frame(testthat::test_dir(
  "r/tests/testthat", package = "peabiru", load_package = "installed",
  reporter = "summary", stop_on_failure = FALSE
))

options(width = 200)
cat("\ncomparisons with r5r: tests", nrow(p), "| expectations passed", sum(p$passed),
    "| failed", sum(p$failed), "| errors", sum(p$error), "| skipped", sum(p$skipped), "\n")
print(p[, c("file", "test", "nb", "passed", "failed", "skipped", "error")], row.names = FALSE)
by_file <- vapply(split(p$passed, p$file), sum, numeric(1))
cat("\nexpectations passed by file:\n")
print(by_file)
cat("\npackage tests: passed", sum(r$passed), "failed", sum(r$failed), "errors", sum(r$error),
    "skipped", sum(r$skipped), "\n")

bad <- character()
if (sum(p$failed) > 0 || any(p$error)) bad <- c(bad, "comparisons with r5r failed")
if (sum(r$failed) > 0 || any(r$error)) bad <- c(bad, "package tests failed")
must_run <- !p$file %in% may_skip
if (sum(p$skipped[must_run]) > 0) bad <- c(bad, "comparisons with r5r were skipped")
missing <- setdiff(union(expected, files), unique(p$file))
if (length(missing)) bad <- c(bad, paste("no test ran from", paste(missing, collapse = ", ")))
gone <- setdiff(expected, files)
if (length(gone)) bad <- c(bad, paste("missing from", parity_dir, ":", paste(gone, collapse = ", ")))
if (length(may_skip)) cat("no zip program:", may_skip, "may skip\n")
if (length(bad)) {
  cat("\nFAIL:", paste(bad, collapse = "; "), "\n")
  quit(status = 1)
}
cat("\nOK\n")
