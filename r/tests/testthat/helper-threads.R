# CRAN allows a check two cores. Most calls in the tests pass n_threads (1
# or 2); the ones that leave the default ("all cores"), and setup_r5(), which
# has no such argument, get two threads through R CMD check's own switch,
# which the package honours (assign_n_threads() in R/assign.R). R CMD check
# --as-cran sets it; CRAN's regular checks may not, so it is set here unless
# the tests run in development (NOT_CRAN = "true": devtools, the CI).
if (!identical(Sys.getenv("NOT_CRAN"), "true") &&
    !nzchar(Sys.getenv("_R_CHECK_LIMIT_CORES_"))) {
  Sys.setenv("_R_CHECK_LIMIT_CORES_" = "TRUE")
}

# The number of threads for the tests that compare one thread with several:
# three in development, two on CRAN.
several_threads <- function() {
  if (identical(Sys.getenv("NOT_CRAN"), "true")) 3 else 2
}
