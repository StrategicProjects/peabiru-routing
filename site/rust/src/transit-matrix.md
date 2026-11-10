# Public transport matrices

`transit_travel_time_matrix` is the engine of
`travel_time_matrix(mode = c("WALK", "TRANSIT"))` in R and Python: walk to a stop, ride up to
`max_rides` vehicles with walking transfers, walk to the destination, or walk
all the way, for every minute of the departure window; then percentiles.

```rust,ignore
pub fn transit_travel_time_matrix(
    net: &StreetNetwork,
    layer: &TransitLayer,
    data: &TransitData,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
) -> Result<TransitMatrix, RouteError>
```

The result holds, for every origin and destination, one time per percentile,
in whole minutes (rounded down), with the percentiles in ascending order
(`percentiles()` lists them). A pair has times when its lowest percentile is
within `max_trip_duration`; higher percentiles beyond it are `None`.
`get(origin, destination)` gives a pair's times, one per percentile, and
`into_vec()` the whole buffer without copying (origin, then destination,
then percentile).

`TransitParams::default()` holds the defaults of the R and Python packages;
change the fields you need on it (`let mut p = TransitParams::default();
p.date = 20190513;`):

| Field | Default | Meaning |
|---|---|---|
| `date` | — | service date as `yyyymmdd` (required: the default, 0, is rejected) |
| `departure_time` | 0 | seconds after midnight |
| `time_window` | 10 | departure minutes, starting at `departure_time` |
| `percentiles` | `[50]` | 1 to 5 values in 1..=99 |
| `max_walk_time` | 120 | minutes, for each walk |
| `max_trip_duration` | 120 | minutes |
| `walk_speed_kmh` | 3.6 | |
| `max_rides` | 3 | vehicles per trip |
| `draws_per_minute` | 5 | Monte Carlo draws, frequency-based trips only |
| `seed` | `None` | `Some(n)` makes the draws reproducible |
| `n_threads` | 0 | 0 = all cores |
| `cancel` | a new token | see [Stopping a call](street-matrix.md#stopping-a-call) |

An unlimited walk (`max_walk_time = Inf` in R) corresponds to
`max_walk_time = max_trip_duration` here.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/transit_matrix.rs}}
```
