# Changelog

Changes of the Rust crate `peabiru-core`, the command-line tool, and the
Python and JavaScript packages. The R package has its own `r/NEWS.md`.

## 0.1.0

The R, Python and JavaScript packages have the same routing and network
functions. peabiru is described in its own terms: each package has one page
comparing it with r5r, and the comparisons that check the results live in
the repository (`bench/compare`), outside the packages.

### Routing and inputs (all packages)

- `trip_options()` (Python, R; `tripOptions()` in JavaScript; the core's
  `trip_options` and `write_trip_options_csv`; `peabiru options` on the
  command line): a multi-criteria trip planner. The trips not dominated on total time, fare, transfers and
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
  +/-3276.7 m wrap around as in R5, with a warning; where the pixel R5 reads
  depends on the machine, Java's `StrictMath` decides, with a warning.
- Network inspection: snapping, the bounding box, the street and transit
  networks as geometries, the dates with service, and the GTFS errors.
- GTFS is read as gtfs-lib reads it (including GTFS-Flex
  `locations.geojson`), and feeds with errors of priority HIGH are refused.
- Isochrone rings follow libstdc++'s order on Linux and Windows, and
  libc++'s on macOS. Park-and-ride areas link to the street as in R5; edge lengths use
  Apache Commons Math's `FastMath`.
- Argument errors name the argument, what was expected and what was given,
  in the same words in the three packages.
- Itineraries: rides along GTFS shapes whose stop times and points carry
  `shape_dist_traveled` are cut by those distances, as R5 cuts them (they
  were refused). With car egress through a turn restriction with two or
  more via ways, every pair gets the result of a freshly built network
  for that pair alone, with one warning per call naming the pairs whose
  result in R5 depends on the searches that ran before (they were
  refused).
- `write_fare_structure()` deflates the archive.
- "is not supported by peabiru yet" is now "is not supported by peabiru".

### Rust crate and CLI

- **Breaking:** `build_from_dir`, `build_from_bytes`,
  `build_from_bytes_with_elevation` and `build_network` return a `Network`
  (the street network and, with GTFS feeds, the transit layer linked to
  it), and every routing and inspection function takes `&Network` in place
  of the street network, transit layer and `TransitData` it took apart
  (`TransitData` is no longer public). `Network::street()`,
  `Network::transit()` (`None` without feeds) and
  `Network::build_warnings()` (the feeds built from with
  `ignore_gtfs_errors`, then the elevation raster's warnings) describe it.
  Public transport functions return `RouteError::Invalid` on a network
  without it. `pareto_frontier` routes by the access mode alone when
  `TransitParams::transit_modes` is empty; `setup_fare_structure`,
  `FareStructure::validate_for` and `WebMercatorGrid::for_network` take the
  network too.
- **Breaking:** `RouteError` no longer converts from `String` or `&str`;
  build `RouteError::Invalid` explicitly.
- **Breaking:** one name per concept in the parameter structs, the R and
  Python packages' argument name, with the unit in the field's
  documentation: `RouteParams::max_trip_duration_min` is
  `max_trip_duration` (minutes), as in `TransitParams`, `ItineraryParams`
  and `TripOptionsParams`; `walk_speed_kmh` and `bike_speed_kmh` are
  `walk_speed` and `bike_speed` (km/h) in all four;
  `TripOptionsParams::walk_resolution_s` is `walk_resolution` (seconds);
  `TransitParams::access_mode` is `mode`, as in `ItineraryParams` and
  `TripOptionsParams`. The engine's error messages use the new names.
- **Breaking:** `Criterion` is `#[non_exhaustive]` (the criteria of
  `trip_options` may grow), and `Criterion::ALL` and `TransitMode::ALL`
  are `&'static [_]` slices instead of arrays of a fixed length, so that a
  new criterion or mode does not change their type.
- `StreetVertices`, `StreetEdges`, `TransitStops` and `TransitRoutes`
  implement `PartialEq`, as the other result types do (not `Eq`: each holds
  coordinates as floats).
- `cargo test` passes in the packaged `peabiru-core`: the 38 tests that
  read the repository's sample data (`r/inst/extdata/spo`), which the
  crate does not carry, say on stderr that they are skipped and return.
- `output_dir` (R, Python, the CLI's `--output-dir`; the core's
  `CsvOutput::new`, now fallible, `write_itineraries_csv` and
  `write_trip_options_csv`): an id that cannot name a file in the
  directory (a path separator `/` or `\`, NUL; on Windows also
  `< > : " | ? *` or a control character) is an error naming it, before
  anything is routed or written. `trip_options()` uses the same rule.
- **Breaking:** renamed in `peabiru-core`: `r5r_trip_limits` is
  `accessibility_trip_limits`; `Elevation::r5r_name` and `from_r5r_name`
  are `name` and `from_name`; `FeedReport::r5r_refusals`, `r5r_warning` and
  `r5r_warnings_per_case` are `high_priority_cases`, `warning` and
  `warnings_per_case`; `TransitLayer::r5r_warnings` is `feed_warnings`.
- A snapshot of the public API (`tools/public-api/peabiru-core.txt`),
  checked in CI, so that changes to it are deliberate.
- **The command-line tool, redesigned** to be published as the crate
  `peabiru-cli` (binary `peabiru`): one command per function of the
  packages (`info`/`build`, `matrix`, `expanded`, `arrival`,
  `accessibility`, `isochrone`, `itineraries`, `pareto`, `options`,
  `snap`, `network`, `fares setup|read|write`, `completions`), named
  options with the packages' argument names in kebab-case, defaults and
  checks (`--departure 2019-05-13T14:00`, `--mode WALK,BUS`,
  `--max-walk-time Inf`, `--fare-structure fares.zip`, `--elevation`,
  `--new-carspeeds`, `--walk-resolution`, ...), networks as directories or
  a `.pbf` with `--gtfs` and `--tif`, CSV and GeoJSON output, `--output-dir`
  (one file per origin), help with examples for every command, exit status 0/1/2 and
  130 on Ctrl-C. What the environment variables switched
  (`PEABIRU_ELEVATION`, `PEABIRU_IGNORE_GTFS_ERRORS`, `PEABIRU_SCENARIO`,
  `PEABIRU_STREET_MATRIX`) is now an option (`--elevation`,
  `--ignore-gtfs-errors`, `--new-carspeeds`/`--new-lts`,
  `--street-engine`). Every command gives the R package's values
  (`bench/compare/cli_parity.R`). The old positional spellings
  (`ttm`, `transit`, `dump-edges`, ...) still run for the repository's
  scripts, undocumented, also as `peabiru dev <command>`.
- API: `find_snap`, `StreetNetwork::bbox`, `street_vertices`,
  `transit_stops`, `transit_routes`, `transit_service_dates`, `gtfs_errors`,
  `detailed_itineraries`, `write_itineraries_csv`, `Scenario` and its parts,
  `FareStructure` with `setup_`/`read_`/`write_fare_structure`,
  `pareto_frontier`, `Elevation` and `build_from_bytes_with_elevation`;
  `BuildOptions::ignore_gtfs_errors` and `BuildOptions::elevation`;
  `detailed_itineraries_noted` and `ItineraryNotes` (the pairs to warn
  about, and the pairs planned again with McRAPTOR because FastRaptor's
  path could not be read back). The CLI's `itineraries` reports
  those pairs on stderr, and `PEABIRU_FAIL_FAST_RAPTOR=1` fails every
  FastRaptor trip, to test the fallback.
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
- `output_dir` in the matrix and accessibility functions: one CSV file per
  origin.
- `n_threads` defaults to `math.inf`, all cores (`None` still means the
  same).
- The text columns of fare-structure tables have `object` dtype, as in every
  other result.
- Argument errors have the R package's messages in Python's terms
  (`peabiru._checkmate`): 1,367 cases shared with the R and JavaScript
  packages are checked (`bench/compare/argument_errors.json`). A list of one value counts as
  that value, as a vector of one in R. As in R: a missing destination id is
  an error; `isochrone()` with no origins is an error; `write_fare_structure()`
  checks the structure; the engine's refusals are `UnsupportedError`s.
- Ctrl-C tests bound the CPU time of the interrupted call, not the wall
  time, so a loaded machine no longer fails them.
- Docs: guides on itineraries, fares, elevation, scenarios, network
  inspection and GTFS errors; every function's docstring has a doctested
  example; one page compares the package with r5r and r5py.

### JavaScript

- **Breaking:** `network.buildWarnings()` gives the warnings of the build
  (elevation voids, samples on a pixel boundary, ...); it replaces
  `network.r5rWarnings()`, which is removed.
- New functions: `detailedItineraries()`, `setupFareStructure()`,
  `readFareStructure()`, `writeFareStructure()`, `paretoFrontier()`,
  `findSnap()`, `streetNetworkBbox()`, `streetNetworkToSf()`,
  `transitNetworkToSf()`, `checkTransitAvailability()`, `getGtfsErrors()`;
  `tif` and `elevation` in `buildNetwork()`.
- Argument errors have the R package's messages in JavaScript's terms, with
  the options in camelCase (engine errors too), checked on the 1,367 cases
  shared with the R and Python packages; every function refuses options it
  does not have (`detailedItineraries()` took `drawsPerMinute` and `seed`
  and ignored them). As in R: a missing destination id is an error;
  `isochrone()` with no origins is an error; the engine's refusals are
  `UnsupportedError`s.
- Fixed: `paretoFrontier()` defaults to `mode: ["WALK", "TRANSIT"]`;
  `getGtfsErrors()` copes with very long lists of errors.
- Guide: pages on itineraries, elevation, scenarios, inspecting a network
  and GTFS errors, and one comparing the package with r5r; a complete API reference
  whose examples are run and checked.

## 0.0.1

First release: networks from OpenStreetMap and GTFS, travel-time matrices,
accessibility, expanded and arrival matrices, and isochrones, on crates.io,
PyPI and npm.
