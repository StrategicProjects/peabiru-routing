# Package index

## Networks

Build a network once from a directory with an OpenStreetMap `.pbf` and
GTFS feeds.

- [`build_network()`](build_network.md) : Build a transport network

## Travel times

Travel-time matrices by street or by public transport, summarised over
the departure window or trip by trip.

- [`travel_time_matrix()`](travel_time_matrix.md) : Calculate travel
  time matrix between origin destination pairs considering a departure
  time
- [`expanded_travel_time_matrix()`](expanded_travel_time_matrix.md) :
  Calculate minute-by-minute travel times between origin destination
  pairs
- [`arrival_travel_time_matrix()`](arrival_travel_time_matrix.md) :
  Calculate travel time matrix between origin destination pairs
  considering a time of arrival

## Accessibility

- [`accessibility()`](accessibility.md) : Calculate access to
  opportunities

## Fares

Fare structures (how much each ride and transfer costs), which the
routing functions take, and the trade-off between travel time and fare.

- [`setup_fare_structure()`](setup_fare_structure.md) : Setup a fare
  structure to calculate the monetary costs of trips
- [`read_fare_structure()`](read_fare_structure.md) : Read a fare
  structure object from a file
- [`write_fare_structure()`](write_fare_structure.md) : Write a fare
  structure object to disk
- [`pareto_frontier()`](pareto_frontier.md) : Calculate travel time and
  monetary cost Pareto frontier

## Isochrones and itineraries

The areas or street segments reached from each origin within each
cutoff, and the legs of each trip.

- [`isochrone()`](isochrone.md) : Estimate isochrones from a given
  location
- [`detailed_itineraries()`](detailed_itineraries.md) : Detailed
  itineraries between origin-destination pairs

## Trip options

A multi-criteria trip planner: every trip worth considering on time,
fare, transfers and walking.

- [`trip_options()`](trip_options.md) : Trip options between
  origin-destination pairs

## Facility location

Where to open new facilities (maximal covering, set covering, p-median,
p-center), and supply against demand around each place (two-step
floating catchment area).

- [`locate_facilities()`](locate_facilities.md) : Choose where to open
  new facilities
- [`floating_catchment()`](floating_catchment.md) : Supply against
  demand around each place: the two-step floating catchment area

## Inspecting a network

Where points snap, the street and transit networks as `sf`, the dates
with service, and the errors gtfs-lib finds in the feeds.

- [`find_snap()`](find_snap.md) : Find snapped locations of input points
  on street network
- [`street_network_bbox()`](street_network_bbox.md) : Extract the
  geographic bounding box of the transport network
- [`street_network_to_sf()`](street_network_to_sf.md) : Extract
  OpenStreetMap network in sf format
- [`transit_network_to_sf()`](transit_network_to_sf.md) : Extract
  transit network in sf format
- [`check_transit_availability()`](check_transit_availability.md) :
  Check transit service availability by date
- [`get_gtfs_errors()`](get_gtfs_errors.md) : Get GTFS errors
  encountered in network building

## The package

- [`peabiru`](peabiru-package.md)
  [`peabiru-package`](peabiru-package.md) : peabiru: multimodal routing
  on OpenStreetMap and GTFS networks
- [`core_version()`](core_version.md) : Version of the Rust routing core
