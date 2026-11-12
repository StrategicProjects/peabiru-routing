# Scenarios: car speeds and traffic stress

A `Scenario` changes the car speeds and the levels of traffic stress of the
streets for one call, as r5r's `new_carspeeds`, `carspeed_scale` and
`new_lts` do. The network itself is left as it is, so one network serves
every scenario. It goes in the `scenario` field of `RouteParams`,
`TransitParams` and `ItineraryParams`; the default changes nothing.

```rust,ignore
let mut s = Scenario::default();
s.car_speeds = CarSpeeds::ScaleByWay(vec![(27184648, 0.5)]); // one OSM way at half speed
s.car_speed_scale = 0.8;                                    // every other street
s.lts = LtsChange::ByWay(vec![(27184648, 1)]);
let mut p = RouteParams::default();
p.scenario = s;
```

- `car_speeds` (r5r's `new_carspeeds`):
  - `CarSpeeds::ScaleByWay(vec![(osm_way_id, factor)])` scales the car
    speed of the way's edges; 0 forbids cars on them;
  - `CarSpeeds::KmhByWay(vec![(osm_way_id, kmh)])` sets the speed in km/h;
    0 forbids cars;
  - `CarSpeeds::ByPolygon(vec![SpeedPolygon::new(polygons, factor, priority)])`
    scales the edges a polygon meets; where polygons overlap, the highest
    priority wins (ties: the longest overlap).
- `car_speed_scale` (r5r's `carspeed_scale`, default 1) scales every edge
  that `car_speeds` does not change.
- `lts` (r5r's `new_lts`): `LtsChange::ByWay(vec![(osm_way_id, level)])`, or
  `LtsChange::ByLine(vec![LtsLine::new(points, level)])`, where each street
  segment takes the level of the closest line.

OSM way ids are those of `street_edges` (see
[Inspecting a network](inspect.md)). Polygons and lines are `[lon, lat]`
degrees; a polygon is a shell then its holes, every ring closed. Invalid
values (a negative or non-finite factor, open rings, self-crossing
polygons) are rejected with `RouteError::Invalid`.

As in r5r, the scenario applies to the street searches from the origins
(direct trips and the access to public transport); the egress from the
stops and the walking transfers use the network as built, because R5 copies
them from it. Detailed itineraries route every street leg on the scenario.

## Example

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/scenarios.rs}}
```

```text
car, as built:           179 min
car, all speeds x 0.7:   207 min
car, primary 20 km/h:    182 min (88 ways)
car, x 0.5 near origin:  210 min
bicycle, as built:       354 min
bicycle, LTS 3-4 to 2:   268 min (914 ways)
```

With the stressful streets calmed to level 2, a cyclist with the default
`max_lts = 2` rides on them instead of walking the bicycle.
