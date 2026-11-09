# Expanded and arrival matrices

These two functions report individual trips rather than percentiles. Both
route by public transport with walking access and egress
(`mode=["WALK", "TRANSIT"]`); a street mode alone is an error (use
[`travel_time_matrix()`](travel-time-matrices.md)).

```python
import datetime as dt

import peabiru

network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
points = peabiru.sample_points()
```

## Expanded matrices

[`expanded_travel_time_matrix()`](reference.md#peabiru.expanded_travel_time_matrix)
gives the travel time of the trip found for every departure minute of
`time_window` and, with frequency-based GTFS, for every Monte Carlo draw
(`draws_per_minute`, 5 by default), as `r5r::expanded_travel_time_matrix()`
does. With `breakdown=True` each trip is split into access, waiting,
in-vehicle, transfer and egress times, and the number of rides:

```python
ettm = peabiru.expanded_travel_time_matrix(
    network, points.iloc[:2], points,
    mode=["WALK", "TRANSIT"],
    departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    time_window=2,
    breakdown=True,
    seed=42,
    n_threads=2,
)
print(ettm.dtypes)
trips = ettm[ettm["n_rides"] > 0]
cols = ["to_id", "departure_time", "draw_number", "wait_time", "ride_time",
        "routes", "total_time"]
print(trips[cols].head().to_string())
```

```text
from_id            object
to_id              object
departure_time     object
draw_number         int32
access_time       float64
wait_time         float64
ride_time         float64
transfer_time     float64
egress_time       float64
routes             object
n_rides             Int32
total_time        float64
dtype: object
               to_id departure_time  draw_number  wait_time  ride_time    routes  total_time
120  89a8100c393ffff       14:00:00            1        1.4        2.2   2002-10        22.2
130  89a8100c02fffff       14:00:00            1        1.5        1.9  METRÔ L1        21.4
131  89a8100c02fffff       14:00:00            2        1.5        1.9  METRÔ L1        21.4
132  89a8100c02fffff       14:00:00            3        1.6        1.9  METRÔ L1        21.5
133  89a8100c02fffff       14:00:00            4        2.0        1.9  METRÔ L1        21.9
```

Times are minutes with one decimal (`float64`). `routes` joins the route ids
of the rides with `"|"`, or is `"[WALK]"` for a trip on foot. Trips longer
than `max_trip_duration` are kept with missing values, as in r5r: `NaN`
times, `None` routes and `<NA>` rides.

```python
short = peabiru.expanded_travel_time_matrix(
    network, points.iloc[:3], points,
    mode="TRANSIT",
    departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    time_window=2,
    max_trip_duration=8,
    breakdown=True,
    seed=1,
    n_threads=2,
)
print(short[short["total_time"].isna()][["to_id", "routes", "n_rides", "total_time"]].head(3))
```

```text
              to_id routes  n_rides  total_time
50  89a8100c03bffff   None     <NA>         NaN
51  89a8100c03bffff   None     <NA>         NaN
52  89a8100c03bffff   None     <NA>         NaN
```

## Arrival matrices

[`arrival_travel_time_matrix()`](reference.md#peabiru.arrival_travel_time_matrix)
finds, for each origin and destination, the trip that leaves latest and
still arrives by `arrival_datetime`, as `r5r::arrival_travel_time_matrix()`
does: departures are searched minute by minute from
`arrival_datetime - max_trip_duration`.

```python
attm = peabiru.arrival_travel_time_matrix(
    network, points.iloc[:2], points,
    mode=["WALK", "TRANSIT"],
    arrival_datetime=dt.datetime(2019, 5, 13, 15, 0),
    max_trip_duration=30,
    seed=42,
    n_threads=2,
)
cols = ["from_id", "to_id", "departure_time", "routes", "total_time"]
print(attm.sort_values("total_time", ascending=False)[cols].head().to_string())
```

```text
            from_id            to_id departure_time             routes  total_time
22  89a8100c00fffff  89a8100c3b3ffff       14:30:00           METRÔ L1        29.0
20  89a8100c00fffff  89a8100c14bffff       14:30:00           METRÔ L1        28.9
47  89a8100c007ffff  89a8100c14fffff       14:31:00  METRÔ L1|METRÔ L3        28.7
9   89a8100c00fffff  89a8100c037ffff       14:32:00             [WALK]        27.8
18  89a8100c00fffff  89a8100c38fffff       14:32:00           METRÔ L1        27.7
```

### Around midnight and time zones

The search window may start on the day before; departure times after
midnight are then written past `24:00:00`, as R5 does:

```python
late = peabiru.arrival_travel_time_matrix(
    network, points.iloc[:1], points.iloc[:4],
    mode="TRANSIT",
    arrival_datetime=dt.datetime(2019, 5, 14, 0, 20),
    max_trip_duration=30,
    seed=42,
    n_threads=2,
)
print(late[["to_id", "departure_time", "total_time"]])
```

```text
             to_id departure_time  total_time
0  89a8100c00fffff       24:19:00         0.4
1  89a8100c007ffff       24:12:00         7.3
2  89a8100c077ffff       24:09:00        10.9
3  89a8100c03bffff       24:11:00         8.8
```

`arrival_datetime - max_trip_duration` is computed as R computes it with a
`POSIXct`: on the absolute time for a timezone-aware datetime (so a
daylight-saving change in between is accounted for) and then read in the
datetime's own zone; on the clock time for a naive datetime.
