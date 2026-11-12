# Trip options

`tripOptions()` is peabiru's own trip planner, with no counterpart in r5r:
for each origin and destination it gives every trip worth considering, the
ones that no other trip beats on all the chosen criteria at once. Where
`detailedItineraries()` answers "what is the fastest way there?",
`tripOptions()` answers "what are my choices?": faster but dearer, cheaper
but slower, fewer transfers, less walking. It is the same function as
`trip_options()` in R and Python, with the same results.

## Not dominated

A trip *dominates* another when it is no worse on every chosen criterion and
better on at least one. The criteria are

- `"TIME"`: door-to-door minutes from the departure minute, waits included;
- `"FARE"`: the fare by a fare structure (see [Fares](fares.md));
- `"TRANSFERS"`: rides minus one; a trip without public transport has 0, as
  many as a trip with a single ride;
- `"WALK"`: minutes on foot: the access and egress legs when they are
  walked, and the walks between stops (a bicycle or car leg counts 0).

By default all four are used with a fare structure, all but `"FARE"`
without one. At each departure minute of the window the options are the
exact set of trips not dominated, with one trip per combination of values.
The trips are those of r5r's network semantics (the same access and egress
searches, transfers between stops and boarding rules as
`detailedItineraries()`), so the times agree with the other functions.

The example downloads r5r's sample of Porto Alegre and its fare structure
once into the temporary directory:

```js
{{#include ../examples/trip-options.mjs:setup}}
```

## Time and fare

```js
{{#include ../examples/trip-options.mjs:time-fare}}
```

```text
{{#include ../examples/output/trip-options.txt:1:6}}
```

Each row is an *option*: trips with the same street modes, the same routes
between the same stops, and the same fare, transfers and minutes on foot,
found at `n_departures` of the window's minutes. Its total time changes with
the minute (the wait at the start does), so the row reports the trip of its
best minute, `departure_time`. Over the whole window an option can look
dominated by another that was not available at its own minutes;
`byDeparture: true` adds the frontier of every minute:

```js
{{#include ../examples/trip-options.mjs:by-departure}}
```

```text
{{#include ../examples/output/trip-options.txt:7:16}}
```

## All four criteria and the legs

The result is `{ options, itineraries, departures }`. The legs of each
option are in `itineraries`, in the format of `detailedItineraries()` and
keyed by `from_id`, `to_id` and `option` (a GeoJSON `FeatureCollection`, or
plain rows with `dropGeometry: true`); `itineraries: false` skips them, and
`departures` is null without `byDeparture`.

```js
{{#include ../examples/trip-options.mjs:legs}}
```

```text
{{#include ../examples/output/trip-options.txt:17:}}
```

`maxOptions` keeps the first options of each pair, and `maxFare` and
`maxTripDuration` leave out the trips beyond them. With a pool of workers
(`nThreads` in `buildNetwork()`) the pairs are shared out and the result is
the same.

## Fares are not additive

A fare is not the sum of its rides: a transfer within the time allowance is
discounted, some types have unlimited transfers, a cap may apply. Two
partial trips are only compared on fare when one is at least as cheap for
every way both can continue; the comparison makes no assumption about the
fare structure. This exactness has a price: partial trips that could still
get a discount are kept apart until they cannot, which makes the fare
criterion the slowest one.

## Frequency-based routes

GTFS `frequencies.txt` gives a headway, not departure times. r5r draws
random departures (Monte Carlo); `tripOptions()` does not: a
frequency-based vehicle is boarded at its expected time, half a headway
after the traveller is ready to board (or after the period starts), within
the period, and with `exact_times = 1` at the first scheduled departure.
The results do not change from call to call, there is no `seed`, and they
differ on purpose from r5r's draws.

## Compared with the functions of r5r

- `paretoFrontier()` gives, for each fare cutoff, the percentiles of the
  travel time over Monte Carlo draws of departure times: a summary of time
  against budget, without the trips.
- `detailedItineraries()` gives the fastest trip, or with
  `shortestPath: false` R5's alternatives (one per sequence of routes).
- `tripOptions()` gives the trade-offs themselves, trip by trip, with their
  legs. With `criteria: ["TIME"]` and `timeWindow: 1`, its first option has
  the total time of `detailedItineraries()`'s fastest trip for that minute.
