# Accessibility

`transit::accessibility::transit_accessibility` (walk + public transport) and
`street_accessibility` (one street mode) are r5r's `accessibility()`: for
each origin, the sum over destinations of the opportunities at the
destination times a decay weight of the travel time.

<img src="../assets/diagrams/accessibility.svg" alt="Accessibility is the sum over destinations of opportunities times a decay weight of the travel time; five decay functions drawn for a 30 minute cutoff." width="980" height="420">

- `Destinations` pairs the destination points with one column of counts per
  kind of opportunity (`opportunities[k][j]`). Counts that are zero,
  negative or NaN add nothing.
- `AccessibilityParams` holds the cutoffs (minutes, up to 12) and the
  `DecayFunction`: `Step`, `Exponential`, `FixedExponential { decay_constant }`,
  `Linear { width_minutes }` or `Logistic { standard_deviation_minutes }`.
- The result, `Accessibility`, has one value per origin, opportunity column,
  percentile and cutoff; `get(origin, opportunity, percentile, cutoff)` reads
  one.

The travel time given to the decay function is the percentile in seconds,
not rounded to minutes, and the exponential is a port of the one R5 calls
(`FastMath.exp` from Apache Commons Math 3), so the values are bit-identical
to r5r's.

r5r adjusts the trip limits before calling R5 (with `Step`,
`max_trip_duration` becomes the largest cutoff; `max_walk_time = Inf` becomes
`max_trip_duration`). `r5r_trip_limits` does the same, so that the crate
gives r5r's numbers for r5r's arguments.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/accessibility.rs}}
```
