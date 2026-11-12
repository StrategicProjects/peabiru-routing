# Changelog

Changes of the Rust crate `peabiru-core`, the command-line tool, and the
Python and JavaScript packages. The R package has its own `r/NEWS.md`.

## Development version

The three packages now cover all of r5r 2.5.0: the R package exports its 22
functions with r5r's formals, and Python and JavaScript have the same
routing and network functions (without r5r's functions for Java).

### Routing and inputs (all packages)

- `trip_options()` (Python, R; `tripOptions()` in JavaScript; the core's
  `trip_options` and `write_trip_options_csv`; `peabiru options` on the
  command line): peabiru's own multi-criteria trip planner, with no r5r
  counterpart. The trips not dominated on total time, fare, transfers and
  time on foot at each departure minute, merged into options, with their
  legs; frequency-based routes at their expected times. GTFS `exact_times`
  is now read (only this function uses it).
- Public transport by any subset of modes (`BUS`, `RAIL`, ...); bicycle and
  car access and egress, with `max_bike_time` and `max_car_time`.
- Detailed itineraries: the fastest trip or McRAPTOR alternatives
  (`shortest_path`, `suboptimal_minutes`), `all_to_all`, `drop_geometry`,
  `osm_link_ids`, scenarios and fares; CSV output in R and Python.
- Fares: fare structures (setup, read, write), `fare_structure` and
  `max_fare` in matrices, accessibility and itineraries (McRAPTOR), and the
  Pareto frontier of travel time and fare.
- Scenarios: car speeds by way or by polygon, `carspeed_scale`, levels of
  traffic stress by way or by line.
- Elevation: Tobler and Minetti slope costs from a GeoTIFF, in EPSG:4326 or
  SIRGAS 2000, SAD69, Corrego Alegre and their UTM zones, SIRGAS 2000 /
  Brazil Polyconic, WGS 84 / UTM and Pseudo-Mercator; voids and values beyond
  +/-3276.7 m wrap around as in R5, with a warning; where r5r's pixel depends
  on the machine, Java's `StrictMath` decides, with a warning.
- Network inspection: snapping, the bounding box, the street and transit
  networks as geometries, the dates with service, and the GTFS errors.
- GTFS is read as gtfs-lib reads it (including GTFS-Flex
  `locations.geojson`), and feeds with errors of priority HIGH are refused.
- Isochrone rings follow libstdc++'s order on Linux and Windows, as r5r's do
  there. Park-and-ride areas link to the street as in R5; edge lengths use
  Apache Commons Math's `FastMath`.
- Argument errors have r5r's messages, word for word.

### Rust crate and CLI

- API: `find_snap`, `StreetNetwork::bbox`, `street_vertices`,
  `transit_stops`, `transit_routes`, `transit_service_dates`, `gtfs_errors`,
  `detailed_itineraries`, `write_itineraries_csv`, `Scenario` and its parts,
  `FareStructure` with `setup_`/`read_`/`write_fare_structure`,
  `pareto_frontier`, `Elevation` and `build_from_bytes_with_elevation`;
  `BuildOptions::ignore_gtfs_errors` and `BuildOptions::elevation`.
- CLI: `isochrone`, `itineraries`, `dump-elevation`, `sample-raster`,
  `crs-transform`; `dump-edges` accepts a directory; `PEABIRU_ELEVATION`,
  `PEABIRU_IGNORE_GTFS_ERRORS`, `PEABIRU_SCENARIO`.
- Guide: pages and examples on itineraries, fares, elevation, scenarios and
  inspecting a network.

### Python

- New functions: `isochrone()`, `detailed_itineraries()`,
  `setup_fare_structure()`, `read_fare_structure()`, `write_fare_structure()`,
  `pareto_frontier()`, `find_snap()`, `street_network_bbox()`,
  `street_network_to_sf()`, `transit_network_to_sf()`,
  `check_transit_availability()`, `get_gtfs_errors()`.
- `output_dir` in the matrix and accessibility functions, writing r5r's CSV
  files.
- `n_threads` defaults to `math.inf`, r5r's `Inf` (`None` still means the
  same).
- The text columns of fare-structure tables have `object` dtype, as in every
  other result.
- A test compares every function's arguments with r5r 2.5.0's.
- Docs: guides on itineraries, fares, elevation, scenarios, network
  inspection and GTFS errors; every function's docstring has a doctested
  example and a "Compatibility with r5r" note; "Coming from r5r / r5py"
  lists every deliberate difference.

### JavaScript

- New functions: `detailedItineraries()`, `setupFareStructure()`,
  `readFareStructure()`, `writeFareStructure()`, `paretoFrontier()`,
  `findSnap()`, `streetNetworkBbox()`, `streetNetworkToSf()`,
  `transitNetworkToSf()`, `checkTransitAvailability()`, `getGtfsErrors()`;
  `tif` and `elevation` in `buildNetwork()`, `network.r5rWarnings()`.
- Fixed: `paretoFrontier()` defaults to `mode: ["WALK", "TRANSIT"]`, as r5r;
  `getGtfsErrors()` copes with very long lists of errors.
- Guide: pages on itineraries, elevation, scenarios, inspecting a network
  and GTFS errors, and on the differences from r5r; a complete API reference
  whose examples are run and checked.

## 0.0.1

First release: networks from OpenStreetMap and GTFS, travel-time matrices,
accessibility, expanded and arrival matrices, and isochrones, on crates.io,
PyPI and npm.
