# Inspecting a network

Five functions look into a network instead of routing on it: where points
snap to the streets, the extent of the street network, the streets
themselves, the public transport stops and routes, and the dates when
public transport runs. The functions that return geometries need geopandas
(`pip install "peabiru[geo]"`). The examples use the sample network:

```python
import pandas as pd

import peabiru

network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
points = peabiru.sample_points()
```

## Where points snap

Routing places each origin and destination on the nearest street the mode
may use, looked for first within 300 metres of the point and then, if there
is none, within `radius` (1,600 m by default). A point that does not snap
is not routed. [`find_snap()`][peabiru.find_snap] shows where each point
snaps, and how far away:

```python
snaps = peabiru.find_snap(network, points, mode="CAR")
print(snaps.head(3).to_string())
```

```text
          point_id        lat        lon   snap_lat   snap_lon   distance  found
0  89a8100c00fffff -23.556312 -46.628965 -23.555916 -46.629602  78.357652   True
1  89a8100c007ffff -23.557703 -46.632067 -23.558186 -46.632041  53.681610   True
2  89a8100c077ffff -23.553245 -46.628874 -23.553252 -46.628872   0.768439   True
```

The street a point snaps to depends on the mode, since some streets cannot
be used by some modes (a pedestrian street cannot be used to snap car
trips). A point too far from every street has `found` False and NaN
coordinates:

```python
far = pd.DataFrame({"id": ["far away"], "lon": [-46.70], "lat": [-23.60]})
print(peabiru.find_snap(network, far).to_string())
```

```text
   point_id   lat   lon  snap_lat  snap_lon  distance  found
0  far away -23.6 -46.7       NaN       NaN       NaN  False
```

## The extent of the street network

[`street_network_bbox()`][peabiru.street_network_bbox] gives the box that
covers every OpenStreetMap node of the ways streets were made from, as a
polygon (a `GeoSeries`, the default), a tuple or a dict:

```python
print(peabiru.street_network_bbox(network, output="bbox"))
print(peabiru.street_network_bbox(network, output="polygon").to_wkt()[0])
```

```text
(-46.6477135, -23.5653412, -46.6113004, -23.5371085)
POLYGON ((-46.6113 -23.565341, -46.6113 -23.537108, -46.647714 -23.537108, -46.647714 -23.565341, -46.6113 -23.565341))
```

## Streets

[`street_network_to_sf()`][peabiru.street_network_to_sf] returns the
vertices and the directed edges of the street network as GeoDataFrames in
EPSG:4326. Each edge says which modes may use it, its car speed and its
level of traffic stress for cycling:

```python
streets = peabiru.street_network_to_sf(network)
edges = streets["edges"]
print(len(streets["vertices"]), "vertices,", len(edges), "edges")
print(edges.drop(columns="geometry").head(3).to_string())
```

```text
1937 vertices, 5222 edges
   edge_index   osm_id  from_vertex  to_vertex street_class   length   walk    car  car_speed  bicycle  bicycle_lts
0           0  4331470            0          1        OTHER  178.805  False   True  50.004002     True            4
1           1  4331470            1          0        OTHER  178.805  False  False  50.004002    False            4
2           2  8103459            2          3        OTHER   73.563   True   True  20.016001     True            2
```

The connectors of transit stops and park-and-rides are left out, and only
the vertices at the ends of street edges are listed, so there are fewer of
both than [`Network.info()`][peabiru.Network.info] counts. The `osm_id` and
`edge_index` of an edge are what [scenarios](scenarios.md) and
`detailed_itineraries(osm_link_ids=True)` refer to:

```python
print(edges.groupby("street_class")[["walk", "car", "bicycle"]].mean().round(2))
```

```text
              walk   car  bicycle
street_class                     
OTHER         0.97  0.25     0.65
PRIMARY       1.00  0.37     0.53
SECONDARY     1.00  0.51     0.63
TERTIARY      0.99  0.50     0.59
```

## Public transport stops and routes

[`transit_network_to_sf()`][peabiru.transit_network_to_sf] returns the
stops and the routes of the GTFS feeds. There is one route row per trip
pattern (the trips of a route with the same stops), so a route can appear
more than once; its line is the GTFS shape of the pattern's first trip or,
without one, the positions of its stops:

```python
transit = peabiru.transit_network_to_sf(network)
stops, routes = transit["stops"], transit["routes"]
print(stops.drop(columns="geometry").head(3).to_string())
print(stops["linked_to_street"].value_counts().to_dict())
print(routes.drop(columns="geometry").head(3).to_string())
```

```text
   stop_index             stop_id              stop_name  linked_to_street
0           0  spo_gtfs:100014307    R. Silva Teles, 281              True
1           1  spo_gtfs:100014308     R. Silva Teles, 99              True
2           2  spo_gtfs:100014309  R. João Teodoro, 1331              True
{False: 506, True: 148}
  agency_id agency_name route_id                                long_name short_name mode
0         1     SPTRANS  2002-10   Term. Pq. D. Pedro Ii - Term. Bandeira    2002-10  BUS
1         1     SPTRANS  2105-10  Jd. Filhos Da Terra - Lgo. Da Concórdia    2105-10  BUS
2         1     SPTRANS  2105-10  Jd. Filhos Da Terra - Lgo. Da Concórdia    2105-10  BUS
```

Most stops of the sample feed are outside the area of the `.pbf`: they are
not linked to the street network, and their points are empty. A pattern
with no linked stop has no line and is left out; one with a single linked
stop and no shape is a `Point` (shapely has no line of one point):

```python
print(routes.geometry.geom_type.value_counts().to_dict())
```

```text
{'LineString': 22, 'Point': 6}
```

## When public transport runs

Public transport routing finds trips only on dates when services run.
[`check_transit_availability()`][peabiru.check_transit_availability]
counts the services active on given dates, and their share of all
services:

```python
print(peabiru.check_transit_availability(network, start_date="2019-05-11",
                                         end_date="2019-05-14").to_string())
```

```text
        date  total_services  active_services  pct_active
0 2019-05-11               6                4    0.666667
1 2019-05-12               6                3    0.500000
2 2019-05-13               6                3    0.500000
3 2019-05-14               6                3    0.500000
```

Give `dates` (a date, a `"YYYY-MM-DD"` string or a list of them) or both
`start_date` and `end_date`. With no date at all, it lists every date on
which a service used by some trip runs:

```python
dates = peabiru.check_transit_availability(network)
print(len(dates), "dates, from", dates["date"].min().date(), "to", dates["date"].max().date())
```

```text
4505 dates, from 2008-01-01 to 2020-05-01
```

The routing functions warn when few services run on the departure date.
To see the errors of the GTFS feeds themselves, see
[GTFS errors](gtfs-errors.md).
