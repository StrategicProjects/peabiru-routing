# Trip options

[`trip_options()`][peabiru.trip_options] is peabiru's own trip planner, with
no counterpart in r5r: for each origin and destination it gives every trip
worth considering, the ones that no other trip beats on all the chosen
criteria at once. Where [`detailed_itineraries()`][peabiru.detailed_itineraries]
answers "what is the fastest way there?", `trip_options()` answers "what
are my choices?": faster but dearer, cheaper but slower, fewer transfers,
less walking.

## Not dominated

A trip *dominates* another when it is no worse on every chosen criterion and
better on at least one. The criteria are

- `"TIME"`: door-to-door minutes from the departure minute, waits included;
- `"FARE"`: the fare by a fare structure (see [Fares](fares.md));
- `"TRANSFERS"`: rides minus one; a trip without public transport has 0, as
  many as a trip with a single ride;
- `"WALK"`: minutes on foot: the access and egress legs when they are
  walked, and the walks between stops (a bicycle or car leg counts 0).

By default all four are used with a fare structure, all but `"FARE"`
without one. At each departure minute of the window the options are the
set of trips not dominated, with one trip per combination of values.

The search is exact, to the second. What is returned is then compared at
the resolution the values are read at: total time to a tenth of a minute,
as it is reported, and walking to the whole minute (rounded half up;
`walk_resolution` sets the step in seconds, and 1 compares the exact seconds). Of the
trips of a minute, one is left out when another is no worse on every
criterion in those terms and better on one, or equal on all of them and
better in exact terms (less total time, then fewer transfers, less walking,
a lower fare). Without this, a trip that is slower, dearer and with more
transfers would be returned only because it walks a few seconds less. The
values reported stay exact. Rounding cannot make a trip look better than
one that beats it to the second, so the result is the same as comparing
every trip at that resolution.

The trips are those of r5r's network semantics (the same access and egress
searches, transfers between stops and boarding rules as
`detailed_itineraries()`), so the times agree with the other functions.

The examples use r5r's sample data of Porto Alegre and the fare structure
r5r ships for it, downloaded once into the temporary directory, as in the
[Fares](fares.md) guide.

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
for name in ["poa_osm.pbf", "poa_eptc.zip", "poa_trensurb.zip", "poa_hexgrid.csv"]:
    if not (poa / name).exists():
        urllib.request.urlretrieve(url + name, poa / name)
fares_zip = Path(tempfile.gettempdir()) / "peabiru-fares_poa.zip"
if not fares_zip.exists():
    urllib.request.urlretrieve(url + "fares/fares_poa.zip", fares_zip)

network = peabiru.build_network(poa)
fares = peabiru.read_fare_structure(fares_zip)
points = pd.read_csv(poa / "poa_hexgrid.csv")
origin = points[points["id"] == "89a90128053ffff"]
destination = points[points["id"] == "89a90129c9bffff"]
departure = dt.datetime(2019, 5, 13, 14, 0)
```

## Time and fare

With time and fare as the criteria, the options of this pair over the ten
minutes from 14:00 are a ride on the T4 bus, a faster trip with a transfer
to the T1D (two fares, with r5r's integration discount: R$ 7.20 instead of
R$ 9.60), and walking, which is free:

```python
res = peabiru.trip_options(
    network, origin, destination, departure_datetime=departure,
    criteria=["TIME", "FARE"], fare_structure=fares,
)
cols = ["option", "total_time", "fare", "transfers", "walk_time", "departure_time",
        "n_departures", "description"]
print(res.options[cols].to_string(index=False))
```

```text
 option  total_time  fare  transfers  walk_time departure_time  n_departures                     description
      1        30.8   7.2          1       16.8       14:05:00             5  WALK > BUS T4 > BUS T1D > WALK
      2        35.1   4.8          0       24.7       14:05:00             8            WALK > BUS T4 > WALK
      3        35.8   7.2          1       20.4       14:00:00             5 WALK > BUS T11 > BUS T1D > WALK
      4        42.1   4.8          0       33.8       14:07:00             2            WALK > BUS T4 > WALK
      5        57.7   0.0          0       57.7       14:00:00            10                            WALK
```

Each row is an *option*: trips with the same street modes, the same routes
between the same stops, and the same fare, transfers and minutes on foot,
found at `n_departures` of the window's minutes. Its total time changes with
the minute (the wait at the start does), so the row reports the trip of its
best minute, `departure_time`. Options 3 and 4 are on the frontier at the
minutes when options 1 and 2 are not (their buses have just left): over the
whole window an option can look dominated by one that was not available at
its minutes. `by_departure=True` adds the frontier of every minute:

```python
res = peabiru.trip_options(
    network, origin, destination, departure_datetime=departure, time_window=3,
    criteria=["TIME", "FARE"], fare_structure=fares, by_departure=True,
)
print(res.departures.to_string(index=False))
```

```text
        from_id           to_id  option departure_time arrival_time  total_time
89a90128053ffff 89a90129c9bffff       1       14:01:00     14:35:48        34.8
89a90128053ffff 89a90129c9bffff       1       14:02:00     14:35:48        33.8
89a90128053ffff 89a90129c9bffff       2       14:00:00     14:35:48        35.8
89a90128053ffff 89a90129c9bffff       3       14:00:00     14:40:03        40.1
89a90128053ffff 89a90129c9bffff       3       14:01:00     14:40:03        39.1
89a90128053ffff 89a90129c9bffff       3       14:02:00     14:40:03        38.1
89a90128053ffff 89a90129c9bffff       4       14:00:00     14:57:40        57.7
89a90128053ffff 89a90129c9bffff       4       14:01:00     14:58:40        57.7
89a90128053ffff 89a90129c9bffff       4       14:02:00     14:59:40        57.7
```

## All four criteria and the legs

With the default criteria (all four, here, since there is a fare structure)
the trade-offs with transfers and walking come in too. The legs of each
option are in `itineraries`, in the format of `detailed_itineraries()` and
keyed by `from_id`, `to_id` and `option` (a `geopandas.GeoDataFrame`, or a
`pandas.DataFrame` with `drop_geometry=True`):

```python
res = peabiru.trip_options(
    network, origin, destination, departure_datetime=departure,
    fare_structure=fares, drop_geometry=True,
)
print(len(res.options), "options")
legs = res.itineraries
print(legs[legs["option"] == 1][["segment", "mode", "cumulative_fare", "segment_duration",
                                 "wait", "distance", "route"]].to_string(index=False))
```

```text
4 options
 segment mode  cumulative_fare  segment_duration  wait  distance route
       1 WALK              0.0              10.9   0.0       694      
       2  BUS              4.8               7.4   1.2      2113    T4
       3  BUS              7.2               2.7   2.9      1392   T1D
       4 WALK              7.2               5.8   0.0       350
```

`max_options` keeps the first options of each pair, `max_fare` and
`max_trip_duration` leave out the trips beyond them, and `itineraries=False`
skips the legs.

## Fares are not additive

A fare is not the sum of its rides: a transfer within the time allowance
is discounted, some types have unlimited transfers, a cap may apply. Two
partial trips are only compared on fare when one is at least as cheap for
every way both can continue; the comparison makes no assumption about the
fare structure (a discounted transfer may even cost more than two full
fares). This exactness has a price: partial trips that could still get a
discount are kept apart until they cannot, which makes the fare criterion
the slowest one.

## Frequency-based routes

GTFS `frequencies.txt` gives a headway, not departure times. r5r draws
random departures (Monte Carlo); `trip_options()` does not: a
frequency-based vehicle is boarded at its expected time, half a headway
after the traveller is ready to board (or after the period starts), within
the period, and with `exact_times = 1` at the first scheduled departure.
The results do not change from call to call, and differ on purpose from
r5r's draws. The Sao Paulo sample shipped with the package runs by
frequency:

```python
spo = peabiru.build_network(peabiru.sample_data_path())
pts = peabiru.sample_points()
res = peabiru.trip_options(
    spo, pts.iloc[[0]], pts.iloc[[26]], departure_datetime=departure,
    time_window=5, itineraries=False,
)
print(res.options[["option", "total_time", "transfers", "walk_time", "n_departures",
                   "description"]].to_string(index=False))
```

```text
 option  total_time  transfers  walk_time  n_departures                                           description
      1        33.1          1       20.4             5       WALK > SUBWAY METRÔ L1 > SUBWAY METRÔ L3 > WALK
      2        33.5          0       24.7             5                         WALK > SUBWAY METRÔ L3 > WALK
      3        35.1          1       18.8             5           WALK > SUBWAY METRÔ L1 > BUS 2002-10 > WALK
      4        37.5          0       22.7             5                             WALK > BUS 2002-10 > WALK
      5        54.9          1       17.7             5               WALK > BUS 5290-10 > BUS 2002-10 > WALK
      6        60.0          2       15.7             5 WALK > BUS 5290-10 > BUS 5290-10 > BUS 2002-10 > WALK
```

## Compared with r5r's functions

- [`pareto_frontier()`][peabiru.pareto_frontier] (r5r's) gives, for each
  fare cutoff, the percentiles of the travel time over Monte Carlo draws of
  departure times: a summary of time against budget, without the trips.
- [`detailed_itineraries()`][peabiru.detailed_itineraries] (r5r's) gives the
  fastest trip, or with `shortest_path=False` R5's alternatives (one per
  sequence of routes, the fastest of each).
- `trip_options()` gives the trade-offs themselves, trip by trip, with their
  legs, on any of four criteria. With `criteria=["TIME"]` and
  `time_window=1`, its first option has the total time of
  `detailed_itineraries()`'s fastest trip for that minute.
