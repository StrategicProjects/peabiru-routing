# How it works

These diagrams describe what the engine computes and in which order: how
the network is built, how streets and public transport are searched, and how
the results are summarised. Every step has fixed rules for rounding and
tie-breaking, so the same inputs give the same outputs whatever the number of
threads. The data structures behind the steps are an implementation detail
and change as the engine is optimised.

## Architecture

<img src="../assets/diagrams/architecture.svg" alt="Architecture: one OpenStreetMap .pbf and zero or more GTFS .zip feeds go into peabiru-core, which builds the network in memory and computes every result; the R package, the Python package and the command line are front-ends over it." width="980" height="470">

## Building the network

<img src="../assets/diagrams/network-build.svg" alt="Network build in eight steps: read the extract; ways to edges with permissions, speeds and traffic stress; clean up (intersection stress, islands); index, park-and-ride and turn restrictions; read the GTFS feeds; trips to patterns; link stops to streets; walking tables for transfers (1 km) and egress (2 km)." width="980" height="470">

In the API: steps 1 to 4 are `street::build_network`; steps 5 to 7 happen in
`transit::build_from_dir` (`TransitLayer::load` and `link_stops`); step 8 is
`TransitData::new`.

## Street routing

<img src="../assets/diagrams/street-routing.svg" alt="Street routing: link each point to the nearest street open to the mode, within 300 m or else 1600 m; search directed edges from the origin, earliest arrival first, up to the time limit; reach each destination through either end of its edge. Edge costs: walking speed; bike speed up to max_lts, else walking the bike; car speeds, turn costs and turn restrictions." width="980" height="512">

## Public transport

<img src="../assets/diagrams/transit-matrix.svg" alt="Transit travel-time matrix: walk from the origin; RAPTOR over each departure minute, latest first, each minute reusing the later one's state, with Monte Carlo draws for frequency-based trips; walk from stops to each destination; percentiles of the iteration times." width="980" height="560">

RAPTOR works in rounds: round *k* finds the earliest arrival at every stop
using *k* vehicles, so `max_rides` is the number of rounds. Running the
minutes of the window from the last to the first lets each minute start from
the results of the later one (range RAPTOR). For frequency-based trips, each
minute is repeated `draws_per_minute` times with random offsets; the random
generator is a Mersenne Twister seeded from `seed` and the origin's
coordinates, so a given `seed` gives the same numbers on every run and with
any number of threads.

## Accessibility

<img src="../assets/diagrams/accessibility.svg" alt="Accessibility is the sum over destinations of opportunities times a decay weight of the travel-time percentile in seconds; the five decay functions are drawn for a 30 minute cutoff." width="980" height="420">

## Expanded and arrival matrices

<img src="../assets/diagrams/expanded-arrival.svg" alt="Expanded matrix: one row per departure minute and draw with the trip breakdown. Arrival matrix: departures scanned from the latest minute back, keeping the first trip that arrives in time." width="980" height="520">
