# peabiru (development version)

The package now exports all 22 functions of r5r 2.5.0, with r5r's formals
(names, order and defaults), and gives r5r's results.

## New functions

* `detailed_itineraries()`: the legs of each
  trip with their geometry, the fastest trip or the alternatives
  (`shortest_path = FALSE`, `suboptimal_minutes`), `all_to_all`,
  `drop_geometry`, `osm_link_ids` and `output_dir`.
* Fares: `setup_fare_structure()`, `read_fare_structure()`,
  `write_fare_structure()`, and `fare_structure` and `max_fare` in
  `travel_time_matrix()`, `accessibility()` and `detailed_itineraries()`,
  routed with R5's multi-criteria router (McRAPTOR); `pareto_frontier()`.
* Network inspection: `find_snap()`, `street_network_bbox()`,
  `street_network_to_sf()`, `transit_network_to_sf()`,
  `check_transit_availability()` and `get_gtfs_errors()`.
* r5r's functions for Java, kept so that r5r scripts run: `setup_r5()`,
  `stop_r5()`, `download_r5()`, `r5r_cache()` and `r5r_sitrep()`.

## New arguments and inputs

* Public transport by specific modes (`"BUS"`, `"RAIL"`, ...), and
  bicycle or car access and egress (`mode`, `mode_egress`, `max_bike_time`,
  `max_car_time`).
* `output_dir` in every function that has it in r5r except
  `pareto_frontier()`: one CSV file per origin (or pair), byte for byte the
  files r5r writes, routed and written a slice at a time.
* Scenarios: `new_carspeeds` (by 'OpenStreetMap' way or by polygon),
  `carspeed_scale` and `new_lts` (by way or by line).
* Elevation: a `.tif` raster in the data directory gives walking and
  cycling slope costs (`elevation = "TOBLER"` or `"MINETTI"`), in
  EPSG:4326 or the coordinate systems used for Brazil and worldwide whose
  transform is reproduced exactly (SIRGAS 2000, SAD69, Corrego Alegre and
  their UTM zones, SIRGAS 2000 / Brazil Polyconic, WGS 84 / UTM,
  Pseudo-Mercator). Where r5r's pixel depends on the machine, peabiru reads
  the one Java's `StrictMath` gives, with a warning; voids and values beyond
  +/-3276.7 m are stored wrapped around as r5r stores them, with a warning.
* 'GTFS' feeds are read as gtfs-lib reads them, including the feeds it
  loads with missing columns, short times or empty dates, and GTFS-Flex
  `locations.geojson`; feeds with errors of priority `"HIGH"` are refused,
  as r5r refuses them (and `get_gtfs_errors()` lists the errors).

## Changes

* Deprecated arguments (`r5r_core`, and `sample_size` in `isochrone()`)
  have r5r's default, `deprecated()`; passing `deprecated()` counts as not
  giving the argument, as in r5r.
* Argument errors have r5r's messages, word for word.
* Isochrone polygons follow the C++ library 'isoband' is built with on each
  platform, so they equal r5r's on Linux and Windows too.
* Street edge lengths are computed with Apache Commons Math's `FastMath`, as
  R5 computes them.
* Park-and-ride areas link to the street as R5 links them.
* `detailed_itineraries()` follows rides along GTFS shapes cut by
  `shape_dist_traveled` as R5 cuts them, and plans car egress through turn
  restrictions with two or more via ways as r5r does on a freshly built
  network, with a warning naming the pairs whose result in r5r depends on
  what ran before (both were errors).
* `write_fare_structure()` deflates the archive, as r5r's does.
* The reference documentation describes each function on its own terms,
  with a "Compatibility with r5r" section; new articles on itineraries,
  elevation, scenarios and inspecting a network, and "Coming from r5r" lists
  every deliberate difference.
* A test compares the formals of every function with a snapshot of r5r
  2.5.0's (`tests/testthat/fixtures/r5r-formals.csv`).

# peabiru 0.0.1

* First release: `build_network()`, `travel_time_matrix()`,
  `accessibility()`, `expanded_travel_time_matrix()`,
  `arrival_travel_time_matrix()` and `isochrone()`.
