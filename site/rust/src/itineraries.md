# Detailed itineraries

`detailed_itineraries` routes each origin to the destination of the same
index and describes the trips leg by leg, with their lines, as
`detailed_itineraries()` does in R, Python and JavaScript:

```rust,ignore
pub fn detailed_itineraries(
    network: &Network,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &ItineraryParams,
) -> Result<Vec<Vec<Itinerary>>, RouteError>
```

Origins and destinations pair up (`origins[i]` with `destinations[i]`);
for every origin to every destination, pair the points beforehand (the
packages do it with `all_to_all = TRUE`). The result has one entry per pair,
empty when no trip is found.

- Without public transport (`ItineraryParams::transit_modes` empty, the
  default), the itinerary is the street path by `mode` (walk, bicycle or
  car); the network needs no public transport.
- With public transport, the network must have it (GTFS feeds), `date`
  and `departure_time` are required, and with `shortest_path = true` (the
  default) the result is the faster of the direct street trip and the
  fastest public transport trip over the departure window. With
  `shortest_path = false` the trips come from the multi-criteria router
  (McRAPTOR), one per sequence of routes, up to `suboptimal_minutes` slower
  than the best one; with a `fare_structure` they also carry their fares
  (see [Fares](fares.md)).
- `egress_mode`, the trip limits (`max_walk_time`, `max_bike_time`,
  `max_car_time`, `max_trip_duration`), speeds, `max_rides`, `max_lts` and
  a [scenario](scenarios.md) are set as for the matrices.
- Networks with frequency-based trips (`frequencies.txt`) are refused with
  public transport, as r5r refuses them; convert the feed to timetables
  first (in R, `gtfstools::frequencies_to_stop_times()`).

An `Itinerary` has its `option` number, departure (`departure_time` as
`HH:MM:SS`), `total_duration` (minutes, one decimal), `total_distance`
(metres), `total_fare` with a fare structure, and its `legs`. Each
`ItineraryLeg` has its `mode` (`WALK`, `BICYCLE`, `CAR`, or the transit
mode of a ride, such as `BUS`), `segment_duration`, `wait`, `distance`, GTFS
`route`, `cumulative_fare`, and its line in `geometry` (`[lon, lat]`
points; `wkt()` gives the WKT text). With `osm_link_ids = true`, street legs
also list their OpenStreetMap way ids and edge indices, and rides their
boarding and alighting stops.

`write_itineraries_csv` writes them as the packages do with `output_dir`:
one CSV file per pair with itineraries.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/itineraries.rs}}
```

On the São Paulo sample:

```text
pair 0 WALK    27.2 min  1914 m, 79 points in the line
pair 1 WALK    20.8 min  1310 m, 45 points in the line
pair 0 BICYCLE 18.4 min  1812 m, 75 points in the line
pair 1 BICYCLE 13.3 min  1646 m, 64 points in the line
pair 0 CAR      7.1 min  2461 m, 83 points in the line
pair 1 CAR      4.5 min  1588 m, 52 points in the line
with public transport: None of the GTFS feeds used to create the transit network can contain a 'frequencies' table. Try using gtfstools::frequencies_to_stop_times() to create a suitable feed.
```

Its feed is frequency-based, so both public transport calls are refused
(the example prints the first refusal and skips the alternatives). On r5r's
Porto Alegre sample, whose feeds are timetables (a directory with
`poa_osm.pbf`, `poa_eptc.zip`, `poa_trensurb.zip` and
`poa_points_of_interest.csv`, copied from r5r's `extdata/poa`), the public
transport part prints:

```text
pair 0 option 1 at 14:06:00: 16.6 min
  WALK     3.1 min (wait 0.0)   214 m
  RAIL     6.6 min (wait 1.9)  4796 m LINHA1
  WALK     5.0 min (wait 0.0)   298 m
pair 1 option 1 at 14:04:00: 36.6 min
  WALK     3.6 min (wait 0.0)   206 m
  BUS      4.1 min (wait 1.4)  1656 m 255
  WALK     0.4 min (wait 0.0)    21 m
  BUS     14.8 min (wait 1.6)  5684 m 188
  WALK    10.8 min (wait 0.0)   645 m
pair 0: 5 alternatives
  option 1 at 14:00:00: 80.4 min, routes []
  option 2 at 14:05:50: 16.7 min, routes ["LINHA1"]
  option 3 at 14:09:26: 28.0 min, routes ["731"]
pair 1: 183 alternatives
  option 1 at 14:00:00: 92.4 min, routes []
  option 2 at 14:05:51: 49.0 min, routes ["165"]
  option 3 at 14:05:51: 49.7 min, routes ["188"]
```

The alternatives start with the direct walk (no routes); their departure
times are the ones McRAPTOR samples in the window.

## Trip options

To see every trip worth considering between two points, faster or cheaper, with fewer transfers or less walking, and not only the fastest one, see `trip_options`, peabiru's own multi-criteria planner: [Trip options](trip-options.md).
