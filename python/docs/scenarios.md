# Scenarios

The routing functions take three arguments that change the street network
for one call, without building it again:

- `new_carspeeds`: new car speeds, by OpenStreetMap way or inside polygons;
- `carspeed_scale`: a factor for the car speeds of every other street;
- `new_lts`: new levels of traffic stress (LTS) for cycling, by
  OpenStreetMap way or near lines.

They are accepted by [`travel_time_matrix()`][peabiru.travel_time_matrix],
[`accessibility()`][peabiru.accessibility],
[`expanded_travel_time_matrix()`][peabiru.expanded_travel_time_matrix],
[`arrival_travel_time_matrix()`][peabiru.arrival_travel_time_matrix] and
[`detailed_itineraries()`][peabiru.detailed_itineraries]. A scenario
changes the street search from the origins (the trip without public
transport, and access to it by bicycle or car) but not the egress from the
stops, as in R5, whose scenarios these are. The examples use the sample
network; the OpenStreetMap ids come from its primary roads:

```python
import geopandas as gpd
import pandas as pd

import peabiru

network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
points = peabiru.sample_points()
edges = peabiru.street_network_to_sf(network)["edges"]
primary = edges[edges["street_class"] == "PRIMARY"]
primary_ids = primary["osm_id"].unique()


def summary(mode, **scenario):
    """Pairs reached and their mean travel time, by one street mode."""
    ttm = peabiru.travel_time_matrix(network, points, points, mode=mode,
                                     n_threads=2, **scenario)
    return len(ttm), round(float(ttm["travel_time_p50"].astype(float).mean()), 2)


print(len(primary_ids), "primary ways;", "by car:", summary("CAR"))
```

```text
88 primary ways; by car: (729, 7.46)
```

## Car speeds by way

`new_carspeeds` as a DataFrame has columns `osm_id`, `max_speed` and
`speed_type`. With `speed_type="scale"`, `max_speed` multiplies the car
speed of the way's edges; with `"km/h"` it replaces it. A value of 0
closes the way to cars. `carspeed_scale` multiplies the speeds of the
edges `new_carspeeds` leaves out (all of them without `new_carspeeds`):

```python
slower = pd.DataFrame({"osm_id": primary_ids, "max_speed": 0.5, "speed_type": "scale"})
at_20 = pd.DataFrame({"osm_id": primary_ids, "max_speed": 20.0, "speed_type": "km/h"})
closed = pd.DataFrame({"osm_id": primary_ids, "max_speed": 0.0, "speed_type": "scale"})
print("primary roads at half speed:", summary("CAR", new_carspeeds=slower))
print("primary roads at 20 km/h:   ", summary("CAR", new_carspeeds=at_20))
print("primary roads closed:       ", summary("CAR", new_carspeeds=closed))
print("every street at 80%:        ", summary("CAR", carspeed_scale=0.8))
print("20 km/h, the rest at 80%:   ", summary("CAR", new_carspeeds=at_20, carspeed_scale=0.8))
```

```text
primary roads at half speed: (729, 7.54)
primary roads at 20 km/h:    (729, 7.6)
primary roads closed:        (651, 7.52)
every street at 80%:         (729, 8.14)
20 km/h, the rest at 80%:    (729, 8.26)
```

With the primary roads closed, one of the points, which snaps to one of
them, can only reach itself by car, and so 78 pairs are lost.

## Car speeds in polygons

`new_carspeeds` as a GeoDataFrame of polygons in WGS 84 has columns
`poly_id` (strings), `scale` (a factor) and `priority` (integers). Each
street edge takes the factor of the polygon it meets with the highest
priority (ties: the one it overlaps most); `carspeed_scale` applies to the
edges outside every polygon:

```python
bbox = peabiru.street_network_bbox(network, "vector")
centre = gpd.points_from_xy([(bbox["xmin"] + bbox["xmax"]) / 2],
                            [(bbox["ymin"] + bbox["ymax"]) / 2])
zone = gpd.GeoDataFrame(
    {"poly_id": ["centre"], "scale": [0.3], "priority": [1]},
    geometry=centre.buffer(0.005),  # about 500 m around the centre
    crs="EPSG:4326",
)
print("30% of the speed within 500 m of the centre:", summary("CAR", new_carspeeds=zone))
```

```text
30% of the speed within 500 m of the centre: (729, 9.66)
```

## Levels of traffic stress

`max_lts` (2 by default) is the highest level of traffic stress, from 1 to
4, a cyclist rides on. `new_lts` changes the level of streets: as a
DataFrame with columns `osm_id` and `lts` (integers), or as a GeoDataFrame
of lines in WGS 84 with columns `line_id`, `lts` (1 to 4) and `priority`,
where each street edge takes the level of the line closest to it among
those whose bounding box meets its own:

```python
calm = pd.DataFrame({"osm_id": primary_ids, "lts": 1})
lanes = gpd.GeoDataFrame(
    {"line_id": primary["edge_index"].astype(str), "lts": 1, "priority": 1},
    geometry=primary.geometry.values,
    crs="EPSG:4326",
)
print("by bicycle:                 ", summary("BICYCLE"))
print("primary roads at LTS 1:     ", summary("BICYCLE", new_lts=calm))
print("cycle lanes along them:     ", summary("BICYCLE", new_lts=lanes))
```

```text
by bicycle:                  (729, 11.82)
primary roads at LTS 1:      (729, 11.21)
cycle lanes along them:      (729, 10.87)
```

The lines lower the stress of more streets than the way ids do: an edge
takes the level of the closest line whose bounding box meets its own, so
the streets next to a primary road, whose bounding boxes meet a line's,
get level 1 too.

## What is an error

A scenario that R5 would corrupt is an error rather than a result:

- a car speed that R5's 16-bit centimetres per second would wrap or bring
  to 0 or less (a negative factor or speed, a polygon `scale` of `math.inf`
  or 0, a `carspeed_scale` far above 1), and a `carspeed_scale` beyond the
  32-bit float range;
- a `carspeed_scale` of NaN;
- a polygon that is not valid, and ties between polygons that peabiru
  cannot settle exactly as R5 does.

```python
backwards = pd.DataFrame({"osm_id": primary_ids, "max_speed": -1.0, "speed_type": "scale"})
try:
    summary("CAR", new_carspeeds=backwards)
except ValueError as e:
    print(str(e)[:72], "...")
```

```text
the scenario leaves 140 street edges open to cars with a speed of 0 or l ...
```

On a network with an elevation raster, a call with a scenario runs without
the slope costs, with a warning (see [Elevation](elevation.md#elevation-and-scenarios)).
