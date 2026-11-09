# Public transport matrices

`transit::matrix::travel_time_matrix` is the engine of
`travel_time_matrix(mode = c("WALK", "TRANSIT"))` in R and Python: walk to a stop, ride up to
`max_rides` vehicles with walking transfers, walk to the destination, or walk
all the way, for every minute of the departure window; then percentiles.

```rust,ignore
pub fn travel_time_matrix(
    net: &StreetNetwork,
    layer: &TransitLayer,
    data: &TransitData,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
) -> Result<Vec<Row>, String>
```

The result is indexed `[origin][destination][percentile]`, in whole minutes
(rounded down), with the percentiles in ascending order. A pair
has times when its lowest percentile is within `max_trip_duration`; higher
percentiles beyond it are `None`.

`TransitParams` (with the defaults of the R and Python packages):

| Field | Default | Meaning |
|---|---|---|
| `date` | — | service date as `yyyymmdd` (required) |
| `departure_time` | — | seconds after midnight (required) |
| `time_window` | 10 | departure minutes, starting at `departure_time` |
| `percentiles` | `[50]` | 1 to 5 values in 1..=99 |
| `max_walk_time` | 120 | minutes, for each walk |
| `max_trip_duration` | 120 | minutes |
| `walk_speed_kmh` | 3.6 | |
| `max_rides` | 3 | vehicles per trip |
| `draws_per_minute` | 5 | Monte Carlo draws, frequency-based trips only |
| `seed` | `None` | `Some(n)` makes the draws reproducible |
| `n_threads` | 0 | 0 = all cores |

An unlimited walk (`max_walk_time = Inf` in R) corresponds to
`max_walk_time = max_trip_duration` here.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/transit_matrix.rs}}
```
