# Detailed itineraries

[`detailed_itineraries()`][peabiru.detailed_itineraries] gives the legs of
each trip between pairs of origins and destinations: walking, cycling or
driving, and each public transport ride, with its duration, waiting time,
distance, route and line. It is r5r's `detailed_itineraries()` (r5r 2.5.0):
the same arguments, the same rows in the same order, the same numbers and
the same coordinates.

The examples use the sample data of Porto Alegre from r5r 2.5.0, downloaded
once into the temporary directory (about 2 MB); its GTFS feeds only have
service in the afternoon, hence the 14:00 departure.

```python
import datetime as dt
import tempfile
import urllib.request
from pathlib import Path

import pandas as pd

import peabiru

poa = Path(tempfile.gettempdir()) / "peabiru-poa"
poa.mkdir(exist_ok=True)
url = "https://raw.githubusercontent.com/ipea/r5r/v2.5.0/r-package/inst/extdata/poa/"
for name in ["poa_osm.pbf", "poa_eptc.zip", "poa_trensurb.zip", "poa_points_of_interest.csv"]:
    if not (poa / name).exists():
        urllib.request.urlretrieve(url + name, poa / name)

network = peabiru.build_network(poa)  # a .pbf and GTFS .zip files
points = pd.read_csv(poa / "poa_points_of_interest.csv")  # id, lat, lon

trips = peabiru.detailed_itineraries(
    network, points.iloc[:3], points.iloc[3:6],
    mode=["WALK", "TRANSIT"],
    departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
)
print(trips.drop(columns=["from_lat", "from_lon", "to_lat", "to_lon", "geometry"]).to_string())
```

```text
               from_id                to_id  option departure_time  total_duration  total_distance  segment  mode  segment_duration  wait  distance route
0        public_market  santa_casa_hospital       1       14:00:00            13.1             792        1  WALK              13.1   0.0       792      
1  bus_central_station             townhall       1       14:06:00            10.9            1792        1  WALK               3.6   0.0       206      
2  bus_central_station             townhall       1       14:06:00            10.9            1792        2   BUS               4.0   1.4      1475  2441
3  bus_central_station             townhall       1       14:06:00            10.9            1792        3  WALK               1.9   0.0       111      
4     gasometer_museum      piratini_palace       1       14:01:00            12.6            1393        1  WALK               4.1   0.0       224      
5     gasometer_museum      piratini_palace       1       14:01:00            12.6            1393        2   BUS               4.1   1.6      1001    C1
6     gasometer_museum      piratini_palace       1       14:01:00            12.6            1393        3  WALK               2.8   0.0       168
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

With `shortest_path=False` the result lists the alternatives R5's
McRAPTOR finds, one per sequence of routes, up to `suboptimal_minutes`
slower than the best; with a fare structure, each with its fares (see
[Fares](fares.md)).

As in r5r, a network whose GTFS has frequency-based
trips cannot be used with public transport here.

With `mode_egress="CAR"`, R5 reverses the via edges of turn restrictions
with two or more via ways in place each time its backward egress search
checks one, so what r5r returns for a pair whose car searches cross such a
restriction can depend on the searches that ran before (in the same call,
on other threads, or in earlier calls). peabiru plans such a pair for every
order those via edges can be in: when the itineraries are the same in all
of them, they are returned (they are r5r's whatever ran before); otherwise
the call raises an error naming the pair.
