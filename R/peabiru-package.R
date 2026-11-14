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
#' inside the R session; no 'Java' is needed. What the package does not
#' support is an error that says so, never silently ignored.
#'
#' The routing engine is a 'Rust' port of Conveyal's 'R5' routing engine, and
#' the functions are modelled on those of the 'r5r' package (Ipea), from
#' whose 'Java' code the fare calculator and parts of the trip planner are
#' translated; see the references, and the article "Compared with r5r" on
#' the package website. peabiru is an independent project, not affiliated
#' with or endorsed by Conveyal, Ipea or the r5r team.
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
#'   `data.table`s: `sf` `data.table`s for isochrone polygons,
#'   itineraries, the street edges and the transit routes, plain `sf` data
#'   frames for isochrone lines, street vertices and transit stops, and an
#'   index on the column `found` of [find_snap()]. A good choice for large
#'   matrices handled with 'data.table'.
#'
#' Any other value is an error. The option applies to every table: the
#' matrices, accessibility, Pareto frontiers, [find_snap()],
#' [check_transit_availability()], [get_gtfs_errors()], the tables of a fare
#' structure, the tables of [trip_options()] and the `sf` results (whose
#' geometries are always in EPSG:4326). The rows, the columns, their order
#' and types, and the row names are the same whatever the class.
#'
#' @references
#' Conway, M. W., Byrd, A. and van der Linden, M. (2017). Evidence-based
#' transit and land use sketch planning using interactive accessibility
#' methods on combined schedule and headway-based networks. *Transportation
#' Research Record*, 2653(1), 45-53. \doi{10.3141/2653-06}
#'
#' Conway, M. W., Byrd, A. and van Eggermond, M. (2018). Accounting for
#' uncertainty and variation in accessibility metrics for public transport
#' sketch planning. *Journal of Transport and Land Use*, 11(1).
#' \doi{10.5198/jtlu.2018.1074}
#'
#' Conway, M. W. and Stewart, A. F. (2019). Getting Charlie off the MTA: a
#' multiobjective optimization method to account for cost constraints in
#' public transit accessibility metrics. *International Journal of
#' Geographical Information Science*, 33(9), 1759-1787.
#' \doi{10.1080/13658816.2019.1605075}
#'
#' Delling, D., Pajor, T. and Werneck, R. F. (2015). Round-based public
#' transit routing. *Transportation Science*, 49(3), 591-604.
#' \doi{10.1287/trsc.2014.0534}
#'
#' Geisberger, R., Sanders, P., Schultes, D. and Vetter, C. (2012). Exact
#' routing in large road networks using contraction hierarchies.
#' *Transportation Science*, 46(3), 388-404. \doi{10.1287/trsc.1110.0401}
#'
#' Pereira, R. H. M., Saraiva, M., Herszenhut, D., Braga, C. K. V. and Conway,
#' M. W. (2021). r5r: Rapid Realistic Routing on Multimodal Transport Networks
#' with R5 in R. *Findings*. \doi{10.32866/001c.21262}
#'
#' @section Sample data:
#' `system.file("extdata/spo", package = "peabiru")` holds a small network
#' for examples and tests, cut from the Sao Paulo sample data of the r5r
#' project: `spo_osm.pbf`, the streets around Avenida Paulista (data
#' (c) OpenStreetMap contributors, available under the Open Database
#' License, <https://www.openstreetmap.org/copyright>); `spo_gtfs.zip`, the
#' SPTrans frequency-based 'GTFS' feed of that sample, without
#' `shapes.txt`; and `spo_points.csv`, 27 points of its hexagonal grid with
#' their population and opportunity counts.
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
