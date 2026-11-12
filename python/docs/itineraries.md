# Detailed itineraries

[`detailed_itineraries()`][peabiru.detailed_itineraries] gives the legs of
each trip between pairs of origins and destinations: walking, cycling or
driving, and each public transport ride, with its duration, waiting time,
distance, route and line.

The examples use the sample data of Porto Alegre from the r5r project, downloaded
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
Of note:

- a street leg's line holds the whole of the first and last street
  segments, not only the parts walked or driven; the leg from the last stop
  ends at the destination itself;
- a ride follows the GTFS shape of its trip pattern, or straight lines
  between stops when the feed has no shapes;
- a walk between stops is routed again on the streets for its line and
  distance; its duration is the one public transport routing used.

## Alternatives

With `shortest_path=False`, each pair gets, besides the trip by the street
mode alone, the alternatives R5's multi-criteria router (McRAPTOR) finds
over the departure window, one per sequence of routes. `suboptimal_minutes`
(which must be 0 with `shortest_path=True`) keeps search branches up to
that many minutes slower than the best one, so higher values return more
itineraries. The options are numbered by `option` in this order: the trip
by the street mode alone first, then by the number of legs, then by
duration, so the first option is not the fastest when a trip with fewer
legs is slower (on Porto Alegre with `suboptimal_minutes=10`, a pair had 102
options, the first of 60.4 minutes and the fastest of 53.3):

```python
def alternatives(minutes):
    return peabiru.detailed_itineraries(
        network, points.iloc[[1]], points.iloc[[4]],
        mode=["WALK", "TRANSIT"],
        departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
        shortest_path=False, suboptimal_minutes=minutes, drop_geometry=True,
    )


for minutes in [0, 1, 5]:
    print(minutes, "minutes:", alternatives(minutes)["option"].nunique(), "options")
print(alternatives(0)[["option", "departure_time", "total_duration", "segment", "mode",
                       "segment_duration", "wait", "route"]].to_string())
```

```text
0 minutes: 7 options
1 minutes: 10 options
5 minutes: 19 options
    option departure_time  total_duration  segment  mode  segment_duration  wait route
0        1       14:00:00            23.4        1  WALK              23.4   0.0      
1        2       14:04:18            10.8        1  WALK               3.6   0.0      
2        2       14:04:18            10.8        2   BUS               3.6   1.1   255
3        2       14:04:18            10.8        3  WALK               2.6   0.0      
4        3       14:05:51            11.0        1  WALK               3.6   0.0      
5        3       14:05:51            11.0        2   BUS               4.0   1.6  2441
6        3       14:05:51            11.0        3  WALK               1.9   0.0      
7        4       14:02:02            12.0        1  WALK               3.6   0.0      
8        4       14:02:02            12.0        2   BUS               4.0   1.9   346
9        4       14:02:02            12.0        3  WALK               2.6   0.0      
10       5       14:09:26            12.2        1  WALK               3.6   0.0      
11       5       14:09:26            12.2        2   BUS               4.3   2.7  2821
12       5       14:09:26            12.2        3  WALK               1.6   0.0      
13       6       14:00:43            13.1        1  WALK               3.6   0.0      
14       6       14:00:43            13.1        2   BUS               3.5   1.2   397
15       6       14:00:43            13.1        3  WALK               4.8   0.0      
16       7       14:06:46            13.5        1  WALK               3.6   0.0      
17       7       14:06:46            13.5        2   BUS               3.4   2.2   525
18       7       14:06:46            13.5        3  WALK               4.2   0.0
```

With a fare structure, the alternatives come with the fare of each trip
(`total_fare`) and the fare paid by the end of each leg
(`cumulative_fare`); see [Fares](fares.md#alternative-itineraries-and-their-fares).

## Pairs

Origins and destinations are paired row by row, and a single origin or
destination is repeated to match the other, with a warning; with
`all_to_all=True` every origin is routed to every destination. The street
examples below use the sample network of Sao Paulo, which is quicker to
build:

```python
import warnings

spo = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
spo_points = peabiru.sample_points()
pairs = peabiru.detailed_itineraries(
    spo, spo_points.iloc[:3], spo_points.iloc[3:6], mode="WALK",
    drop_geometry=True, n_threads=2,
)
every = peabiru.detailed_itineraries(
    spo, spo_points.iloc[:3], spo_points.iloc[3:6], mode="WALK",
    all_to_all=True, drop_geometry=True, n_threads=2,
)
with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always")
    one = peabiru.detailed_itineraries(
        spo, spo_points.iloc[:1], spo_points.iloc[3:6], mode="WALK",
        drop_geometry=True, n_threads=2,
    )
def n_pairs(trips):
    return len(trips[["from_id", "to_id"]].drop_duplicates())


print(n_pairs(pairs), "pairs;", n_pairs(every), "with all_to_all;", n_pairs(one), "from one origin")
print(caught[0].message)
```

```text
3 pairs; 9 with all_to_all; 3 from one origin
'origins' was expanded to match the number of destinations.
```

## The streets of each leg

With `osm_link_ids=True` (which needs the geometry, so not with
`drop_geometry=True`), each street leg also lists the OpenStreetMap ways
(`osm_id_list`) and the edges of the street network (`edge_id_list`, as
[`street_network_to_sf()`][peabiru.street_network_to_sf] numbers them) it
goes along, and each ride its board and alight stops (`board_stop_id`,
`alight_stop_id`, as `"<feed>:<stop_id>"`). Consecutive edges on a
roundabout give only their first ids, and connectors none:

```python
bike = peabiru.detailed_itineraries(
    spo, spo_points.iloc[[0]], spo_points.iloc[[5]], mode="BICYCLE",
    osm_link_ids=True, n_threads=2,
)
print(bike[["mode", "distance", "osm_id_list"]].to_string())
```

```text
      mode  distance                                                                                                                                                                                                     osm_id_list
0  BICYCLE      1268  [424630871, 424630809, 8579169, 779662527, 16975352, 565942870, 565942869, 211060855, 841434976, 231281385, 231281386, 153916352, 153916347, 153916351, 153878962, 436639672, 431397815, 431397814, 368723480]
```

## Car egress and turn restrictions

With `mode_egress="CAR"`, R5 reverses the via edges of turn restrictions
with two or more via ways in place each time its backward egress search
checks one, so what R5 finds for a pair whose car searches cross such a
restriction can depend on the searches that ran before (in the same call,
on other threads, or in earlier calls). peabiru returns, for every pair,
the itineraries of a freshly built network for that pair alone with one
thread, and warns once per call, naming the pairs whose itineraries some
other order of those via edges would change (on Porto Alegre, about 1 pair
in 200). Rides along GTFS shapes cut by `shape_dist_traveled` follow
R5's cut. Networks whose
GTFS has frequency-based trips cannot be used with public transport here:
convert the frequencies to stop times first (with gtfstools'
`frequencies_to_stop_times()` in R, for instance).

## Trip options

To see every trip worth considering between two points, faster or cheaper, with fewer transfers or less walking, and not only the fastest one, see [`trip_options()`][peabiru.trip_options], peabiru's own multi-criteria planner: [Trip options](trip-options.md).
