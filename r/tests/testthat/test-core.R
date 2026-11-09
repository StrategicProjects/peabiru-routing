test_that("the Rust core is linked", {
  expect_match(core_version(), "^[0-9]+\\.[0-9]+\\.[0-9]+$")
})
