# Changelog

## peabiru 0.1.5

- New function
  [`locate_facilities()`](../reference/locate_facilities.md): where to
  open new facilities among candidate sites, by maximal covering,
  location set covering, p-median or p-center, on the travel times from
  the demand points to the sites by any mode (those of
  [`travel_time_matrix()`](../reference/travel_time_matrix.md), or a
  table given in `travel_times`). Existing facilities count first. The
  models are solved by a greedy construction followed by vertex
  substitution, with no solver; the result has the new sites with what
  each adds, the potential of every candidate, the facility and travel
  time of every demand point, and a summary. See the article “Facility
  location”.
- New function
  [`floating_catchment()`](../reference/floating_catchment.md): the
  two-step floating catchment area (supply per unit of demand at each
  demand point) and its enhanced form, with any of the decay functions
  of [`accessibility()`](../reference/accessibility.md).
- Credits: the `Title` and `Description`, the README and the package
  help page now state up front that the engine is a ‘Rust’ port of
  Conveyal’s ‘R5’ routing engine and that the functions are modelled on
  those of ‘r5r’ (Ipea), from whose ‘Java’ code the fare calculator and
  parts of the trip planner are translated; the reference of ‘r5r’
  (Pereira et al., 2021) is cited; and the README says that peabiru is
  not affiliated with or endorsed by Conveyal, Ipea or the r5r team. No
  change in the code.

## peabiru 0.1.0

### Breaking changes

- The first argument of every function that takes a network is now
  `network` (it was `r5r_network`). Calls that pass the network by
  position are not affected.
- The arguments `r5r_core` (every function) and `sample_size`
  ([`isochrone()`](../reference/isochrone.md)), which were accepted with
  a warning and ignored, are gone.

### New functions

- [`trip_options()`](../reference/trip_options.md), a multi-criteria
  trip planner: for each pair, the trips not dominated on total time,
  fare, transfers and minutes on foot (any subset) at each departure
  minute of the window, merged into options, with their legs in the
  format of
  [`detailed_itineraries()`](../reference/detailed_itineraries.md).
  Exact on its trip definition, fares compared soundly for any fare
  structure, and frequency-based routes boarded at their expected times
  (no random draws).
- [`detailed_itineraries()`](../reference/detailed_itineraries.md): the
  legs of each trip with their geometry, the fastest trip or the
  alternatives (`shortest_path = FALSE`, `suboptimal_minutes`),
  `all_to_all`, `drop_geometry`, `osm_link_ids` and `output_dir`.
- Fares:
  [`setup_fare_structure()`](../reference/setup_fare_structure.md),
  [`read_fare_structure()`](../reference/read_fare_structure.md),
  [`write_fare_structure()`](../reference/write_fare_structure.md), and
  `fare_structure` and `max_fare` in
  [`travel_time_matrix()`](../reference/travel_time_matrix.md),
  [`accessibility()`](../reference/accessibility.md) and
  [`detailed_itineraries()`](../reference/detailed_itineraries.md),
  routed with the multi-criteria router (McRAPTOR);
  [`pareto_frontier()`](../reference/pareto_frontier.md).
- Network inspection: [`find_snap()`](../reference/find_snap.md),
  [`street_network_bbox()`](../reference/street_network_bbox.md),
  [`street_network_to_sf()`](../reference/street_network_to_sf.md),
  [`transit_network_to_sf()`](../reference/transit_network_to_sf.md),
  [`check_transit_availability()`](../reference/check_transit_availability.md)
  and [`get_gtfs_errors()`](../reference/get_gtfs_errors.md).

### New arguments and inputs

- Public transport by specific modes (`"BUS"`, `"RAIL"`, …), and bicycle
  or car access and egress (`mode`, `mode_egress`, `max_bike_time`,
  `max_car_time`).
- `output_dir` in the matrix functions,
  [`accessibility()`](../reference/accessibility.md) and
  [`detailed_itineraries()`](../reference/detailed_itineraries.md): one
  CSV file per origin (or pair), routed and written a slice at a time.
- Scenarios: `new_carspeeds` (by ‘OpenStreetMap’ way or by polygon),
  `carspeed_scale` and `new_lts` (by way or by line).
- Elevation: a `.tif` raster in the data directory gives walking and
  cycling slope costs (`elevation = "TOBLER"` or `"MINETTI"`), in
  EPSG:4326 or the coordinate systems used for Brazil and worldwide
  whose transform is reproduced exactly (SIRGAS 2000, SAD69, Corrego
  Alegre and their UTM zones, SIRGAS 2000 / Brazil Polyconic, WGS 84 /
  UTM, Pseudo-Mercator). Where the pixel a street point falls in depends
  on the last bits of the projection, peabiru reads the same pixel on
  every machine, with a warning; voids and values beyond +/-3276.7 m are
  stored wrapped around, with a warning.
- ‘GTFS’ feeds with missing optional columns, short times or empty dates
  are read, and so is GTFS-Flex `locations.geojson`; feeds with errors
  of priority `"HIGH"` are refused (and
  [`get_gtfs_errors()`](../reference/get_gtfs_errors.md) lists the
  errors).

### Changes

- [`isochrone()`](../reference/isochrone.md): when
  [`sf::st_make_valid()`](https://r-spatial.github.io/sf/reference/valid.html)
  with spherical geometry (s2) changes the area of a polygon by more
  than one part in 10,000 (it can, by several per cent, in polygons of
  many parts and holes), the polygons of that origin are repaired on the
  plane instead, with a warning of class `peabiru_isochrone_repair`.
  `options(peabiru.isochrone_repair = "spherical")` keeps the spherical
  repair.
- The class of the tables returned is chosen by
  `options(peabiru.output = )`, read at each call: `"tibble"` (the
  default), `"data.frame"` (what every function returned before) or
  `"data.table"` (`sf` `data.table`s for isochrone polygons,
  itineraries, street edges and transit routes; an index on `found` in
  [`find_snap()`](../reference/find_snap.md)). It applies to every
  table, `sf` results and the tables of fare structures and of
  [`trip_options()`](../reference/trip_options.md) included. ‘tibble’ is
  now imported; ‘data.table’ stays suggested.
- An `sf` of points (`origins`, `destinations`, `points`) or of scenario
  polygons or lines (`new_carspeeds`, `new_lts`) in another coordinate
  reference system than WGS 84 is reprojected with
  [`sf::st_transform()`](https://r-spatial.github.io/sf/reference/st_transform.html),
  with a message of class `peabiru_reprojected` naming the argument and
  the CRS (it was an error). An `sf` without a CRS is an error that says
  how to set it. Results stay in EPSG:4326.
- [`read_fare_structure()`](../reference/read_fare_structure.md) keeps
  the columns of a table without rows in the file (it returned a table
  without columns).
- Arguments are checked with the checks and messages of ‘checkmate’ and
  ‘cli’ (ported, without the dependencies).
- The error for exactly 200 Monte Carlo draws with fares has class
  `peabiru_200_draws`.
- Isochrone polygons follow the C++ library ‘isoband’ is built with on
  each platform, so that on each system they are the polygons ‘isoband’
  gives there.
- Street edge lengths are computed with a translation of Apache Commons
  Math’s `FastMath`, the same on every platform.
- Park-and-ride areas link to the first vertex of the street edge they
  meet.
- [`detailed_itineraries()`](../reference/detailed_itineraries.md)
  follows rides along GTFS shapes cut by `shape_dist_traveled`, and
  plans car egress through turn restrictions with two or more via ways
  in a fixed order (that of a freshly built network), with a warning
  naming the pairs whose itinerary another order would change (both were
  errors).
- [`write_fare_structure()`](../reference/write_fare_structure.md)
  deflates the archive.
- What peabiru does not do “is not supported by peabiru” (no “yet”).
- Under `R CMD check` with its limit on cores (`_R_CHECK_LIMIT_CORES_`),
  the default `n_threads = Inf` uses two threads instead of all cores.
- The reference documentation describes each function on its own terms;
  new articles on itineraries, elevation, scenarios and inspecting a
  network. The article “Compared with r5r” says what the package shares
  with r5r, what differs and how to move a script over.

## peabiru 0.0.1

- First release: [`build_network()`](../reference/build_network.md),
  [`travel_time_matrix()`](../reference/travel_time_matrix.md),
  [`accessibility()`](../reference/accessibility.md),
  [`expanded_travel_time_matrix()`](../reference/expanded_travel_time_matrix.md),
  [`arrival_travel_time_matrix()`](../reference/arrival_travel_time_matrix.md)
  and [`isochrone()`](../reference/isochrone.md).
