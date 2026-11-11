# r5r's functions that manage Java and the R5 jar: setup_r5(), stop_r5(),
# r5r_cache(), download_r5() and r5r_sitrep(). peabiru has no Java and no
# jar, so each does the closest honest thing, and scripts written for r5r
# run unchanged.

#' Create a transport network used for routing (deprecated)
#'
#' @description
#' `setup_r5()` is r5r's old name of [build_network()], deprecated in r5r
#' since 2.3.0 and kept here so that scripts written for r5r run. As in
#' r5r, it warns and then calls [build_network()] with the same arguments.
#'
#' @inheritParams build_network
#' @return A `peabiru_network` object, as [build_network()].
#' @family support functions
#' @export
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' # `setup_r5()` is deprecated: use `build_network()` instead
#' network <- suppressWarnings(setup_r5(data_path))
setup_r5 <- function(data_path,
                     verbose = FALSE,
                     temp_dir = FALSE,
                     elevation = "TOBLER",
                     overwrite = FALSE) {
  warning("`setup_r5()` is being deprecated in r5r and will be removed in a ",
          "future release. Please use `build_network()` instead.", call. = FALSE)
  build_network(data_path, verbose = verbose, temp_dir = temp_dir,
                elevation = elevation, overwrite = overwrite)
}

#' Stop running networks
#'
#' @description In r5r, `stop_r5()` stops the Java side of the given
#' `r5r_network` objects, or of all of them in the calling environment
#' when none is given, by removing the objects and running Java's garbage
#' collector. peabiru's networks live in R's memory: `stop_r5()` removes
#' the given network objects (or every network in the calling environment)
#' from the calling environment and runs R's garbage collector, which frees
#' the memory of a network no other object refers to. The messages and the
#' warning for an object that is not a network are r5r's.
#'
#' @param ... Networks built with [build_network()]. By default, if none is
#'   supplied, all the networks in the calling environment are removed.
#' @return No return value, called for side effects.
#' @family support functions
#' @export
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' stop_r5(network)
#' exists("network")
stop_r5 <- function(...) {
  supplied <- list(...)
  env <- parent.frame()
  objects <- mget(ls(envir = env), envir = env)
  running <- objects[vapply(objects, inherits, logical(1), what = "peabiru_network")]
  same_as_supplied <- function(x) {
    any(vapply(supplied, identical, logical(1), y = x))
  }
  if (length(supplied) == 0) {
    rm(list = names(running), envir = env)
    message("All peabiru networks have been successfully stopped.")
  } else {
    matches <- running[vapply(running, same_as_supplied, logical(1))]
    if (length(matches) >= 1) {
      rm(list = names(matches), envir = env)
      message(paste0(paste(names(matches), collapse = ", "),
                     " has been successfully stopped."))
    } else {
      unknown <- objects[vapply(objects, same_as_supplied, logical(1))]
      warning(paste0(paste(names(unknown), collapse = ", "),
                     " is not a peabiru network object."), call. = FALSE)
    }
  }
  gc(verbose = FALSE)
  invisible(NULL)
}

#' Manage cached files
#'
#' @description In r5r, `r5r_cache()` lists the R5 jar files r5r keeps in
#' its cache directory and deletes them. peabiru downloads and caches
#' nothing (the routing engine is compiled into the package), so the
#' listing is empty and there is nothing to delete; the arguments are
#' checked as r5r checks them.
#'
#' @param list_files Logical. Whether to print a message with the files
#'   cached locally. Defaults to `TRUE`.
#' @param delete_file String. The file name of a cached file to delete, or
#'   `"all"`. Defaults to `NULL`, so that no file is deleted.
#' @return A message saying that no files are cached; invisibly `NULL`.
#' @family support functions
#' @export
#' @examples
#' r5r_cache()
r5r_cache <- function(list_files = TRUE, delete_file = NULL) {
  # checkmate::assert_logical(list_files)
  if (!is.logical(list_files)) {
    fail_assert("list_files", "Must be of type 'logical'")
  }
  # checkmate::assert_character(delete_file, null.ok = TRUE)
  if (!is.null(delete_file) && !is.character(delete_file)) {
    fail_assert("delete_file", "Must be of type 'character' (or 'NULL')")
  }
  if (!is.null(delete_file)) {
    if (identical(delete_file, "all")) {
      message("No files are cached: peabiru does not download anything.")
    } else {
      message("The file '", delete_file, "' is not cached: peabiru does not ",
              "download anything.")
    }
  }
  if (isTRUE(list_files)) {
    message("Files currently cached:")
    message("(none: peabiru does not download anything)")
  }
  invisible(NULL)
}

#' Download R5.jar (nothing to download)
#'
#' @description In r5r, `download_r5()` downloads the R5 jar and returns
#' its path. peabiru needs neither Java nor R5's jar: the routing engine,
#' a port of R5 7.6 to 'Rust', is compiled into the package. So this
#' checks the arguments as r5r does, says there is nothing to download
#' (unless `quiet`) and returns `NULL` invisibly, which is what r5r's
#' `download_r5()` returns when the download fails.
#'
#' @param version A string, the version of R5 (ignored).
#' @param quiet A logical. Whether to keep quiet. Defaults to `FALSE`.
#' @param force_update A logical (ignored). Defaults to `FALSE`.
#' @param temp_dir A logical (ignored). Defaults to `FALSE`.
#' @return `NULL`, invisibly.
#' @family support functions
#' @export
#' @examples
#' download_r5()
download_r5 <- function(version = NULL,
                        quiet = FALSE,
                        force_update = FALSE,
                        temp_dir = FALSE) {
  for (a in c("quiet", "force_update", "temp_dir")) {
    if (!is.logical(get(a))) fail_assert(a, "Must be of type 'logical'")
  }
  if (!isTRUE(quiet)) {
    message("peabiru does not use Java or the R5 jar: there is nothing to ",
            "download.")
  }
  invisible(NULL)
}

#' Generate a situation report to help debug errors
#'
#' @description r5r's `r5r_sitrep()` reports the versions of r5r, of the
#' R5 jar and of Java, the memory set for Java and the session info. The
#' peabiru version reports what plays those roles here: the versions of
#' the package and of its 'Rust' core, the versions of R5 and r5r whose
#' results it reproduces, and the versions of R and of the platform (in
#' place of r5r's session info, which needs package 'utils': call
#' `sessionInfo()` for it).
#'
#' @return A `list` with `peabiru_package_version`, `core_version`,
#'   `r5_version`, `r5r_version`, `r_version` and `platform`.
#' @family support functions
#' @export
#' @examples
#' r5r_sitrep()
r5r_sitrep <- function() {
  list(
    peabiru_package_version = package_version(getNamespaceVersion("peabiru")),
    core_version = core_version(),
    r5_version = "7.6",
    r5r_version = "2.5.0",
    r_version = R.version.string,
    platform = R.version$platform
  )
}
