# Runs the R package's tests against r5r on this machine and fails unless the
# tests that compare with r5r ran: the step of .github/workflows/r5r-parity.yml,
# also usable locally (from the repository root, with peabiru installed):
#
#   NOT_CRAN=true PEABIRU_REQUIRE_R5R=true Rscript tools/r5r_parity.R
#
# With PEABIRU_REQUIRE_R5R=true the test helpers (r/tests/testthat/helper-r5r.R)
# turn a missing r5r, sample data, Java or R5 jar into errors instead of skips
# (and say what is missing); on its own, without the variable, this script
# still fails if any test in the files that compare with r5r was skipped.

if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx4G")
if (!identical(Sys.getenv("NOT_CRAN"), "true")) stop("set NOT_CRAN=true")

library(peabiru)
cat("peabiru", as.character(packageVersion("peabiru")),
    "| r5r", if (requireNamespace("r5r", quietly = TRUE)) as.character(packageVersion("r5r")) else "missing",
    "| isoband", if (requireNamespace("isoband", quietly = TRUE)) as.character(packageVersion("isoband")) else "missing",
    "|", R.version.string, "|", R.version$platform, "\n")

r <- as.data.frame(testthat::test_dir(
  "r/tests/testthat", package = "peabiru", load_package = "installed",
  reporter = "summary", stop_on_failure = FALSE
))

# the files with the tests that compare with r5r running here (every test in
# them must run, none be skipped)
files <- c("test-parity.R", "test-parity-network.R", "test-output_dir.R",
           "test-parity-elevation.R")
parity <- r$file %in% files
cat("\nall tests: passed", sum(r$passed), "failed", sum(r$failed), "errors", sum(r$error),
    "skipped", sum(r$skipped), "\n")
cat("in", paste(files, collapse = ", "), ": tests", sum(parity), "| expectations passed",
    sum(r$passed[parity]), "| skipped", sum(r$skipped[parity]), "\n")
options(width = 200)
print(r[parity,c("file", "test", "nb", "passed", "failed", "skipped", "error")], row.names = FALSE)

bad <- character()
if (sum(r$failed) > 0 || any(r$error)) bad <- c(bad, "tests failed")
if (sum(r$skipped[parity]) > 0) bad <- c(bad, "tests against r5r were skipped")
missing <- setdiff(files, r$file)
if (length(missing)) bad <- c(bad, paste("no test ran from", paste(missing, collapse = ", ")))
if (length(bad)) {
  cat("\nFAIL:", paste(bad, collapse = "; "), "\n")
  quit(status = 1)
}
cat("\nOK\n")
