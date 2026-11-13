# Trip options

`trip_options` is peabiru's own trip planner:
for each origin-destination pair, every trip worth considering, the ones
that no other trip beats on all the chosen criteria at once (`Criterion::Time`,
`Fare`, `Transfers`, `Walk`), at each departure minute of the window, merged
into options.

```rust,ignore
pub fn trip_options(
    network: &Network,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TripOptionsParams,
) -> Result<Vec<Vec<TripOption>>, RouteError>;
```

```rust,ignore
let mut p = peabiru_core::TripOptionsParams::default();
p.transit_modes = peabiru_core::TransitMode::ALL.to_vec();
p.date = 20190513;
p.departure_time = 14 * 3600;
p.fare_structure = Some(std::sync::Arc::new(fares)); // all four criteria by default
let options = peabiru_core::trip_options(&network, &o, &d, &p)?;
for t in &options[0] {
    println!("{} {:.1} min {:?} {} transfers: {}", t.option, t.total_time, t.fare,
             t.transfers, t.description);
}
```

A `TripOption` holds its values at its best minute (`total_time_s`, `fare`,
`transfers`, `walk_time_s`), every minute it is on the frontier with the
total time there (`departures`, `totals_s`), a description (`WALK > BUS T4 >
WALK`) and, with `itineraries`, its legs as an `Itinerary` of
`detailed_itineraries`. `write_trip_options_csv` writes one CSV file per pair.
The command line has it as `peabiru options`.

## How it works

The search (`options/search.rs`) is McRAPTOR (Delling, Pajor and Werneck,
"Round-Based Public Transit Routing") from each departure minute, with Pareto
bags of labels: arrival time, rides, seconds on foot and fare state. It runs
on the same network semantics as the other functions, so that its times
agree with theirs: the access and egress stop searches of the itinerary
planner, the same transfer tables, a minimum boarding wait of 60 s, the
same trip limit as RAPTOR. A
ride takes the vehicle that reaches the alighting stop first.

- **Fares** are not additive (discounts within an allowance, unlimited
  transfers, a cap), so two partial trips are compared on fare only when one
  is at least as cheap for every continuation (`fares/incremental.rs`): their
  fare states are equivalent (same type, discounts, full fare and route of
  the last ride, same time and discount deadline), or a later deadline is
  provably no worse, or one has paid less by more than the other could still
  save. The simpler comparison the fare router of `pareto_frontier` makes
  (paid fare and transfer allowance) is not sound for every structure; on
  the Porto Alegre sample's structure the two give the same frontiers.
- **Frequency-based trips** are boarded at their expected times (half a
  headway of waiting; the scheduled departures with `exact_times = 1`), so
  a trip is still a path and the search is exact on those values. There are
  no random draws.
- **Coarse comparison of what is returned:** of each minute's exact
  frontier, the trips beaten once total time is rounded to a tenth of a
  minute and walking to `TripOptionsParams::walk_resolution` (default 60
  s) are left out (ties go to the better trip in exact terms). Rounding is
  monotone, so filtering the exact frontier gives the coarse Pareto set of
  all trips; the values reported stay exact.
- **Lower bounds** per stop of the time, rides and walking still to come
  (a backward Dijkstra from the egress stops) prune labels that cannot reach
  the destination within the limits or that a trip already found dominates.

The tests check the search against an exhaustive enumeration of every trip
of random small networks (random routes, trips that overtake, frequencies,
pickup and drop-off rules, transfers, fare structures with penalties, caps
and short allowances), departure minute by departure minute.
