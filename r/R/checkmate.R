# A port of the parts of 'checkmate' (2.3.4) the package checks its
# arguments with, so that the checks and their messages are checkmate's
# without a dependency on it. Each check_*() returns TRUE or the
# message checkmate would give; cm_assert() turns a message into checkmate's
# error ("Assertion on '<name>' failed: <message>.").

cm_assert <- function(res, name) {
  if (!isTRUE(res)) {
    stop(simpleError(sprintf("Assertion on '%s' failed: %s.", name, res)))
  }
  invisible(TRUE)
}

# checkmate::assert(..., combine = "and"): the first failing check.
cm_assert_and <- function(name, ...) {
  for (res in list(...)) cm_assert(res, name)
  invisible(TRUE)
}

# guess_type(): the class attribute ("a/b" for several), "matrix" or
# "array" for an atomic object with dimensions, else the type.
cm_guess_type <- function(x) {
  cl <- oldClass(x)
  if (!is.null(cl)) return(paste(cl, collapse = "/"))
  d <- attr(x, "dim", exact = TRUE)
  if (!is.null(d) && is.atomic(x)) return(if (length(d) == 2L) "matrix" else "array")
  typeof(x)
}

cm_type_msg <- function(x, expected, null_ok = FALSE) {
  sprintf("Must be of type '%s'%s, not '%s'", expected,
          if (null_ok) " (or 'NULL')" else "", cm_guess_type(x))
}

cm_tol <- sqrt(.Machine$double.eps)

# checkIntegerish(): "ok", or the kind of failure and its position.
cm_integerish <- function(x, logicals_ok = FALSE, tol = cm_tol) {
  ok <- list(err = "ok", pos = 0L)
  switch(typeof(x),
    integer = if (inherits(x, "factor")) list(err = "type", pos = 0L) else ok,
    logical = if (logicals_ok || all(is.na(x))) ok else list(err = "type", pos = 0L),
    double = {
      if (inherits(x, "Date") || inherits(x, "POSIXt")) return(list(err = "type", pos = 0L))
      x <- unclass(x)
      for (i in seq_along(x)) {
        v <- x[i]
        if (is.na(v)) next
        if (v <= -2147483648 || v > 2147483647) return(list(err = "range", pos = i))
        if (abs(v - round(v)) > tol) return(list(err = "tol", pos = i))
      }
      ok
    },
    complex = {
      for (i in seq_along(x)) {
        im <- Im(x[i])
        if (!is.na(im) && abs(im) > tol) return(list(err = "complex", pos = i))
        v <- Re(x[i])
        if (is.na(v)) next
        if (v <= -2147483648 || v > 2147483647) return(list(err = "range", pos = i))
        if (abs(v - round(v)) > tol) return(list(err = "tol", pos = i))
      }
      ok
    },
    list(err = "type", pos = 0L)
  )
}

cm_is_integerish <- function(x) cm_integerish(x)$err == "ok"

# a vector of missing values of any atomic type passes the type checks
# (check_typed_missing with typed.missing = FALSE)
cm_typed_missing <- function(x) {
  !is.list(x) && is.atomic(x) && !is.null(x) && all(is.na(x))
}

cm_is_scalar_na <- function(x) {
  length(x) == 1L && typeof(x) %in% c("logical", "integer", "double", "character") &&
    is.na(x)
}

cm_check_len <- function(x, len = NULL, min.len = NULL, max.len = NULL) {
  n <- length(x)
  if (!is.null(len) && n != len) {
    return(sprintf("Must have length %g, but has length %g", len, n))
  }
  if (!is.null(min.len) && n < min.len) {
    return(sprintf("Must have length >= %g, but has length %g", min.len, n))
  }
  if (!is.null(max.len) && n > max.len) {
    return(sprintf("Must have length <= %g, but has length %g", max.len, n))
  }
  TRUE
}

cm_check_missings <- function(x, any.missing = TRUE, all.missing = TRUE) {
  if (!any.missing) {
    pos <- if (is.list(x)) which(vapply(x, is.null, TRUE)) else which(is.na(x))
    if (length(pos)) return(sprintf("Contains missing values (element %i)", pos[1L]))
  }
  if (!all.missing && !is.list(x) && all(is.na(x))) return("Contains only missing values")
  TRUE
}

cm_check_bounds <- function(x, lower = -Inf, upper = Inf) {
  # doubles and integers, whatever their class (a Date too)
  if (!typeof(x) %in% c("double", "integer")) return(TRUE)
  if (is.finite(lower) || lower == Inf) {
    w <- which(!is.na(x) & x < lower)
    if (length(w)) return(sprintf("Element %i is not >= %g", w[1L], lower))
  }
  if (is.finite(upper) || upper == -Inf) {
    w <- which(!is.na(x) & x > upper)
    if (length(w)) return(sprintf("Element %i is not <= %g", w[1L], upper))
  }
  TRUE
}

cm_check_unique <- function(x, unique = FALSE) {
  if (unique) {
    pos <- anyDuplicated(x)
    if (pos > 0L) return(sprintf("Contains duplicated values, position %i", pos))
  }
  TRUE
}

`%cm_and%` <- function(lhs, rhs) if (isTRUE(lhs)) rhs else lhs

cm_vector_checks <- function(x, len, min.len, max.len, any.missing, all.missing,
                             lower, upper, finite, unique) {
  r <- cm_check_len(x, len, min.len, max.len)
  if (!isTRUE(r)) return(r)
  r <- cm_check_missings(x, any.missing, all.missing)
  if (!isTRUE(r)) return(r)
  r <- cm_check_bounds(x, lower, upper)
  if (!isTRUE(r)) return(r)
  if (finite && is.numeric(x) && any(is.infinite(x))) return("Must be finite")
  cm_check_unique(x, unique)
}

check_numeric <- function(x, lower = -Inf, upper = Inf, finite = FALSE,
                          any.missing = TRUE, all.missing = TRUE, len = NULL,
                          min.len = NULL, max.len = NULL, unique = FALSE,
                          null.ok = FALSE) {
  if (is.null(x)) {
    return(if (null.ok) TRUE else "Must be of type 'numeric', not 'NULL'")
  }
  if (!((is.double(x) || (is.integer(x) && !inherits(x, "factor"))) || cm_typed_missing(x))) {
    return(cm_type_msg(x, "numeric", null.ok))
  }
  cm_vector_checks(x, len, min.len, max.len, any.missing, all.missing, lower,
                   upper, finite, unique)
}

check_integer <- function(x, lower = -Inf, upper = Inf, any.missing = TRUE,
                          all.missing = TRUE, len = NULL, min.len = NULL,
                          max.len = NULL, unique = FALSE, null.ok = FALSE) {
  if (is.null(x)) {
    return(if (null.ok) TRUE else "Must be of type 'integer', not 'NULL'")
  }
  if (!(is.integer(x) || cm_typed_missing(x))) return(cm_type_msg(x, "integer", null.ok))
  cm_vector_checks(x, len, min.len, max.len, any.missing, all.missing, lower,
                   upper, FALSE, unique)
}

check_integerish <- function(x, lower = -Inf, upper = Inf, any.missing = TRUE,
                             all.missing = TRUE, len = NULL, min.len = NULL,
                             max.len = NULL, unique = FALSE, null.ok = FALSE) {
  if (is.null(x)) {
    return(if (null.ok) TRUE else "Must be of type 'integerish', not 'NULL'")
  }
  ok <- cm_integerish(x)
  if (ok$err == "type" && !cm_typed_missing(x)) return(cm_type_msg(x, "integerish", null.ok))
  if (ok$err == "range") {
    return(sprintf("Must be of type 'integerish', but element %g is not in integer range",
                   ok$pos))
  }
  if (ok$err == "complex") {
    return(sprintf("Must be of type 'integerish', but element %g has an imaginary part",
                   ok$pos))
  }
  if (ok$err == "tol") {
    return(sprintf("Must be of type 'integerish', but element %g is not close to an integer",
                   ok$pos))
  }
  cm_vector_checks(x, len, min.len, max.len, any.missing, all.missing, lower,
                   upper, FALSE, unique)
}

check_character <- function(x, any.missing = TRUE, all.missing = TRUE, len = NULL,
                            min.len = NULL, max.len = NULL, unique = FALSE,
                            null.ok = FALSE) {
  if (is.null(x)) {
    return(if (null.ok) TRUE else "Must be of type 'character', not 'NULL'")
  }
  if (!(is.character(x) || cm_typed_missing(x))) return(cm_type_msg(x, "character", null.ok))
  cm_vector_checks(x, len, min.len, max.len, any.missing, all.missing, -Inf, Inf,
                   FALSE, unique)
}

check_logical <- function(x, any.missing = TRUE, all.missing = TRUE, len = NULL,
                          min.len = NULL, max.len = NULL, null.ok = FALSE) {
  if (is.null(x)) {
    return(if (null.ok) TRUE else "Must be of type 'logical', not 'NULL'")
  }
  if (!(is.logical(x) || cm_typed_missing(x))) return(cm_type_msg(x, "logical", null.ok))
  cm_vector_checks(x, len, min.len, max.len, any.missing, all.missing, -Inf, Inf,
                   FALSE, FALSE)
}

check_posixct <- function(x, any.missing = TRUE, all.missing = TRUE, len = NULL,
                          min.len = NULL, max.len = NULL, null.ok = FALSE) {
  if (is.null(x)) {
    return(if (null.ok) TRUE else "Must be of type 'POSIXct', not 'NULL'")
  }
  if (!(is.numeric(unclass(x)) && inherits(x, "POSIXct"))) {
    return(cm_type_msg(x, "POSIXct", null.ok))
  }
  cm_vector_checks(x, len, min.len, max.len, any.missing, all.missing, -Inf, Inf,
                   FALSE, FALSE)
}

# scalar checks: NA first, then the type, then the length
cm_scalar_head <- function(x, expected, ok_type, na.ok, null.ok) {
  if (cm_is_scalar_na(x)) return(if (na.ok) NA else "May not be NA")
  if (is.null(x)) {
    return(if (null.ok) NA else sprintf("Must be of type '%s', not 'NULL'", expected))
  }
  if (!ok_type) return(cm_type_msg(x, expected, null.ok))
  if (length(x) != 1L) return("Must have length 1")
  TRUE
}

check_count <- function(x, na.ok = FALSE, positive = FALSE, null.ok = FALSE) {
  h <- cm_scalar_head(x, "count", cm_is_integerish(x), na.ok, null.ok)
  if (!isTRUE(h)) return(if (is.na(h)) TRUE else h)
  if (as.integer(x) < as.integer(positive)) return(sprintf("Must be >= %i", as.integer(positive)))
  TRUE
}

check_int <- function(x, na.ok = FALSE, lower = -Inf, upper = Inf, null.ok = FALSE) {
  h <- cm_scalar_head(x, "single integerish value", cm_is_integerish(x), na.ok, null.ok)
  if (!isTRUE(h)) return(if (is.na(h)) TRUE else h)
  cm_check_bounds(x, lower, upper)
}

check_number <- function(x, na.ok = FALSE, lower = -Inf, upper = Inf, finite = FALSE,
                         null.ok = FALSE) {
  ok <- is.double(x) || (is.integer(x) && !inherits(x, "factor"))
  h <- cm_scalar_head(x, "number", ok, na.ok, null.ok)
  if (!isTRUE(h)) return(if (is.na(h)) TRUE else h)
  if (finite && is.infinite(x)) return("Must be finite")
  cm_check_bounds(x, lower, upper)
}

check_string <- function(x, na.ok = FALSE, null.ok = FALSE) {
  h <- cm_scalar_head(x, "string", is.character(x), na.ok, null.ok)
  if (!isTRUE(h)) return(if (is.na(h)) TRUE else h)
  TRUE
}

check_flag <- function(x, na.ok = FALSE, null.ok = FALSE) {
  h <- cm_scalar_head(x, "logical flag", is.logical(x), na.ok, null.ok)
  if (!isTRUE(h)) return(if (is.na(h)) TRUE else h)
  TRUE
}

check_class <- function(x, classes, null.ok = FALSE) {
  if (is.null(x) && null.ok) return(TRUE)
  ord <- inherits(x, classes, TRUE)
  w <- which(ord == 0L)
  if (length(w)) {
    cl <- class(x)
    return(sprintf("Must inherit from class '%s', but has class%s '%s'", classes[w[1L]],
                   if (length(cl) > 1L) "es" else "", paste0(cl, collapse = "','")))
  }
  TRUE
}

cm_set_collapse <- function(x) {
  if (length(x) == 0L) return("{}")
  sprintf("{'%s'}", paste0(unique(x), collapse = "','"))
}

cm_set_msg <- function(msg, what, ...) {
  cap <- function(s) paste0(toupper(substr(s, 1L, 1L)), substring(s, 2L))
  if (is.null(what)) sprintf(cap(msg), ...) else paste(cap(what), sprintf(msg, ...))
}

# allMissing(): every element missing (NULL in a list)
cm_all_missing <- function(x) {
  if (!(is.atomic(x) || is.list(x))) {
    stop(sprintf("Object of type '%s' not supported", typeof(x)), call. = FALSE)
  }
  if (is.list(x)) all(vapply(x, is.null, TRUE)) else all(is.na(x))
}

cm_same_type <- function(x, y) identical(typeof(x), typeof(y)) || (is.numeric(x) && is.numeric(y))

cm_subset_internal <- function(x, choices, what = NULL) {
  if (length(choices) == 0L) {
    if (length(x) == 0L) return(TRUE)
    return(cm_set_msg("must be a subset of the empty set, i.e. also empty", what))
  }
  if (!is.null(x)) {
    if (!cm_same_type(x, choices) && !cm_all_missing(x)) {
      return(cm_set_msg("must be a subset of %s, but has different type", what,
                        cm_set_collapse(choices)))
    }
    ii <- match(x, choices)
    if (anyNA(ii)) {
      return(cm_set_msg("must be a subset of %s, but has additional elements %s", what,
                        cm_set_collapse(choices), cm_set_collapse(x[is.na(ii)])))
    }
  }
  TRUE
}

# checkmate::check_names(x, type = "named" or "unique", subset.of =,
# must.include =)
check_names <- function(x, subset.of = NULL, must.include = NULL, what = "names",
                        type = "named") {
  if (!(is.character(x) || is.null(x))) return("Must be a character vector")
  if (is.null(x)) return(sprintf("Must have %s", what))
  pos <- which(is.na(x))
  if (length(pos)) return(sprintf("Must have %s, but is NA at position %i", what, pos[1L]))
  pos <- which(nchar(x) < 1L)
  if (length(pos)) return(sprintf("Must have %s, but element %i is empty", what, pos[1L]))
  if (type == "unique") {
    pos <- anyDuplicated(x)
    if (pos > 0L) return(sprintf("Must have unique %s, but element %i is duplicated", what, pos))
  }
  if (!is.null(subset.of)) {
    r <- cm_subset_internal(x, subset.of, what)
    if (!isTRUE(r)) return(r)
  }
  if (!is.null(must.include)) {
    ii <- match(must.include, x)
    if (anyNA(ii)) {
      return(cm_set_msg("must include the elements %s, but is missing elements %s", what,
                        cm_set_collapse(must.include),
                        cm_set_collapse(must.include[is.na(ii)])))
    }
  }
  TRUE
}

check_subset <- function(x, choices, empty.ok = TRUE) {
  if (length(x) == 0L) {
    if (!empty.ok) return(sprintf("Must be a subset of %s, not empty", cm_set_collapse(choices)))
    return(TRUE)
  }
  cm_subset_internal(x, choices)
}

check_choice <- function(x, choices, null.ok = FALSE) {
  if (is.null(x)) {
    if (null.ok) return(TRUE)
    return(sprintf("Must be a subset of %s, not 'NULL'", cm_set_collapse(choices)))
  }
  if (!(is.atomic(x) && length(x) == 1L)) {
    return(sprintf("Must be element of set %s, but is not atomic scalar",
                   cm_set_collapse(choices)))
  }
  if (!cm_same_type(x, choices) && !cm_all_missing(x)) {
    return(sprintf("Must be element of set %s, but types do not match (%s != %s)",
                   cm_set_collapse(choices), class(x)[1L], class(choices)[1L]))
  }
  if (match(x, choices, 0L) == 0L) {
    return(sprintf("Must be element of set %s, but is '%s'", cm_set_collapse(choices), x))
  }
  TRUE
}

check_directory_exists <- function(x) {
  if (!(is.character(x) && length(x) >= 1L && !anyNA(x))) return("No directory provided")
  w <- which(!file.exists(x))
  if (length(w)) return(sprintf("Directory '%s' does not exist", x[w[1L]]))
  w <- which(!dir.exists(x))
  if (length(w)) return(sprintf("Directory expected, but file in place: '%s'", x[w[1L]]))
  TRUE
}

# The text of an error raised with cli::cli_abort(), as
# conditionMessage() gives it: `header` (or none) and bullets named "x",
# "i", "!", "*", " " or "v", the inline markup already rendered, wrapped as
# cli wraps them (the first line within the console width less 8, the
# others within the width less 1, bullets continued two spaces in).
cli_text <- function(header = NULL, bullets = character()) {
  utf8 <- isTRUE(l10n_info()[["UTF-8"]])
  sym <- if (utf8) {
    c(x = "\u2716", i = "\u2139", "!" = "!", "*" = "\u2022", v = "\u2714", " " = " ")
  } else {
    c(x = "x", i = "i", "!" = "!", "*" = "*", v = "v", " " = " ")
  }
  width <- getOption("cli.width", getOption("width", 80L))
  lines <- character()
  wrap <- function(text, prefix, indent) {
    words <- strsplit(text, " ", fixed = TRUE)[[1]]
    line <- prefix
    started <- FALSE
    for (w in words) {
      limit <- if (length(lines) == 0L) width - 8L else width - 1L
      cand <- if (started) paste(line, w) else paste0(line, w)
      if (started && nchar(cand) > limit) {
        lines <<- c(lines, line)
        line <- paste0(indent, w)
      } else {
        line <- cand
      }
      started <- TRUE
    }
    lines <<- c(lines, line)
  }
  if (!is.null(header)) wrap(header, "", "")
  for (i in seq_along(bullets)) {
    wrap(bullets[[i]], paste0(sym[[names(bullets)[i]]], " "), "  ")
  }
  paste(lines, collapse = "\n")
}

# cli's {.val x}: strings quoted, several collapsed as "a", "b", and "c"
cli_val <- function(x) {
  q <- if (is.character(x)) ifelse(is.na(x), "NA", paste0("\"", x, "\"")) else format(x)
  n <- length(q)
  if (n <= 1L) return(q)
  if (n == 2L) return(paste(q[1L], "and", q[2L]))
  paste0(paste(q[-n], collapse = ", "), ", and ", q[n])
}

cli_abort_text <- function(header = NULL, bullets = character()) {
  stop(cli_text(header, bullets), call. = FALSE)
}
