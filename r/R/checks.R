# Argument checks. r5r validates its arguments with 'checkmate'; these call
# the port of it in checkmate.R with the arguments r5r passes, so that the
# same inputs are accepted and rejected, with the same messages, without
# adding a dependency.

fail_assert <- function(name, msg) {
  stop("Assertion on '", name, "' failed: ", msg, ".", call. = FALSE)
}

# checkmate's default tolerance for "integerish"
is_integerish <- function(x) cm_is_integerish(x) && is.numeric(x)

# checkmate::assert_number
assert_number <- function(x, name, lower = -Inf, upper = Inf, finite = FALSE,
                          null_ok = FALSE) {
  cm_assert(check_number(x, lower = lower, upper = upper, finite = finite,
                         null.ok = null_ok), name)
  invisible(x)
}

# checkmate::assert_count
assert_count <- function(x, name, positive = FALSE) {
  cm_assert(check_count(x, positive = positive), name)
  invisible(x)
}

# checkmate::assert_int
assert_int <- function(x, name, null_ok = FALSE) {
  cm_assert(check_int(x, null.ok = null_ok), name)
  invisible(x)
}

# checkmate::assert_integerish(any.missing = FALSE, ...), as r5r calls it
assert_integerish <- function(x, name, lower = -Inf, upper = Inf,
                              min_len = NULL, max_len = NULL, unique = FALSE,
                              null_ok = FALSE) {
  cm_assert(check_integerish(x, lower = lower, upper = upper, any.missing = FALSE,
                             min.len = min_len, max.len = max_len, unique = unique,
                             null.ok = null_ok), name)
  invisible(x)
}

# checkmate::assert_logical(x, len = 1, any.missing = FALSE)
assert_flag <- function(x, name) {
  cm_assert(check_logical(x, len = 1, any.missing = FALSE), name)
  invisible(x)
}

# checkmate::assert_string
assert_string <- function(x, name, null_ok = FALSE) {
  cm_assert(check_string(x, null.ok = null_ok), name)
  invisible(x)
}

assert_network <- function(x) {
  # checkmate::assert_class(r5r_network, "r5r_network"), with peabiru's class
  cm_assert(check_class(x, "peabiru_network"), "r5r_network")
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
