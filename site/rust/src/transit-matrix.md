# Public transport matrices

`transit_travel_time_matrix` is the engine of
`travel_time_matrix(mode = c("WALK", "TRANSIT"))` in R, Python and
JavaScript: walk (or cycle, or drive) to a stop, ride up to `max_rides`
vehicles with walking transfers, walk (or cycle, or drive) to the
destination, or make the whole trip by the street mode, for every minute of
the departure window; then percentiles.

```rust,ignore
pub fn transit_travel_time_matrix(
    network: &Network,
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

`TransitParams::default()` holds the defaults of the R, Python and JavaScript packages;
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
| `walk_speed` | 3.6 | |
| `max_rides` | 3 | vehicles per trip |
| `draws_per_minute` | 5 | Monte Carlo draws, frequency-based trips only |
| `seed` | `None` | `Some(n)` makes the draws reproducible |
| `mode` | `Mode::Walk` | street mode to public transport, and of the trip without it |
| `egress_mode` | `Mode::Walk` | street mode from public transport to the destination |
| `transit_modes` | all | the public transport modes to ride (`TransitMode::Bus`, `Rail`, `Subway`, ...) |
| `bike_speed` | 12 | for bicycle access and egress |
| `max_lts` | 2 | highest level of traffic stress for bicycle access |
| `max_bike_time`, `max_car_time` | 120 | minutes, for bicycle and car access and egress |
| `fare_structure`, `max_fare` | `None`, infinity | see [Fares](fares.md) |
| `scenario` | no change | see [Scenarios](scenarios.md) |
| `n_threads` | 0 | 0 = all cores |
| `cancel` | a new token | see [Stopping a call](street-matrix.md#stopping-a-call) |

An unlimited walk (`max_walk_time = Inf` in R) corresponds to
`max_walk_time = max_trip_duration` here, and likewise for `max_bike_time`
and `max_car_time`.

By bicycle or car, the access search goes on on foot from wherever the
vehicle got, within the same time limit, so that stops in
pedestrian-only areas are reached too. A bicycle or car egress links every
stop again by that mode.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/transit_matrix.rs}}
```
