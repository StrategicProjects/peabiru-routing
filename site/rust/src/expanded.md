# Expanded and arrival matrices

`transit::expanded::expanded_travel_time_matrix` and
`arrival_travel_time_matrix` keep the trips themselves instead of
percentiles, as r5r's functions of the same names.

<img src="../assets/diagrams/expanded-arrival.svg" alt="Expanded matrix: one row per departure minute and draw, with the breakdown. Arrival matrix: the latest departure that arrives in time." width="980" height="520">

Both return, per origin, a `Vec<ExpandedRow>`:

- `to`: destination index;
- `departure`: seconds after midnight (beyond 24 h when an arrival search
  crosses midnight);
- `draw`: the rank of the trip among the draws of its minute, sorted by total
  time (1 with timetable-based feeds);
- `trip`: `None` when there is no trip within `max_trip_duration`, else a
  `Breakdown` with access, waiting, riding, transfer and egress minutes (one
  decimal), the route ids joined by `|` (`[WALK]` for a walk), the number of
  rides and the total.

For an arrival search, r5r searches the `max_trip_duration` minutes before
the arrival time; `arrival_departure(date, arrival_time, max_trip_duration)`
gives the date and departure to put in `TransitParams` (the window can start
on the previous day), and `time_window` must equal `max_trip_duration`. At
most 5,000 destinations are accepted, as in R5.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/expanded.rs}}
```
