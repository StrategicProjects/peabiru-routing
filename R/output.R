# The class of the tables the package returns: options(peabiru.output = ),
# one of "tibble" (the default), "data.frame" and "data.table", read at each
# call. The functions build their tables as plain data.frames (new_df(),
# below, and the sf builders) and convert them once, at the end, with
# as_output() and as_output_sf(); only the class and its attributes change,
# never the rows, columns, types or row names.

output_choices <- c("tibble", "data.frame", "data.table")

# The class asked for by options(peabiru.output = ), checked. Called at the
# start of each function that returns a table, so that a bad value stops the
# call before it routes.
output_class <- function() {
  x <- getOption("peabiru.output")
  if (is.null(x)) return("tibble")
  if (!(is.character(x) && length(x) == 1L && !is.na(x) && x %in% output_choices)) {
    shown <- if (is.character(x) && length(x) == 1L) {
      paste0("\"", x, "\"")
    } else {
      paste0("an object of class '", paste(class(x), collapse = "/"), "'",
             if (is.atomic(x)) paste0(" and length ", length(x)) else "")
    }
    stop("options(peabiru.output = ) must be one of \"tibble\", \"data.frame\" ",
         "or \"data.table\", not ", shown, ".", call. = FALSE)
  }
  if (x == "data.table" && !requireNamespace("data.table", quietly = TRUE)) {
    stop("Package 'data.table' is needed for options(peabiru.output = \"data.table\"); ",
         "install it with install.packages(\"data.table\"), or set the option to ",
         "\"tibble\" or \"data.frame\".", call. = FALSE)
  }
  x
}

# A plain data.frame with compact row names, as as.data.frame() gives.
new_df <- function(cols) {
  n <- if (length(cols)) length(cols[[1]]) else 0L
  structure(cols, class = "data.frame",
            row.names = if (n > 0) c(NA_integer_, -n) else integer(0))
}

# A table built by the package (a data.frame that nothing else refers to)
# in the class `kind`: a tibble, the data.frame itself, or a data.table
# (data.table::setDT(), which converts in place).
as_output <- function(df, kind) {
  switch(kind,
    "data.frame" = df,
    "tibble" = tibble::new_tibble(unclass_df(df), nrow = nrow_df(df)),
    "data.table" = data.table::setDT(df)
  )
}

# The tables of a list (a fare structure, the result of trip_options() or of
# the network exports) converted in place; other elements are left alone.
as_output_list <- function(x, kind, tables, sf_data_table = character()) {
  for (n in intersect(tables, names(x))) {
    if (inherits(x[[n]], "sf")) {
      x[[n]] <- as_output_sf(x[[n]], kind, data_table = n %in% sf_data_table)
    } else if (is.data.frame(x[[n]])) {
      x[[n]] <- as_output(x[[n]], kind)
    }
  }
  x
}

# An sf built by the package (class c("sf", "data.frame")) with the data
# frame class of `kind`: c("sf", "tbl_df", "tbl", "data.frame"), as
# sf::st_as_sf() makes of a tibble; c("sf", "data.frame"); or, for
# "data.table", c("sf", "data.table", "data.frame") for the results that
# are tables of trips or of network elements with attributes to work on
# (`data_table = TRUE`: isochrone polygons, itineraries, street edges,
# transit routes), a plain sf for the others.
# The attributes of the sf (sf_column, agr, row names) are kept as they are.
as_output_sf <- function(x, kind, data_table) {
  class(x) <- switch(kind,
    "tibble" = c("sf", "tbl_df", "tbl", "data.frame"),
    "data.frame" = c("sf", "data.frame"),
    "data.table" = if (data_table) c("sf", "data.table", "data.frame") else c("sf", "data.frame")
  )
  x
}

unclass_df <- function(df) {
  attr(df, "row.names") <- NULL
  unclass(df)
}

nrow_df <- function(df) .row_names_info(df, 2L)

# The text shared by the Value section of every function that returns a
# table (inline R code in the roxygen comments).
doc_output_class <- function(sf = FALSE) {
  paste0(
    "Tables are tibbles by default; `options(peabiru.output = \"data.frame\")` ",
    "gives base `data.frame`s and `options(peabiru.output = \"data.table\")` ",
    "gives `data.table`s (see [peabiru-package]).",
    if (sf) paste0(
      " An `sf` result keeps its geometry column and EPSG:4326 and takes the ",
      "same data frame class: an `sf` tibble, a plain `sf` data frame, or, with ",
      "\"data.table\", an `sf` `data.table` where the section \"Class of the ",
      "results\" of [peabiru-package] says so."
    ) else ""
  )
}
