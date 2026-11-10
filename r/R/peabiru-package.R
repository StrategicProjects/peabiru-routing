#' peabiru: multimodal routing on OpenStreetMap and GTFS networks
#'
#' peabiru builds a network from an 'OpenStreetMap' extract and 'GTFS' feeds
#' with [build_network()] and computes travel times by walking, cycling,
#' driving and public transport with [travel_time_matrix()],
#' [accessibility()], [expanded_travel_time_matrix()] and
#' [arrival_travel_time_matrix()]. The routing engine is written in 'Rust' and
#' runs inside the R session; no 'Java' is needed. Options that peabiru does
#' not support yet are errors, never silently ignored.
#'
#' The functions have the names, arguments and defaults of the 'r5r' package
#' (2.5.0), and the engine is a port of 'R5' 7.6, the engine r5r drives, so
#' the results are the same.
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
