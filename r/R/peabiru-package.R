#' peabiru: multimodal routing on OpenStreetMap and GTFS networks
#'
#' peabiru provides the routing functions of the 'r5r' package --
#' [build_network()], [travel_time_matrix()], [accessibility()],
#' [expanded_travel_time_matrix()] and [arrival_travel_time_matrix()] -- with
#' the same arguments, defaults and results, computed by a 'Rust' port of the
#' 'R5' routing engine that r5r 2.5.0 drives (R5 7.6). No 'Java' is needed.
#' Options that peabiru does not support yet are errors, never silently
#' ignored.
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
