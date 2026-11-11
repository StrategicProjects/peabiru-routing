#' Build a transport network
#'
#' Builds a multimodal transport network from a directory with one
#' 'OpenStreetMap' extract (`.pbf`) and zero or more 'GTFS' feeds (`.zip`),
#' as `r5r::build_network()` does. The network lives in memory and is reused
#' by every routing function; nothing is written to `data_path`, and there is
#' no `network.dat` cache.
#'
#' As in r5r, the files are recognised by their extension in lower case only:
#' a `map.PBF` or a `feed.ZIP` is ignored.
#'
#' Elevation is not supported yet. As in r5r, when the directory has no
#' `.tif` raster the network is built with `elevation = "NONE"`; when it has
#' one (a file ending in `.tif` or `.tiff`), `elevation = "NONE"` must be
#' given explicitly (and, as in r5r, the
#' raster still disables the swap of origins and destinations that r5r does
#' for walking).
#'
#' The 'GTFS' feeds are loaded in the order in which the operating system
#' lists the directory, as R5 does. With more than one feed that order
#' depends on the file system, so the same directory can give slightly
#' different expanded and arrival matrices on two machines (another stop or
#' route for a few trips); r5r behaves in the same way, and on one machine
#' the results are always the same.
#'
#' As in r5r, the network is not built when a 'GTFS' feed has errors of
#' priority `"HIGH"` (a reference to a stop, route, trip, service or agency
#' that does not exist, a number that cannot be read, tables in a subfolder of
#' the `.zip`, routes without any agency): the error names the feeds and
#' points to [get_gtfs_errors()], which lists the errors. Unlike r5r,
#' nothing is written to `data_path` (r5r writes them to `gtfs_errors.csv`).
#' A feed that r5r's 'GTFS' library fails to load at all is an error too.
#'
#' @param data_path A string, the directory with the `.pbf` file and the
#'   'GTFS' feeds.
#' @param verbose A logical. Whether to show informative messages.
#' @param temp_dir A logical. Accepted for compatibility with r5r, which uses
#'   it to choose where to keep the R5 jar; it has no effect.
#' @param elevation A string, `"TOBLER"` (the default), `"MINETTI"` or
#'   `"NONE"`. Only `"NONE"` can be routed; the default is fine when the
#'   directory has no `.tif` raster.
#' @param overwrite A logical. Accepted for compatibility with r5r, which uses
#'   it to rebuild a cached `network.dat`; the network is always built anew.
#' @param n_threads The number of threads used to read the `.pbf` file.
#'   Defaults to `Inf`, all cores.
#'
#' @return A `peabiru_network` object to pass to the routing functions.
#' @export
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' network
build_network <- function(data_path,
                          verbose = FALSE,
                          temp_dir = FALSE,
                          elevation = "TOBLER",
                          overwrite = FALSE,
                          n_threads = Inf) {
  assert_string(data_path, "data_path")
  if (!dir.exists(data_path)) {
    fail_assert("data_path", paste0("Directory '", data_path, "' does not exist"))
  }
  assert_flag(verbose, "verbose")
  assert_flag(temp_dir, "temp_dir")
  assert_string(elevation, "elevation")
  assert_flag(overwrite, "overwrite")
  threads <- assign_n_threads(n_threads)

  # r5r set_elevation()
  elevation <- toupper(elevation)
  valid_elev <- c("TOBLER", "MINETTI", "NONE")
  if (!elevation %in% valid_elev) {
    stop("Invalid value for 'elevation': \"", elevation, "\". Must be one of: ",
         paste0("\"", valid_elev, "\"", collapse = ", "), ".", call. = FALSE)
  }

  data_path <- normalizePath(path.expand(data_path), mustWork = TRUE)
  files <- list.files(data_path)
  # r5r build_network() looks for its inputs with these patterns. They are
  # regular expressions, so ".tif" also matches a name such as
  # "identificacao.csv"; any_tif keeps that loose match because r5r uses it
  # to turn off the swap of origins and destinations (see swap_od()).
  any_pbf <- length(grep(".pbf", files)) > 0
  any_tif <- length(grep(".tif", files)) > 0
  # an elevation raster that R5 would read
  rasters <- files[grepl("\\.tiff?$", files, ignore.case = TRUE)]
  # lower case only, as r5r and the engine match the extension
  pbf <- files[grepl("\\.pbf$", files)]
  if (!any_pbf || length(pbf) == 0) {
    stop("An OSM .pbf file is required in '", data_path, "' to build a network.",
         call. = FALSE)
  }
  if (length(pbf) > 1) {
    stop("'", data_path, "' has more than one .pbf file (",
         paste(pbf, collapse = ", "), "); keep only one.", call. = FALSE)
  }

  if (!any_tif) {
    elevation <- "NONE"
    if (verbose) {
      message("No raster .tif files found in '", data_path,
              "'. Using elevation = \"NONE\".")
    }
  } else if (length(rasters) == 0) {
    # r5r keeps the elevation model, and R5 finds no raster to apply it to
    elevation <- "NONE"
  } else if (elevation != "NONE") {
    stop("Elevation is not supported by peabiru yet, and '", data_path,
         "' has the raster ", paste0("'", rasters, "'", collapse = ", "),
         ". Use elevation = \"NONE\" to build the network without it.",
         call. = FALSE)
  }

  t0 <- proc.time()[["elapsed"]]
  ptr <- rs_build_network(data_path, threads)
  network <- structure(
    list(ptr = ptr, data_path = data_path, elevation = elevation,
         has_tif = any_tif),
    class = "peabiru_network"
  )
  for (w in rs_network_r5r_warnings(ptr)) {
    warning(w, call. = FALSE)
  }
  if (verbose) {
    message("Finished building network at '", data_path, "' in ",
            format(round(proc.time()[["elapsed"]] - t0, 2)), " s.")
  }
  network
}

#' @export
print.peabiru_network <- function(x, ...) {
  cat("<peabiru_network>\n")
  cat("  data path: ", x$data_path, "\n", sep = "")
  if (!rs_network_valid(x$ptr)) {
    cat("  (no longer valid: networks cannot be saved and reloaded; build it again)\n")
    return(invisible(x))
  }
  info <- rs_network_info(x$ptr)
  cat("  streets:   ", format(info$vertices, big.mark = ","), " vertices, ",
      format(info$edges, big.mark = ","), " edges\n", sep = "")
  if (length(info$feeds) == 0) {
    cat("  transit:   none (no GTFS feed)\n")
  } else {
    cat("  transit:   ", length(info$feeds), " feed(s) (",
        paste(info$feeds, collapse = ", "), "), ",
        format(info$stops, big.mark = ","), " stops, ",
        format(info$patterns, big.mark = ","), " patterns, ",
        format(info$services, big.mark = ","), " services",
        if (info$frequencies) ", frequency-based trips" else "", "\n", sep = "")
  }
  cat("  elevation: ", x$elevation, "\n", sep = "")
  invisible(x)
}
