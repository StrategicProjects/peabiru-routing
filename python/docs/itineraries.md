# Detailed itineraries

[`detailed_itineraries()`][peabiru.detailed_itineraries] gives the legs of
each trip between pairs of origins and destinations: walking, cycling or
driving, and each public transport ride, with its duration, waiting time,
distance, route and line. It is r5r's `detailed_itineraries()` (r5r 2.5.0):
the same arguments, the same rows in the same order, the same numbers and
the same coordinates.

```python
import datetime as dt
import peabiru

network = peabiru.build_network("data/poa")  # a .pbf and GTFS .zip files
points = ...  # a DataFrame with id, lon, lat

trips = peabiru.detailed_itineraries(
    network, points.iloc[:10], points.iloc[10:20],
    mode=["WALK", "TRANSIT"],
    departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
)
```

Origins and destinations are paired row by row (a single origin or
destination is repeated to match the other); `all_to_all=True` routes every
origin to every destination. Each pair gets its fastest itinerary
(`shortest_path=True`): the trip by the street mode alone, or the fastest
public transport trip over the departure window, whichever is faster. Pairs
without one have no rows.

The result is a `geopandas.GeoDataFrame` of `LineString`s in EPSG:4326, one
row per leg (`segment`), or a `pandas.DataFrame` with `drop_geometry=True`.
As in r5r:

- a street leg's line holds the whole of the first and last street
  segments, not only the parts walked or driven; the leg from the last stop
  ends at the destination itself;
- a ride follows the GTFS shape of its trip pattern, or straight lines
  between stops when the feed has no shapes;
- a walk between stops is routed again on the streets for its line and
  distance; its duration is the one public transport routing used.

Not supported yet, and an [`UnsupportedError`][peabiru.UnsupportedError]:
`shortest_path=False` with public transport (r5r then uses R5's McRAPTOR
to list alternatives), `osm_link_ids=True`, `output_dir` with the geometry,
fares and scenarios. As in r5r, a network whose GTFS has frequency-based
trips cannot be used with public transport here.
