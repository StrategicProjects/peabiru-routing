# Inspecting a network

These functions report what the network holds, as r5r's network functions
do (`find_snap()`, `street_network_bbox()`, `street_network_to_sf()`,
`transit_network_to_sf()`, `check_transit_availability()`):

| Function | What it gives |
|---|---|
| `find_snap(network, points, radius_m, mode)` | where each point meets the streets open to `mode`: the snapped point and its distance, or `None` |
| `network.street().bbox()` | the box of the street network, `[xmin, ymin, xmax, ymax]` |
| `street_vertices(network)` | the street vertices, with their position and park-and-ride flag |
| `street_edges(network)` | the directed street edges: OSM way id, street class, length, permissions, car speed, level of traffic stress, line |
| `transit_stops(network)` | the stops, with their position on the streets and whether they are linked to them |
| `transit_routes(network)` | one row per trip pattern: agency, route ids and names, mode, line |
| `transit_service_dates(network)` | the dates on which some service runs |
| `network.transit()?.n_services_active_on(date)` | how many services run on a date |

The rows come in r5r's order, which for the street vertices is that of a
Java `HashSet`, reproduced.

## GTFS errors

`gtfs_errors(name, zip_bytes)` lists the errors conveyal's gtfs-lib records
while loading a feed, as r5r writes them to `gtfs_errors.csv`: per error its
table (`file()`), `line()`, `error_type()`, `field()`, `id()` and
`priority()` (`High`, `Medium`, `Low` or `Unknown`). Every check of
gtfs-lib's table loaders and post-load validation is made; the rows are
listed by table, in gtfs-lib's loading order, then by line (r5r lists them
in the order of a Java `HashSet`, which changes from run to run).
`gtfs_files_in_dir` gives the feeds of a directory in the order they are
loaded.

## Feeds with errors of priority HIGH

As r5r's `build_network()` does, `build_from_dir` and `build_from_bytes`
fail when a feed has an error of priority `HIGH`, or is one gtfs-lib cannot
load at all. `NetworkError::high_priority_gtfs_errors()` then gives each
such feed with its number of `HIGH` errors. With
`BuildOptions::ignore_gtfs_errors = true` the feeds are not checked: the
network is built from what can be read, leaving out trips that refer to
stops or routes that do not exist, and `Network::build_warnings()` says
which feeds r5r would refuse.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/inspect.rs}}
```

On the São Paulo sample:

```text
point 89a8100c00fffff snaps to (-23.556435, -46.628960), 13.7 m away
point 89a8100c007ffff snaps to (-23.557701, -46.631974), 9.5 m away
point 89a8100c077ffff snaps to (-23.553252, -46.628872), 0.8 m away
bbox: lon -46.6477 to -46.6113, lat -23.5653 to -23.5371
streets: 1937 vertices, 5222 directed edges (1680 open to cars), 299.7 km
transit: 654 stops (148 linked to the streets), 36 patterns {"BUS": 10, "RAIL": 14, "SUBWAY": 12}
service from 20080101 to 20200501 (4505 dates); on 2019-05-13, 3 of 6 services run
spo_gtfs.zip: {("MEDIUM", "DuplicateKeyError"): 7}
```

With the same feed without its `agency.txt`, the build is refused, and the
example builds it again with `ignore_gtfs_errors` (the directory's path is
shortened to `<dir>` here):

```text
refused: high priority GTFS errors found in <dir>/spo_gtfs.zip (20 errors); network build failed (as in r5r)
built anyway: GTFS feed 'spo_gtfs.zip': it has no agency.txt. r5r refuses to build a network from this feed.
...
spo_gtfs.zip: {("HIGH", "NoAgencyInFeedError"): 19, ("HIGH", "ReferentialIntegrityError"): 1, ("MEDIUM", "DuplicateKeyError"): 6, ("MEDIUM", "MissingTableError"): 1}
```
