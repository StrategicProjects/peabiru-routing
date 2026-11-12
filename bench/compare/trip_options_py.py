"""The trip_options() scenarios of bench/compare/trip_options_bindings.R, with
the Python package, written as CSV.

    python bench/compare/trip_options_py.py <poa dir> <fares.zip> <poa points.csv> <spo dir> <spo points.csv> <out dir>

The points files have id,lon,lat; Porto Alegre's first 30 rows are the
origins and the next 30 the destinations, Sao Paulo's first 10 and next 10.
"""
import datetime as dt
import os
import sys

import pandas as pd

import peabiru

poa, fares_zip, poa_pts, spo, spo_pts, out = sys.argv[1:]
os.makedirs(out, exist_ok=True)
t0 = dt.datetime(2019, 5, 13, 14, 0)
M = ["WALK", "TRANSIT"]


def save(name, r):
    r.options.to_csv(os.path.join(out, f"{name}_options.csv"), index=False)
    if r.itineraries is not None:
        r.itineraries.to_csv(os.path.join(out, f"{name}_legs.csv"), index=False)
    if r.departures is not None:
        r.departures.to_csv(os.path.join(out, f"{name}_departures.csv"), index=False)


net = peabiru.build_network(poa)
fs = peabiru.read_fare_structure(fares_zip)
pts = pd.read_csv(poa_pts, dtype={"id": str})
o, d = pts.iloc[:30], pts.iloc[30:60]
save("poa_fares", peabiru.trip_options(net, o, d, mode=M, departure_datetime=t0, time_window=5,
                                       fare_structure=fs, by_departure=True, drop_geometry=True))
save("poa_bike", peabiru.trip_options(net, o, d, mode=["BICYCLE", "TRANSIT"], departure_datetime=t0,
                                      time_window=3, criteria=["TIME", "TRANSFERS"], max_rides=2,
                                      drop_geometry=True))
save("poa_cheap", peabiru.trip_options(net, o, d, mode=M, departure_datetime=t0, time_window=2,
                                       criteria=["FARE", "WALK"], fare_structure=fs, max_fare=7.2,
                                       itineraries=False, by_departure=True))
net = peabiru.build_network(spo)
pts = pd.read_csv(spo_pts, dtype={"id": str})
save("spo", peabiru.trip_options(net, pts.iloc[:10], pts.iloc[10:20], mode=M, departure_datetime=t0,
                                 time_window=5, by_departure=True, drop_geometry=True))
