# peabiru's functions against the formals of r5r 2.5.0's: the same names, in
# the same order, with the same defaults (as expressions), except for the
# differences declared below. The reference is a snapshot of r5r's formals
# (fixtures/r5r-formals.csv, written by tools/r5r_formals.R), so this runs
# without r5r installed. Anything that drifts and is not declared fails.

# r5r's functions peabiru does not have: they manage Java and the R5 jar.
not_in_peabiru <- c("setup_r5", "stop_r5", "download_r5", "r5r_cache", "r5r_sitrep")

# peabiru's functions r5r does not have.
only_in_peabiru <- c("core_version", "trip_options")

# r5r's arguments under another name in peabiru (every function).
renamed <- c(r5r_network = "network")

# r5r's (deprecated) arguments peabiru does not have (every function).
removed <- c("r5r_core", "sample_size")

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

test_that("the snapshot is of r5r 2.5.0's 22 functions", {
  snap <- read_snapshot()
  expect_length(snap, 22)
  expect_true(all(not_in_peabiru %in% names(snap)))
  # every declared rename and removal exists in r5r
  args <- unique(do.call(rbind, snap)$argument)
  expect_true(all(c(names(renamed), removed) %in% args))
})

test_that("peabiru exports r5r's functions but the Java ones, and two of its own", {
  snap <- read_snapshot()
  exports <- getNamespaceExports("peabiru")
  expect_setequal(exports, c(setdiff(names(snap), not_in_peabiru), only_in_peabiru))
})

test_that("the formals are r5r 2.5.0's, but for the declared renames and removals", {
  snap <- read_snapshot()
  for (fun in setdiff(names(snap), not_in_peabiru)) {
    s <- snap[[fun]]
    s <- s[s$position > 0 & !s$argument %in% removed, , drop = FALSE]
    r <- s$argument %in% names(renamed)
    s$argument[r] <- unname(renamed[s$argument[r]])
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

test_that("no function or argument of peabiru is named after r5r", {
  exports <- getNamespaceExports("peabiru")
  expect_length(grep("r5", exports, ignore.case = TRUE), 0)
  for (fun in exports) {
    f <- getExportedValue("peabiru", fun)
    if (!is.function(f)) next
    expect_length(grep("r5", names(formals(f)), ignore.case = TRUE), 0)
    expect_false(any(removed %in% names(formals(f))), label = paste0(fun, "(): removed arguments"))
  }
})
