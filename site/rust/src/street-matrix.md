# Street travel-time matrices

`street_travel_time_matrix` computes walking, cycling or driving times
between every origin and every destination (in R, Python and JavaScript,
`travel_time_matrix(mode = "WALK" | "BICYCLE" | "CAR")`):

```rust,ignore
pub fn street_travel_time_matrix(
    net: &StreetNetwork,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    mode: Mode,
    params: &RouteParams,
) -> Result<StreetMatrix, RouteError>
```

The result holds whole minutes, rounded down, for every origin and
destination; `None` where there is no time: a point that cannot be linked to
a street open to the mode, an unreachable destination, or a trip longer than
`max_trip_duration_min`. `get(origin, destination)` reads one value,
`row(origin)` and `rows()` give the rows, and `into_vec()` hands over the
whole buffer, row by row, without copying.

`RouteParams::default()` holds the defaults: `walk_speed_kmh = 3.6`,
`bike_speed_kmh = 12.0`, `max_lts = 2`, `max_trip_duration_min = 120`,
`n_threads = 0` (all cores), no [scenario](scenarios.md), and
`engine = StreetEngine::Auto`. The engine only decides how the times are
found, never what they are: `Search` runs a search from every origin,
`Hierarchy` builds a contraction hierarchy of the network (kept in the
network for later calls) and answers from it, and `Auto` builds one when
enough origins make it pay off. Car routing on a network with turn
restrictions always searches. Change the fields you need on a default value
(`let mut p = RouteParams::default(); p.max_lts = 3;`). Origins are spread
over the threads; the result does not depend on how many there are.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/street_matrix.rs}}
```

## Stopping a call

The two routing parameter structs, `RouteParams` and `TransitParams`, carry
a `CancelToken` in their `cancel` field. Keep a clone, and call `cancel()` on
it from another thread to stop the call:

```rust,ignore
let params = RouteParams::default();
let stop = params.cancel.clone();
// from another thread: stop.cancel();
match street_travel_time_matrix(&net, &origins, &destinations, Mode::Car, &params) {
    Ok(matrix) => { /* ... */ }
    Err(RouteError::Cancelled) => { /* stopped; there is no partial result */ }
    Err(e) => { /* a parameter or an input was rejected: e.to_string() says why */ }
}
```

`RouteError` is non-exhaustive, so a `match` on it needs a catch-all arm.

The workers look at the token between origins and, in the transit functions,
at every departure minute. A cancelled token stays cancelled, so use fresh
parameters for the next call. Building the network cannot be stopped.

## Accessibility on streets

`street_accessibility` runs the same search, but keeps each origin's times
in seconds, before rounding, only as long as it takes to sum the
opportunities, so no matrix is held in memory (see
[Accessibility](accessibility.md)).
