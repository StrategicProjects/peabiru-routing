# Fares and Pareto frontiers

A `FareStructure` is r5r 2.5.0's fare structure, with the values as r5r's R
object holds them (doubles, `Inf` included): the global settings
(`max_discounted_transfers`, `transfer_time_allowance` in minutes,
`fare_cap`), one `FarePerType` per fare type, one `FarePerTransfer` per pair
of types with a discounted transfer, and one `FarePerRoute` per GTFS route
(matched by `agency_id` and `route_id`).

```rust,ignore
pub fn setup_fare_structure(layer: &TransitLayer, base_fare: f64, by: &str)
    -> Result<FareStructure, RouteError>;
pub fn read_fare_structure(bytes: &[u8], encoding: &str) -> Result<FareStructure, RouteError>;
pub fn write_fare_structure(s: &FareStructure) -> Result<Vec<u8>, RouteError>;
```

`setup_fare_structure` makes one where every ride and every discounted
transfer costs `base_fare`, with one fare type per mode (`by = "MODE"`), per
agency (`"AGENCY_ID"`, `"AGENCY_NAME"`) or a single `GENERIC` type; change
its fields to follow a city's rules. `read_fare_structure` and
`write_fare_structure` read and write the bytes of r5r's `.zip` file: five
CSV files in `data.table::fwrite()`'s format, the same text as r5r's (the
archive stores them uncompressed, so its bytes differ). A structure written
by r5r is read here, and the other way round.

## Routing with fares

Set `TransitParams::fare_structure` (and `max_fare`) to route with fares:
the matrix functions then keep, for each pair, the fastest trip that costs
no more than `max_fare`. Accessibility takes the same fields.

```rust,ignore
let mut p = peabiru_core::TransitParams::default();
p.date = 20190513;
p.departure_time = 14 * 3600;
p.fare_structure = Some(std::sync::Arc::new(peabiru_core::read_fare_structure(&bytes, "UTF-8")?));
p.max_fare = 4.8;
let m = peabiru_core::transit_travel_time_matrix(&net, &layer, &data, &o, &d, &p)?;
```

With a fare structure, public transport is routed as r5r routes it then:
R5's multi-criteria router (`McRaptorSuboptimalPathProfileRouter`, ported
with its tie-breaking and iteration orders) with R5's `FareDominatingList`
and r5r's `RuleBasedInRoutingFareCalculator`, over `time_window x
draws_per_minute` departure times sampled in the window (seeded from the
origin as r5r seeds them), instead of RAPTOR at every minute. The numbers
go through the conversions r5r's Java side makes: `jsonlite` writes them
with four decimals, Jackson reads fares into `float`s and counts into
`int`s, and the calculator works in integer cents.

`pareto_frontier` is r5r's `pareto_frontier()`: the matrix once per fare
cutoff, and per destination and percentile a row at each cutoff where the
travel time changes. `detailed_itineraries` takes the same structure in
`ItineraryParams::fare_structure` (and routes with McRAPTOR then, as it
does with `shortest_path = false`); its itineraries get `total_fare` and
each leg `cumulative_fare` (see [Detailed itineraries](itineraries.md)).

The fare structure is r5r's rule-based one. R5's own fare calculators for other cities (a JSON structure with a `type`)
are not ported.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/fares.rs}}
```

On the São Paulo sample, a few kilometres across, walking is as fast as any
paid trip from the first point, and no budget changes the times:

```text
type SUBWAY  fare 5.00
type BUS     fare 5.00
type RAIL    fare 5.00
9 transfer rules, 19 routes, at most 1 discounted transfers within 120 min
max_fare    0: 27 destinations, median 25 min
max_fare    5: 27 destinations, median 25 min
max_fare  inf: 27 destinations, median 25 min
```

On r5r's Porto Alegre sample (the directory of the
[itineraries example](itineraries.md#example)), with the fare structure r5r
ships for it as the second argument (`extdata/poa/fares/fares_poa.zip`),
one fare buys most of the time saved, and a second one more for some
destinations:

```text
type BUS     fare 4.80
type RAIL    fare 4.50
3 transfer rules, 117 routes, at most 1 discounted transfers within 60 min
max_fare    0: 14 destinations, median 26 min
max_fare    5: 15 destinations, median 16 min
max_fare  inf: 15 destinations, median 16 min

to bus_central_station: 21 min for 0.00 13 min for 5.00
to gasometer_museum: 26 min for 0.00 12 min for 5.00
to farroupilha_park: 31 min for 0.00 17 min for 5.00
to moinhos_de_vento_hospital: 37 min for 0.00 26 min for 5.00 24 min for 10.00
to farrapos_station: 82 min for 0.00 21 min for 5.00
to iguatemi_shopping_center: 59 min for 5.00 49 min for 10.00
to praia_de_belas_shopping_center: 44 min for 0.00 21 min for 5.00
to beira_rio_stadium: 77 min for 0.00 32 min for 5.00
to ufrgs: 21 min for 0.00 16 min for 5.00
to pucrs: 114 min for 0.00 44 min for 5.00 35 min for 10.00
```

## Trip options

To see every trip worth considering between two points, faster or cheaper, with fewer transfers or less walking, and not only the fastest one, see `trip_options`, peabiru's own multi-criteria planner: [Trip options](trip-options.md).
