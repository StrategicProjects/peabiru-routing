# API reference

The rustdoc reference of `peabiru-core` is published with this guide:
**[open the API reference](api/peabiru_core/index.html)**.

Everything public is at the root of the crate (`use peabiru_core::{...}`).
Main entry points:

| Item | What it does |
|---|---|
| [`Network`](api/peabiru_core/struct.Network.html) | the network every function takes: streets and, with GTFS, public transport |
| [`build_from_dir`](api/peabiru_core/fn.build_from_dir.html), [`build_from_bytes`](api/peabiru_core/fn.build_from_bytes.html) | a network from a directory, or from the files' bytes |
| [`build_network`](api/peabiru_core/fn.build_network.html) | a street-only network from a `.pbf` |
| [`street_travel_time_matrix`](api/peabiru_core/fn.street_travel_time_matrix.html) | street matrix ([`StreetMatrix`](api/peabiru_core/struct.StreetMatrix.html)) |
| [`transit_travel_time_matrix`](api/peabiru_core/fn.transit_travel_time_matrix.html) | walk + transit matrix ([`TransitMatrix`](api/peabiru_core/struct.TransitMatrix.html)) |
| [`transit_accessibility`](api/peabiru_core/fn.transit_accessibility.html), [`street_accessibility`](api/peabiru_core/fn.street_accessibility.html) | accessibility ([`Accessibility`](api/peabiru_core/struct.Accessibility.html)) |
| [`expanded_travel_time_matrix`](api/peabiru_core/fn.expanded_travel_time_matrix.html), [`arrival_travel_time_matrix`](api/peabiru_core/fn.arrival_travel_time_matrix.html) | expanded and arrival matrices ([`ExpandedMatrix`](api/peabiru_core/struct.ExpandedMatrix.html)) |
| [`street_travel_time_surface`](api/peabiru_core/fn.street_travel_time_surface.html), [`transit_travel_time_surface`](api/peabiru_core/fn.transit_travel_time_surface.html), [`isochrone_polygons`](api/peabiru_core/fn.isochrone_polygons.html), [`isochrone_lines`](api/peabiru_core/fn.isochrone_lines.html) | isochrones |
| [`detailed_itineraries`](api/peabiru_core/fn.detailed_itineraries.html) | trips leg by leg ([`Itinerary`](api/peabiru_core/struct.Itinerary.html)) |
| [`setup_fare_structure`](api/peabiru_core/fn.setup_fare_structure.html), [`read_fare_structure`](api/peabiru_core/fn.read_fare_structure.html), [`write_fare_structure`](api/peabiru_core/fn.write_fare_structure.html) | fare structures ([`FareStructure`](api/peabiru_core/struct.FareStructure.html)) |
| [`pareto_frontier`](api/peabiru_core/fn.pareto_frontier.html) | travel time against fare ([`ParetoFrontier`](api/peabiru_core/struct.ParetoFrontier.html)) |
| [`Elevation`](api/peabiru_core/enum.Elevation.html), [`build_from_bytes_with_elevation`](api/peabiru_core/fn.build_from_bytes_with_elevation.html) | slope costs from a raster |
| [`Scenario`](api/peabiru_core/struct.Scenario.html) | car speeds and traffic stress changed per call |
| [`find_snap`](api/peabiru_core/fn.find_snap.html), [`street_vertices`](api/peabiru_core/fn.street_vertices.html), [`street_edges`](api/peabiru_core/fn.street_edges.html), [`transit_stops`](api/peabiru_core/fn.transit_stops.html), [`transit_routes`](api/peabiru_core/fn.transit_routes.html), [`transit_service_dates`](api/peabiru_core/fn.transit_service_dates.html) | network inspection |
| [`gtfs_errors`](api/peabiru_core/fn.gtfs_errors.html) | the GTFS errors of a feed |
| [`CsvOutput`](api/peabiru_core/struct.CsvOutput.html), [`write_itineraries_csv`](api/peabiru_core/fn.write_itineraries_csv.html) | CSV files, one per origin (or per pair for itineraries) |

The parameter structs ([`BuildOptions`](api/peabiru_core/struct.BuildOptions.html),
[`RouteParams`](api/peabiru_core/struct.RouteParams.html),
[`TransitParams`](api/peabiru_core/struct.TransitParams.html),
[`AccessibilityParams`](api/peabiru_core/struct.AccessibilityParams.html),
[`ItineraryParams`](api/peabiru_core/struct.ItineraryParams.html))
are filled in by changing the fields of their `default()`. Routing errors are
a [`RouteError`](api/peabiru_core/enum.RouteError.html), network-building
errors a [`NetworkError`](api/peabiru_core/struct.NetworkError.html).

Locally, `cargo doc --no-deps -p peabiru-core --open` builds the same pages.
