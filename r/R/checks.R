# Argument checks. r5r validates its arguments with 'checkmate'; these are
# base R equivalents of the few checkmate assertions it uses, so that the
# same inputs are accepted and rejected without adding a dependency.

fail_assert <- function(name, msg) {
  stop("Assertion on '", name, "' failed: ", msg, ".", call. = FALSE)
}

# checkmate's default tolerance for "integerish"
is_integerish <- function(x) {
  is.numeric(x) && all(is.na(x) | (is.finite(x) & abs(x - round(x)) < sqrt(.Machine$double.eps)))
}

# checkmate::assert_number
assert_number <- function(x, name, lower = -Inf, upper = Inf, finite = FALSE,
                          null_ok = FALSE) {
  if (is.null(x)) {
    if (null_ok) return(invisible(x))
    fail_assert(name, "Must be of type 'number', not 'NULL'")
  }
  if (!is.numeric(x) || length(x) != 1L) {
    fail_assert(name, "Must be of type 'number'")
  }
  if (is.na(x)) fail_assert(name, "May not be NA")
  if (finite && !is.finite(x)) fail_assert(name, "Must be finite")
  if (x < lower) fail_assert(name, paste0("Element 1 is not >= ", lower))
  if (x > upper) fail_assert(name, paste0("Element 1 is not <= ", upper))
  invisible(x)
}

# checkmate::assert_count
assert_count <- function(x, name, positive = FALSE) {
  if (!is.numeric(x) || length(x) != 1L) {
    fail_assert(name, "Must be of type 'count'")
  }
  if (is.na(x)) fail_assert(name, "May not be NA")
  if (!is_integerish(x) || abs(x) > .Machine$integer.max) {
    fail_assert(name, "Must be of type 'count'")
  }
  if (x < as.integer(positive)) {
    fail_assert(name, paste0("Must be >= ", as.integer(positive)))
  }
  invisible(x)
}

# checkmate::assert_int
assert_int <- function(x, name, null_ok = FALSE) {
  if (is.null(x) && null_ok) return(invisible(x))
  if (!is.numeric(x) || length(x) != 1L || is.na(x) || !is_integerish(x) ||
      abs(x) > .Machine$integer.max) {
    fail_assert(name, "Must be a single integerish value")
  }
  invisible(x)
}

# checkmate::assert_integerish with the options r5r uses
assert_integerish <- function(x, name, lower = -Inf, upper = Inf,
                              min_len = 0L, max_len = Inf, unique = FALSE,
                              null_ok = FALSE) {
  if (is.null(x)) {
    if (null_ok) return(invisible(x))
    fail_assert(name, "Must be of type 'integerish', not 'NULL'")
  }
  if (!is.numeric(x) || !is_integerish(x)) {
    fail_assert(name, "Must be of type 'integerish'")
  }
  if (anyNA(x)) fail_assert(name, "Contains missing values")
  if (length(x) < min_len) {
    fail_assert(name, paste0("Must have length >= ", min_len))
  }
  if (length(x) > max_len) {
    fail_assert(name, paste0("Must have length <= ", max_len))
  }
  if (any(x < lower)) fail_assert(name, paste0("All elements must be >= ", lower))
  if (any(x > upper)) fail_assert(name, paste0("All elements must be <= ", upper))
  if (unique && anyDuplicated(x) > 0) {
    fail_assert(name, "Contains duplicated values")
  }
  invisible(x)
}

# checkmate::assert_logical(x, len = 1, any.missing = FALSE)
assert_flag <- function(x, name) {
  if (!is.logical(x) || length(x) != 1L || is.na(x)) {
    fail_assert(name, "Must be a single TRUE or FALSE")
  }
  invisible(x)
}

# checkmate::assert_string
assert_string <- function(x, name, null_ok = FALSE) {
  if (is.null(x) && null_ok) return(invisible(x))
  if (!is.character(x) || length(x) != 1L || is.na(x)) {
    fail_assert(name, "Must be a single string")
  }
  invisible(x)
}

assert_network <- function(x) {
  if (!inherits(x, "peabiru_network")) {
    stop(
      "Assertion on 'r5r_network' failed: Must inherit from class ",
      "'peabiru_network' (see build_network()).",
      call. = FALSE
    )
  }
  if (!rs_network_valid(x$ptr)) {
    stop(
      "This network is no longer valid (networks cannot be saved and ",
      "reloaded). Build it again with build_network().",
      call. = FALSE
    )
  }
  invisible(x)
}

# An argument peabiru does not support yet: always an error, never ignored.
unsupported <- function(what, instead = "") {
  stop(what, " is not supported by peabiru yet", instead, ".", call. = FALSE)
}

# r5r >= 2.3.0 still accepts the network as `r5r_core`, with this warning
warn_r5r_core <- function() {
  warning("The `r5r_core` argument is deprecated as of r5r v2.3.0. ",
          "Please use the `r5r_network` argument instead.", call. = FALSE)
}
