# Street travel-time matrices

`street::travel_time_matrix` computes walking, cycling or driving times
between every origin and every destination (in R and Python,
`travel_time_matrix(mode = "WALK" | "BICYCLE" | "CAR")`):

```rust,ignore
pub fn travel_time_matrix(
    net: &StreetNetwork,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    mode: Mode,
    params: &RouteParams,
) -> Result<Vec<Option<u32>>, ParamError>
```

The result is row-major (`origins.len() × destinations.len()`), in whole
minutes, rounded down; `None` where there is no time: a point that cannot
be linked to a street open to the mode, an unreachable destination, or a
trip longer than `max_trip_duration_min`.

`RouteParams` holds the defaults: `walk_speed_kmh = 3.6`,
`bike_speed_kmh = 12.0`, `max_lts = 2`, `max_trip_duration_min = 120`, and
`n_threads = 0` (all cores). Origins are spread over the threads; the result
does not depend on how many there are.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/street_matrix.rs}}
```

## Raw seconds

`street::router::travel_time_seconds` runs the same search but hands each
origin's times in seconds, before rounding, to a closure of yours, which is
how accessibility is computed without a full matrix in memory.
