# Articles

### Using peabiru

- [Get started with peabiru](peabiru.md):

- [Travel-time matrices](travel-time-matrices.md):

- [Accessibility](accessibility.md):

- [Expanded and arrival matrices](expanded-and-arrival.md):

- [Detailed itineraries](itineraries.md):

  The legs of each trip, with their times, distances, routes and
  geometries: the fastest trip, the alternatives, and the street edges a
  trip uses.

- [Fares and Pareto frontiers](fares.md):

  Fare structures, travel times within a budget, the trade-off between
  time and fare, and alternative itineraries with their fares.

- [Trip options](trip-options.md):

  peabiru’s own trip planner: every trip worth considering between two
  points, on time, fare, transfers and walking, with the legs of each.

- [Facility location](facility-location.md):

  Where to open new facilities (maximal covering, set covering,
  p-median, p-center) and supply against demand around each place
  (two-step floating catchment area), on the travel times of the
  network.

- [Elevation](elevation.md):

  Slope costs for walking and cycling from an elevation raster: the two
  cost functions, rasters in projected coordinate systems, voids, and
  results that do not depend on the machine.

- [Scenarios: car speeds and traffic stress](scenarios.md):

  Change the car speed of streets, by way or by area, and the level of
  traffic stress cyclists meet, for one routing call.

- [Inspecting a network](network-inspection.md):

  Where points snap to the streets, the street and transit networks as
  sf, the dates with service, and the errors in the GTFS feeds.

- [Compared with r5r](coming-from-r5r.md):

  What peabiru shares with r5r 2.5.0, how the results of the two are
  checked to be the same, every difference between them, and how to move
  a script from one to the other.

### Examples

- [peabiru with sf and the tidyverse](sf-tidyverse.md):

  From points to maps with the tools you already use: an sf grid,
  routing with the native pipe, dplyr on the tibbles peabiru returns,
  joins back to the geometry and ggplot2’s geom_sf().

- [Example: isochrones by mode](isochrones.md):

  Areas reached from one point in Porto Alegre within 10, 20 and 30
  minutes, walking, cycling, driving and by public transport.

- [Example: mapping access to jobs and schools](accessibility-map.md):

  Travel times from one point to a grid, and the jobs and schools
  reachable within 30 minutes from every cell of Porto Alegre, walking
  and by public transport.

- [Example: how much a trip by public transport varies](trips.md):

  Percentiles of the travel time on a frequency-based network, and one
  trip split into walking, waiting and riding, departure by departure.

- [Example: the Recife metropolitan region by public
  transport](recife-metropolitan.md):

  The 14 municipalities of the Recife metropolitan region as one
  network, with the buses, the metro and the VLT of the GTFS feed of
  Grande Recife: isochrones by mode, access to schools and health units
  on a grid of the Censo 2022, what the rail lines add, how much a trip
  varies with the minute one leaves, and the options of one trip.

- [Example: access to schools and health care in Recife and
  Caruaru](recife-caruaru.md):

  From open data to accessibility for two cities of Pernambuco: an
  OpenStreetMap extract cut with osmium, the population of the Censo
  2022 on a hexagonal grid, public schools from INEP and health units
  from CNES, and the walking, cycling and driving times to the nearest
  of each.

- [Example: terrain, in Pernambuco and in Salvador](terrain.md):

  What an elevation raster changes in walking and cycling times: the
  Recife metropolitan region as one network, six municipalities of
  Pernambuco from the coast to the Sertão, and Salvador, in Bahia, as
  the steepest case, with the Copernicus elevation model checked against
  the ground first: at 30 m it reads the roofs, and a flat district
  shows how much that costs.

- [Example: who reaches the jobs in São Paulo, Rio de Janeiro and Belo
  Horizonte](capitals.md):

  Access to jobs, schools and health facilities by public transport in
  three Brazilian capitals with an official, open GTFS feed, by income,
  on the grid of Ipea’s Access to Opportunities Project; and, in Porto
  Alegre, what a fare budget leaves of that access.

- [Example: before and after in Porto
  Alegre](scenarios-porto-alegre.md):

  What-if questions answered without rebuilding the network: a new cycle
  path, congestion and a closed avenue, one more health facility, and a
  change in the fare rules, each with its map and its timing.

- [Example: jobs by public transport in Helsinki](helsinki.md):

  A city outside Brazil, from open data: the streets of the Helsinki
  capital region from OpenStreetMap, HSL’s timetables, and HSY’s 250 m
  grids of residents and jobs; access to jobs at the morning peak,
  isochrones from the Central Station, and the options for one trip.

### Under the hood

- [How it works](how-it-works.md):
- [Benchmarks](benchmarks.md):
