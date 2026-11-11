# Fares

peabiru computes transit fares as r5r 2.5.0 does: a *fare structure*
describes how much each ride costs and which transfers are discounted, and
the routing functions take it as `fare_structure`. With one, public
transport is routed with R5's multi-criteria router (McRAPTOR), which keeps
at each stop the trips that are faster or cheaper than the others, and the
results are r5r's, number for number.

The examples use the sample data of Porto Alegre from r5r 2.5.0, with the
fare structure r5r ships for it, downloaded once into the temporary
directory (about 2 MB); its GTFS feeds only have service in the afternoon,
hence the 14:00 departure.

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
points = pd.read_csv(poa / "poa_hexgrid.csv")
departure = dt.datetime(2019, 5, 13, 14, 0)
```

## The fare structure

[`setup_fare_structure()`][peabiru.setup_fare_structure] builds one where
every route, fare type and transfer costs the same; edit it to follow the
fare rules of the city. [`read_fare_structure()`][peabiru.read_fare_structure]
reads one written by r5r's `write_fare_structure()` or by
[`write_fare_structure()`][peabiru.write_fare_structure]; the files are the
same, so a structure goes from one package to the other.

```python
fares = peabiru.read_fare_structure(fares_zip)
print(fares["fares_per_type"])
print(fares["fares_per_transfer"])
print(fares["max_discounted_transfers"], fares["transfer_time_allowance"], fares["fare_cap"])
```

```text
   type  unlimited_transfers  allow_same_route_transfer  use_route_fare  fare
0   BUS                False                      False           False   4.8
1  RAIL                 True                      False           False   4.5
  first_leg second_leg  fare
0       BUS        BUS  7.20
1      RAIL        BUS  8.37
2       BUS       RAIL  8.37
1 60 inf
```

A bus ride costs 4.80 and a train ride 4.50; two buses within 60 minutes
cost 7.20 together, a bus and a train 8.37; one discounted transfer per
trip; the trains have unlimited transfers between them. Every GTFS route
gets its fare type in `fares_per_route` (matched by `agency_id` and
`route_id`); a route missing from it is an error, as in r5r.

## Travel times within a budget

`max_fare` keeps, for each pair, the fastest trip that costs no more than
it:

```python
o, d = points.iloc[:20], points.iloc[:200]
args = dict(mode=["WALK", "TRANSIT"], departure_datetime=departure,
            fare_structure=fares, draws_per_minute=1, n_threads=2)
cheap = peabiru.travel_time_matrix(network, o, d, max_fare=4.8, **args)
any_fare = peabiru.travel_time_matrix(network, o, d, **args)
both = cheap.merge(any_fare, on=["from_id", "to_id"], suffixes=("_4.8", "_any"))
print(both[["travel_time_p50_4.8", "travel_time_p50_any"]].mean().round(1).to_string())
```

```text
travel_time_p50_4.8    61.2
travel_time_p50_any    52.2
```

With fares, McRAPTOR samples `time_window * draws_per_minute` departure
times in the window instead of routing at every minute, as R5 does; r5r
rejects the combinations its sampler cannot meet (exactly 200 draws, or too
many for the window), and so does peabiru.

## The Pareto frontier

[`pareto_frontier()`][peabiru.pareto_frontier] gives, for each pair, the
travel time at each fare cutoff where it changes: the trade-off between
time and money.

```python
pf = peabiru.pareto_frontier(
    network, points.iloc[:1], points.iloc[:200], departure_datetime=departure,
    fare_structure=fares, fare_cutoffs=[4.5, 4.8, 8.37, 9.6], n_threads=2,
)
print(pf[pf["to_id"] == pf["to_id"].iloc[3]].to_string())
```

```text
           from_id            to_id  percentile  travel_time  monetary_cost
2  89a901291abffff  89a901295b7ffff          50           54           4.50
3  89a901291abffff  89a901295b7ffff          50           53           4.80
4  89a901291abffff  89a901295b7ffff          50           45           8.37
```

`monetary_cost` is the cutoff as R5 reads it, a Java `float`, as r5r
reports it (4.8 is 4.800000190734863).

## Alternative itineraries and their fares

With `shortest_path=False`, [`detailed_itineraries()`][peabiru.detailed_itineraries]
returns the alternatives McRAPTOR finds, one per sequence of routes; with a
fare structure, each comes with its fare (`total_fare`) and the fare paid by
the end of each leg (`cumulative_fare`):

```python
trips = peabiru.detailed_itineraries(
    network, points.iloc[[0]], points.iloc[[120]], mode=["WALK", "TRANSIT"],
    departure_datetime=departure, fare_structure=fares, shortest_path=False,
    drop_geometry=True,
)
print(trips[["option", "total_duration", "total_fare", "segment", "mode",
             "cumulative_fare", "route"]].to_string())
```

```text
    option  total_duration  total_fare  segment  mode  cumulative_fare route
0        1            64.6         4.8        1  WALK              0.0      
1        1            64.6         4.8        2   BUS              4.8   375
2        1            64.6         4.8        3  WALK              4.8      
3        2            71.3         4.8        1  WALK              0.0      
4        2            71.3         4.8        2   BUS              4.8   394
5        2            71.3         4.8        3  WALK              4.8      
6        3            49.2         7.2        1  WALK              0.0      
7        3            49.2         7.2        2   BUS              4.8    T4
8        3            49.2         7.2        3  WALK              4.8      
9        3            49.2         7.2        4   BUS              7.2    T2
10       3            49.2         7.2        5  WALK              7.2
```

Without a fare structure, `suboptimal_minutes` keeps alternatives up to
that many minutes slower than the best.
