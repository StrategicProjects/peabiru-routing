# Accessibility

`transit_accessibility` (walk + public transport) and
`street_accessibility` (one street mode) compute what `accessibility()`
returns in R, Python and JavaScript: for each origin, the sum over destinations of the opportunities at the
destination times a decay weight of the travel time.

<img src="../assets/diagrams/accessibility.svg" alt="Accessibility is the sum over destinations of opportunities times a decay weight of the travel time; five decay functions drawn for a 30 minute cutoff." width="980" height="420">

- `Destinations::new(points, opportunities)` pairs the destination points
  with one column of counts per kind of opportunity (`opportunities[k][j]`).
  Counts that are zero, negative or NaN add nothing.
- `AccessibilityParams` holds the cutoffs (minutes, up to 12) and the
  `DecayFunction`: `Step`, `Exponential`, `FixedExponential { decay_constant }`,
  `Linear { width_minutes }` or `Logistic { standard_deviation_minutes }`.
  Its default: one cutoff of 30 minutes, `Step`.
- The result, `Accessibility`, has one value per origin, opportunity column,
  percentile and cutoff; `get(origin, opportunity, percentile, cutoff)` reads
  one, and `values()` gives them all in that order (the cutoff varying
  fastest).

The travel time given to the decay function is the percentile in seconds,
not rounded to minutes, and the exponential is a port of `FastMath.exp` from
Apache Commons Math 3 rather than the platform's `exp`, so the values do not
depend on the C library.

The packages adjust the trip limits before routing: with
`Step`, `max_trip_duration` becomes the largest cutoff, and an unlimited
`max_walk_time` becomes `max_trip_duration`. `accessibility_trip_limits`
applies the same rules, so the crate gives the packages' numbers for their
arguments.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/accessibility.rs}}
```
