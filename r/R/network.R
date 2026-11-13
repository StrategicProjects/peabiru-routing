#' Build a transport network
#'
#' Builds a multimodal transport network from a directory with one
#' 'OpenStreetMap' extract (`.pbf`), zero or more 'GTFS' feeds (`.zip`)
#' and, optionally, an elevation raster (`.tif`). The network lives in
#' memory and is reused by every routing function; nothing is written to
#' `data_path`. The files are recognised by their extension in lower case
#' only: a `map.PBF` or a `feed.ZIP` is ignored.
#'
#' @section Elevation:
#' A `.tif` raster in `data_path` gives walking and cycling times slope
#' costs (`elevation = "TOBLER"`, the default, or `"MINETTI"`; never for
#' cars): the raster is sampled every 10 m along each street at the nearest
#' pixel, and each street's time is multiplied by the average cost of its
#' slopes. Points outside the raster count as 0 m high. Without a `.tif`
#' raster the network is built with `elevation = "NONE"`.
#'
#' The raster must be a GeoTIFF with one band, north up, uncompressed or
#' compressed with LZW, Deflate or PackBits (what GDAL writes by default),
#' in WGS 84 longitude and latitude (EPSG:4326) or another coordinate
#' reference system given by its EPSG code (what GDAL writes) whose
#' transform is reproduced exactly: SIRGAS 2000 (4674), SAD69 (4618),
#' Corrego Alegre 1970-72 (4225), their UTM zones used in Brazil
#' (31965-31985, 6210, 6211, 5396; 29168-29172, 29187-29195; 22521-22525),
#' SIRGAS 2000 / Brazil Polyconic (5880), WGS 84 / UTM (32601-32660,
#' 32701-32760) and WGS 84 / Pseudo-Mercator (3857). Other rasters are an
#' error that says what to change, and so are a `.prj` file next to the
#' raster, a raster whose name ends in `.tiff` and more than one raster.
#'
#' Elevations are kept in decimetres in 16-bit integers:
#' voids and values beyond +/-3276.7 m wrap around (SRTM's -32768 voids
#' become 0 m, -9999 becomes 3108.2 m, a `NaN` no-data value 0 m), with a
#' warning counting them and the samples equal to the raster's no-data
#' value.
#'
#' In a system other than EPSG:4326 the pixel read for a street point
#' within nanometres (5e-8 m, or 2e-13 degrees) of the boundary between two
#' pixels, or of the raster's edge, depends on the last bits of the sines
#' and cosines of the projection. peabiru computes them with its own
#' 'fdlibm' functions, so the result is the same on every machine, and warns
#' once with the number of such points (about one in a hundred million for
#' 30 m pixels; reprojecting the raster to EPSG:4326 removes the
#' ambiguity). A point that falls on the raster's outer edge (inside its
#' envelope, past the last pixel) is an error, since there is no pixel to
#' read.
#'
#' A raster also turns off the swap of origins and destinations of walking
#' matrices (see [travel_time_matrix()]), and a routing call with a
#' scenario (`new_carspeeds`, `carspeed_scale`, `new_lts`) runs without the
#' slope costs, with a warning.
#'
#' @section Public transport:
#' The 'GTFS' feeds are loaded in the order in which the operating system
#' lists the directory. With more than one feed that order depends on the
#' file system, so the same directory can give slightly different expanded
#' and arrival matrices on two machines (another stop or route for a few
#' trips); on one machine the results are always the same.
#'
#' The network is not built when a 'GTFS' feed has errors of priority
#' `"HIGH"` (a reference to a stop, route, trip, service or agency that does
#' not exist, a number that cannot be read, tables in a subfolder of the
#' `.zip`, routes without any agency): the error names the feeds and points
#' to [get_gtfs_errors()], which lists the errors. A feed that cannot be
#' loaded at all is an error too.
#'
#' @section The network object:
#' The network is held in memory by the 'Rust' engine and is always built
#' anew: there is no cache on disk, and a network cannot be saved with
#' `saveRDS()` or carried to another session (a reloaded one is refused
#' with an error; build it again). More than one `.pbf` file in `data_path`
#' is an error.
#'
#' @param data_path A string, the directory with the `.pbf` file, the
#'   'GTFS' feeds and the elevation raster.
#' @param verbose A logical. Whether to report the build of the network in
#'   messages. Defaults to `FALSE`.
#' @param temp_dir A logical, checked but without effect: nothing is
#'   downloaded or cached.
#' @param elevation A string, the slope cost function for walking and
#'   cycling when `data_path` has a `.tif` elevation raster: `"TOBLER"` (the
#'   default; Tobler's hiking function), `"MINETTI"` (Minetti's energy cost
#'   of walking) or `"NONE"` (no slope costs).
#' @param overwrite A logical, checked but without effect: the network is
#'   always built anew.
#' @param n_threads The number of threads used to read the `.pbf` file.
#'   Defaults to `Inf`, all cores (two under `R CMD check` when it limits
#'   the cores).
#'
#' @return A `peabiru_network` object to pass to the routing functions
#'   (printing it summarises the network).
#' @family network functions
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
  cm_assert(check_directory_exists(data_path), "data_path")
  cm_assert(check_logical(verbose), "verbose")
  cm_assert(check_logical(temp_dir), "temp_dir")
  cm_assert(check_character(elevation), "elevation")
  cm_assert(check_logical(overwrite), "overwrite")
  # one value, not NA
  assert_flag(verbose, "verbose")
  threads <- assign_n_threads(n_threads)

  elevation <- toupper(elevation)
  valid_elev <- c("TOBLER", "MINETTI", "NONE")
  if (!elevation %in% valid_elev) {
    cli_abort_text(paste0("Invalid value for `elevation`: ", cli_val(elevation), "."),
                   c(x = paste0("Must be one of: ", cli_val(valid_elev))))
  }

  shown_path <- path.expand(data_path) # as the messages name it
  data_path <- normalizePath(path.expand(data_path), mustWork = TRUE)
  files <- list.files(data_path)
  # The inputs are looked for with these patterns. They are regular
  # expressions, so ".tif" also matches a name such as "identificacao.csv";
  # any_tif keeps that loose match (it turns off the swap of origins and
  # destinations, see swap_od()), so that one rule decides both.
  any_pbf <- length(grep(".pbf", files)) > 0
  any_tif <- length(grep(".tif", files)) > 0
  # lower case only, as the engine matches the extension
  pbf <- files[grepl("\\.pbf$", files)]
  if (!any_pbf || length(pbf) == 0) {
    cli_abort_text(paste0("An OSM '.pbf' file is required in '", shown_path,
                          "' to build a network."))
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
  }

  t0 <- proc.time()[["elapsed"]]
  ptr <- rs_build_network(data_path, threads, elevation)
  # the model in use: "NONE" when no file ends in .tif (R5 finds no raster)
  network <- structure(
    list(ptr = ptr, data_path = data_path,
         elevation = rs_network_info(ptr)$elevation, has_tif = any_tif),
    class = "peabiru_network"
  )
  for (w in rs_network_build_warnings(ptr)) {
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
