#' peabiru: multimodal routing on OpenStreetMap and GTFS networks
#'
#' peabiru builds a network from an 'OpenStreetMap' extract, 'GTFS' feeds
#' and an optional elevation raster with [build_network()], and computes,
#' by walking, cycling, driving and public transport:
#' travel-time matrices ([travel_time_matrix()],
#' [expanded_travel_time_matrix()], [arrival_travel_time_matrix()]),
#' accessibility ([accessibility()]), isochrones ([isochrone()]) and
#' detailed itineraries ([detailed_itineraries()]), with or without fares
#' ([setup_fare_structure()], [read_fare_structure()],
#' [write_fare_structure()], [pareto_frontier()]) and scenarios of car
#' speeds and traffic stress. It inspects a network with [find_snap()],
#' [street_network_bbox()], [street_network_to_sf()],
#' [transit_network_to_sf()], [check_transit_availability()] and
#' [get_gtfs_errors()]. The routing engine is written in 'Rust' and runs
#' inside the R session; no 'Java' is needed. What the engine cannot do
#' exactly is an error, never silently ignored.
#'
#' @section Compatibility with r5r:
#' The engine is a port of Conveyal's 'R5' 7.6, the engine the 'r5r'
#' package drives, and the package exports the 22 functions of r5r 2.5.0
#' with their arguments and defaults, so the results are r5r's (compared
#' with `identical()` in the tests). r5r's functions that manage 'Java' and
#' the R5 jar ([setup_r5()], [stop_r5()], [download_r5()], [r5r_cache()],
#' [r5r_sitrep()]) are kept so that r5r scripts run. See
#' the article "Coming from r5r" on the package website for the differences.
#'
#' @section Sample data:
#' `system.file("extdata/spo", package = "peabiru")` holds a small network
#' for examples and tests, cut from the Sao Paulo sample data of the 'r5r'
#' package: `spo_osm.pbf`, the streets around Avenida Paulista (data
#' (c) OpenStreetMap contributors, available under the Open Database
#' License, <https://www.openstreetmap.org/copyright>); `spo_gtfs.zip`, the
#' SPTrans frequency-based 'GTFS' feed shipped with r5r, without
#' `shapes.txt`; and `spo_points.csv`, 27 points of r5r's hexagonal grid
#' with their population and opportunity counts.
#'
#' @keywords internal
#' @useDynLib peabiru, .registration = TRUE
"_PACKAGE"

#' Version of the Rust routing core
#'
#' @return A string, the version of the `peabiru-core` crate this package was
#'   built against.
#' @export
#' @examples
#' core_version()
core_version <- function() rs_core_version()

# A routing call that an interrupt (Ctrl-C, the Stop button) or an error
# raised while it waited (a limit from setTimeLimit()) stopped returns a
# `peabiru_stopped` object once its workers have stopped; raise its
# condition here. An interrupt goes to the handlers of the calling code,
# as R does on Ctrl-C, and if none takes it, back to the top level.
raise_if_stopped <- function(res) {
  if (!inherits(res, "peabiru_stopped")) {
    return(res)
  }
  cond <- res$condition
  if (is.null(cond)) {
    cond <- structure(list(), class = c("interrupt", "condition"))
  }
  if (!inherits(cond, "interrupt")) {
    stop(cond)
  }
  signalCondition(cond)
  # what R prints for an interrupt nobody handles
  cat("\n", file = stderr())
  invokeRestart("abort")
}
