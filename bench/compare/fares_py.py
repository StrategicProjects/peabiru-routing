"""The fare scenarios of bench/compare/fares_bindings.R, with the Python
package, written as CSV.

    python bench/compare/fares_py.py <network dir> <fares.zip> <points.csv> <out dir>

<points.csv> has id,lon,lat (and jobs); the first 15 rows are the origins.
"""
import datetime as dt
import os
import sys

import pandas as pd

import peabiru

net_dir, fares_zip, points_csv, out = sys.argv[1:]
os.makedirs(out, exist_ok=True)
net = peabiru.build_network(net_dir)
fs = peabiru.read_fare_structure(fares_zip)
pts = pd.read_csv(points_csv, dtype={"id": str})
o, d = pts.iloc[:15], pts
t0 = dt.datetime(2019, 5, 13, 14, 0)
M = ["WALK", "TRANSIT"]

peabiru.travel_time_matrix(net, o, d, mode=M, departure_datetime=t0, fare_structure=fs, max_fare=4.8,
                           draws_per_minute=2, percentiles=[10, 50, 90]).to_csv(os.path.join(out, "ttm.csv"), index=False)
peabiru.accessibility(net, o, d, opportunities_colnames="jobs", mode=M, departure_datetime=t0, fare_structure=fs,
                      max_fare=7.2, cutoffs=[20, 45], draws_per_minute=1).to_csv(os.path.join(out, "access.csv"), index=False)
peabiru.pareto_frontier(net, o, d, mode=M, departure_datetime=t0, fare_structure=fs,
                        fare_cutoffs=[4.5, 4.8, 9, 15]).to_csv(os.path.join(out, "pareto.csv"), index=False)
peabiru.detailed_itineraries(net, o, d.iloc[15:30], mode=M, departure_datetime=t0, fare_structure=fs,
                             shortest_path=False, drop_geometry=True).to_csv(os.path.join(out, "itineraries.csv"), index=False)
peabiru.write_fare_structure(peabiru.setup_fare_structure(net, 4.8, by="AGENCY_NAME"), os.path.join(out, "setup.zip"))
