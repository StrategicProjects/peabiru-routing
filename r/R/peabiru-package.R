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
#' @section Class of the results:
#' The tables the functions return are tibbles by default. The option
#' `peabiru.output`, read at each call, chooses another class:
#'
#' * `options(peabiru.output = "tibble")` (the default; 'tibble' is the
#'   package's one dependency): tibbles, and `sf` tibbles (class
#'   `c("sf", "tbl_df", "tbl", "data.frame")`) for the results with a
#'   geometry, as `sf::st_as_sf()` makes of a tibble;
#' * `options(peabiru.output = "data.frame")`: base `data.frame`s, and plain
#'   `sf` data frames;
#' * `options(peabiru.output = "data.table")` (needs 'data.table'):
#'   `data.table`s, with the class r5r 2.5.0 gives each result, so that the
#'   result is identical to r5r's: `sf` `data.table`s for isochrone
#'   polygons, itineraries, the street edges and the transit routes, plain
#'   `sf` data frames for isochrone lines, street vertices and transit
#'   stops, and the index r5r's [find_snap()] leaves on `found`. A good
#'   choice for large matrices handled with 'data.table'.
#'
#' Any other value is an error. The option applies to every table: the
#' matrices, accessibility, Pareto frontiers, [find_snap()],
#' [check_transit_availability()], [get_gtfs_errors()], the tables of a fare
#' structure, the tables of [trip_options()] and the `sf` results (whose
#' geometries are always in EPSG:4326). The rows, the columns, their order
#' and types, and the row names are the same whatever the class.
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
