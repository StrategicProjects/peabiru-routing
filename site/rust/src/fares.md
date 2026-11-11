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

`read_fare_structure` and `write_fare_structure` read and write the bytes of
r5r's `.zip` file: five CSV files in `data.table::fwrite()`'s format, the
same text as r5r's (the archive stores them uncompressed, so its bytes
differ).

Set `TransitParams::fare_structure` (and `max_fare`) to route with fares:

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
each leg `cumulative_fare`.

R5's own fare calculators (a JSON structure with a `type`) are not ported.
