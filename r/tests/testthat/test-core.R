test_that("the Rust core is linked", {
  expect_match(core_version(), "^[0-9]+\\.[0-9]+\\.[0-9]+$")
})

test_that("'all cores' is two under R CMD check's limit on cores", {
  old <- Sys.getenv("_R_CHECK_LIMIT_CORES_", NA)
  on.exit(if (is.na(old)) Sys.unsetenv("_R_CHECK_LIMIT_CORES_")
          else Sys.setenv("_R_CHECK_LIMIT_CORES_" = old))
  threads <- peabiru:::assign_n_threads
  for (limit in c("TRUE", "true", "warn")) {
    Sys.setenv("_R_CHECK_LIMIT_CORES_" = limit)
    expect_identical(threads(Inf), 2L)
    # a number given by the caller is used as it is
    expect_identical(threads(1), 1L)
    expect_identical(threads(3), 3L)
  }
  # 0 is "all cores" for the Rust core
  for (limit in c("", "FALSE", "false")) {
    Sys.setenv("_R_CHECK_LIMIT_CORES_" = limit)
    expect_identical(threads(Inf), 0L)
  }
  Sys.unsetenv("_R_CHECK_LIMIT_CORES_")
  expect_identical(threads(Inf), 0L)
  expect_identical(threads(2), 2L)
})
