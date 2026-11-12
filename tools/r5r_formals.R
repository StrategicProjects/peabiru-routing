# Writes the formals of every function r5r exports (names, order and
# defaults, deparsed) to bench/compare/testthat/fixtures/r5r-formals.csv,
# the snapshot bench/compare/testthat/test-formals.R compares peabiru's
# functions with (but for the renames and removals declared there). Run
# from the repository's root with the r5r version peabiru is compared with
# installed:
#
#   Rscript tools/r5r_formals.R
#
# The snapshot is committed, so the test runs without r5r.

if (!requireNamespace("r5r", quietly = TRUE)) stop("r5r is not installed")
version <- as.character(utils::packageVersion("r5r"))
if (!identical(version, "2.5.0")) stop("peabiru is compared with r5r 2.5.0, not ", version)

deparse_default <- function(x) {
  if (identical(x, quote(expr = ))) return("")
  paste(deparse(x, width.cutoff = 500L), collapse = " ")
}

rows <- list()
for (fun in sort(getNamespaceExports("r5r"))) {
  f <- getExportedValue("r5r", fun)
  if (!is.function(f)) next
  fm <- formals(f)
  if (length(fm) == 0) {
    rows[[length(rows) + 1]] <- data.frame(
      fun = fun, position = 0L, argument = "", required = FALSE, default = ""
    )
    next
  }
  rows[[length(rows) + 1]] <- data.frame(
    fun = fun,
    position = seq_along(fm),
    argument = names(fm),
    required = vapply(fm, function(x) identical(x, quote(expr = )), logical(1)),
    default = vapply(fm, deparse_default, character(1)),
    row.names = NULL
  )
}
out <- do.call(rbind, rows)
path <- file.path("bench", "compare", "testthat", "fixtures", "r5r-formals.csv")
utils::write.csv(out, path, row.names = FALSE)
cat("wrote", nrow(out), "arguments of", length(unique(out$fun)), "functions of r5r", version,
    "to", path, "\n")
