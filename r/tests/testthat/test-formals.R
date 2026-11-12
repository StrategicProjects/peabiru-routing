# peabiru's functions have the formals of r5r 2.5.0's: the same names, in
# the same order, with the same defaults (as expressions). The reference is
# a snapshot of r5r's formals (fixtures/r5r-formals.csv, written by
# tools/r5r_formals.R), so this runs without r5r installed. Additions are
# declared below; anything else that drifts fails.

# arguments peabiru adds, with their default: function -> list(argument = default)
added <- list(
  build_network = list(n_threads = "Inf")
)

deparse_default <- function(x) {
  if (identical(x, quote(expr = ))) return("")
  paste(deparse(x, width.cutoff = 500L), collapse = " ")
}

read_snapshot <- function() {
  path <- test_path("fixtures", "r5r-formals.csv")
  snap <- utils::read.csv(path, colClasses = c("character", "integer", "character",
                                               "logical", "character"),
                          na.strings = character())
  split(snap, snap$fun)
}

test_that("peabiru exports every function of r5r 2.5.0, and only core_version() and trip_options() besides", {
  snap <- read_snapshot()
  exports <- getNamespaceExports("peabiru")
  # trip_options() is peabiru's own planner, with no r5r counterpart
  expect_setequal(setdiff(exports, names(snap)), c("core_version", "trip_options"))
  expect_length(setdiff(names(snap), exports), 0)
  expect_length(snap, 22)
})

test_that("the formals of every function are r5r 2.5.0's", {
  snap <- read_snapshot()
  for (fun in names(snap)) {
    s <- snap[[fun]]
    s <- s[s$position > 0, , drop = FALSE]
    fm <- formals(getExportedValue("peabiru", fun))
    extra <- added[[fun]]
    ours <- fm[setdiff(names(fm), names(extra))]
    expect_identical(as.character(names(ours)), s$argument,
                     label = paste0(fun, "(): argument names"))
    if (!identical(as.character(names(ours)), s$argument)) next
    expect_identical(
      vapply(ours, function(x) identical(x, quote(expr = )), logical(1), USE.NAMES = FALSE),
      s$required, label = paste0(fun, "(): required arguments")
    )
    expect_identical(unname(vapply(ours, deparse_default, character(1))), s$default,
                     label = paste0(fun, "(): defaults"))
    for (a in names(extra)) {
      expect_true(a %in% names(fm), label = paste0(fun, "(): has ", a))
      expect_identical(deparse_default(fm[[a]]), extra[[a]], label = paste0(fun, "(): default of ", a))
    }
  }
})

test_that("an explicit deprecated() counts as not given, as in r5r", {
  expect_false(peabiru:::is_present(peabiru:::deprecated()))
  f <- function(r5r_core = peabiru:::deprecated()) peabiru:::is_present(r5r_core)
  expect_false(f())
  expect_true(f(NULL))
  expect_no_warning(street_network_bbox(fixture_network(), r5r_core = peabiru:::deprecated()))
})
