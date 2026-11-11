# Travel-time matrices

[`travel_time_matrix()`](reference.md#peabiru.travel_time_matrix) computes
the travel time from every origin to every destination, by a street mode
or by public transport. The examples on this page use the sample network:

```python
import datetime as dt

import pandas as pd

import peabiru

network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
points = peabiru.sample_points()
```

## Street modes

With one street mode, `"WALK"`, `"BICYCLE"` or `"CAR"`, the result has one
row per pair that can be reached within `max_trip_duration` (120 minutes by
default), in origin-major order, following the order of the input points:

```python
car = peabiru.travel_time_matrix(network, points, points, mode="CAR", n_threads=2)
print(car.head(3))
print(car.dtypes)
```

```text
           from_id            to_id  travel_time_p50
0  89a8100c00fffff  89a8100c00fffff                2
1  89a8100c00fffff  89a8100c007ffff                4
2  89a8100c00fffff  89a8100c077ffff                2
from_id            object
to_id              object
travel_time_p50     Int32
dtype: object
```

`from_id` and `to_id` are strings (object dtype); travel times are whole
minutes in pandas' nullable `Int32`. Speeds and limits are arguments:

```python
bike = peabiru.travel_time_matrix(
    network, points, points,
    mode="BICYCLE",
    bike_speed=15,         # km/h
    max_lts=3,             # highest level of traffic stress, 1 to 4
    max_bike_time=10,      # minutes
    n_threads=2,
)
print(len(bike), "pairs within 10 minutes;", bike["travel_time_p50"].max(), "at most")
```

```text
406 pairs within 10 minutes; 10 at most
```

A street trip has one travel time, so every requested percentile gets the
same value:

```python
walk = peabiru.travel_time_matrix(
    network, points, points, mode="WALK", percentiles=[75, 25], n_threads=2
)
print(list(walk.columns))
```

```text
['from_id', 'to_id', 'travel_time_p25', 'travel_time_p75']
```

!!! note "Walking with more origins than destinations"
    When there are more origins than destinations (and no elevation
    raster), walking trips are routed *from the destinations* and the
    columns are swapped back. The two directions can differ on one-way
    paths; the result still has `from_id` as the caller's origins.

## Public transport

`mode=["WALK", "TRANSIT"]` (or just `"TRANSIT"`) routes by public transport
with walking access and egress. A `departure_datetime` is required; travel
times are computed for every departure minute of `time_window` and summarised
by `percentiles` (up to five, reported in ascending order):

```python
ttm = peabiru.travel_time_matrix(
    network, points, points,
    mode=["WALK", "TRANSIT"],
    departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    time_window=30,
    percentiles=[5, 50, 95],
    max_trip_duration=20,
    seed=42,
    n_threads=2,
)
print(ttm[ttm["travel_time_p95"].isna()].head(3).to_string())
```

```text
            from_id            to_id  travel_time_p05  travel_time_p50  travel_time_p95
16  89a8100c007ffff  89a8100c393ffff               19             <NA>             <NA>
44  89a8100c03bffff  89a8100c383ffff               18             <NA>             <NA>
46  89a8100c03bffff  89a8100c15bffff               17               19             <NA>
```

A pair is reported when its lowest percentile is within `max_trip_duration`;
a higher percentile beyond the limit is missing (`<NA>`).

### Departure times

The date and the clock time of `departure_datetime` are used as they read:
a naive datetime and an aware datetime with the same clock time give the
same result, whatever the time zone.

```python
from zoneinfo import ZoneInfo

kw = dict(mode="TRANSIT", seed=1, n_threads=2)
naive = peabiru.travel_time_matrix(
    network, points, points, departure_datetime=dt.datetime(2019, 5, 13, 14, 0), **kw
)
aware = peabiru.travel_time_matrix(
    network, points, points,
    departure_datetime=dt.datetime(2019, 5, 13, 14, 0, tzinfo=ZoneInfo("Asia/Tokyo")),
    **kw,
)
print(naive.equals(aware))
```

```text
True
```

### Frequency-based GTFS and the seed

When a feed describes trips by frequency (`frequencies.txt`, as the sample
feed does), the engine draws `draws_per_minute` random schedules for each
departure minute. Without a `seed` the result varies from call to call; with
a seed it is reproducible.

```python
kw = dict(mode="TRANSIT", departure_datetime=dt.datetime(2019, 5, 13, 14, 0), n_threads=2)
a = peabiru.travel_time_matrix(network, points, points, seed=7, **kw)
b = peabiru.travel_time_matrix(network, points, points, seed=7, **kw)
print(a.equals(b))
```

```text
True
```

## Odd inputs

Points with missing coordinates are not routed (with a warning), and points
too far from any street are simply absent from the result. Ids
that are not strings are converted with `str()`, with a warning.

```python
import warnings

odd = pd.concat(
    [
        points.iloc[:3],
        pd.DataFrame({"id": ["no_coords", "far_away"], "lon": [None, 0.0], "lat": [None, 0.0]}),
    ],
    ignore_index=True,
)
with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always")
    m = peabiru.travel_time_matrix(network, odd, points.iloc[:3], mode="WALK", n_threads=2)
print(caught[0].message)
print(sorted(set(m["from_id"])))
```

```text
1 point(s) in 'origins' have missing lon/lat coordinates and will not be routed. Affected id(s): "no_coords".
['89a8100c007ffff', '89a8100c00fffff', '89a8100c077ffff']
```

Duplicated ids, a missing `id`, `lon` or `lat` column and non-numeric
coordinates are errors.

### GeoDataFrames

With [geopandas](https://geopandas.org) installed, origins and destinations
can be a `GeoDataFrame` of points in EPSG:4326 with an `id` column; the
coordinates come from the geometry.

```python
import geopandas as gpd

geo = gpd.GeoDataFrame(
    points[["id", "jobs"]],
    geometry=gpd.points_from_xy(points["lon"], points["lat"]),
    crs="EPSG:4326",
)
from_geo = peabiru.travel_time_matrix(network, geo, geo, mode="CAR", n_threads=2)
print(from_geo.equals(car))
```

```text
True
```

## From long to wide

The result is in long format, one row per pair. A wide origin-by-destination table
is one `pivot` away:

```python
wide = car.pivot(index="from_id", columns="to_id", values="travel_time_p50")
print(wide.iloc[:3, :3])
```

```text
to_id            89a8100c007ffff  89a8100c00fffff  89a8100c023ffff
from_id                                                           
89a8100c007ffff                1                4                3
89a8100c00fffff                4                2                4
89a8100c023ffff                3                3                0
```

## Writing results to files

With `output_dir`, an existing directory, the result is written there
instead of being returned, as r5r writes it: one CSV file per origin, named
`from_<origin id>.csv`, and the function returns `output_dir` (a string or
a `pathlib.Path`, as given). The origins are routed a slice at a time and
each slice is written before the next one is routed, so a matrix too large
to hold in memory can still be computed. `accessibility()`,
`expanded_travel_time_matrix()` and `arrival_travel_time_matrix()` take
`output_dir` too.

```python
import tempfile
from pathlib import Path

out = Path(tempfile.mkdtemp())
print(peabiru.travel_time_matrix(network, points, points, mode="CAR",
                                 n_threads=2, output_dir=out) == out)
files = sorted(p.name for p in out.iterdir())
print(len(files), files[0])
print((out / files[0]).read_text().splitlines()[:3])
```

```text
True
27 from_89a8100c007ffff.csv
['from_id,to_id,travel_time_p50', '89a8100c007ffff,89a8100c00fffff,4', '89a8100c007ffff,89a8100c007ffff,1']
```

The files are byte for byte those r5r writes for the same call: the columns
of the result (accessibility files also keep `cutoff` under
`"fixed_exponential"`, as r5r's placeholder `0`), fields holding `,` or `"`
quoted, a missing value as an empty field, decimals as Java writes them
(`2.0`, `1.0E-4`), lines ended by `\n` (`\r\n` on Windows). An origin with
no row gets no file (accessibility writes one for every origin). The id is
not sanitized: an id with `/` names a file in a subdirectory, an error when
that does not exist. An existing file of the same name is replaced, other
files are left alone. As in r5r, `"WALK"` is then routed from the origins
even when there are fewer destinations, and the rule that blanks out trips
over `max_trip_duration` in a returned result is not applied to the files.

## Threads

`n_threads=None` (the default) uses all cores (`math.inf` is accepted
too). The Python interpreter lock is released while
the Rust core routes, so other Python threads keep running.

## Stopping a long call

Ctrl-C stops any of the routing functions within a few tenths of a second
and raises `KeyboardInterrupt`, as it does for Python code; the session
stays usable and there is no partial result. Signals reach only Python's
main thread, so a call made from another thread runs to the end.
`build_network()` cannot be interrupted.
