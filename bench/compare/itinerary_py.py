"""detailed_itineraries() of the Python package as CSV for itinerary_csv.py.

    python bench/compare/itinerary_py.py <dir> <prefix> <mode> [<mode> ...]

reads <prefix>_o.csv and <prefix>_d.csv (id,lat,lon, paired row by row) and
writes <prefix>_py.csv.
"""
import sys, datetime as dt, csv
import pandas as pd
import peabiru
net_dir, pre, *modes = sys.argv[1:]
o = pd.read_csv(pre + "_o.csv", dtype={"id": str})
d = pd.read_csv(pre + "_d.csv", dtype={"id": str})
net = peabiru.build_network(net_dir)
r = peabiru.detailed_itineraries(net, o, d, mode=modes, departure_datetime=dt.datetime(2019, 5, 13, 14, 0))
with open(pre + "_py.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["from_id", "to_id", "option", "departure_time", "total_duration", "total_distance", "segment",
                "mode", "segment_duration", "wait", "distance", "route", "geometry"])
    for row in r.itertuples():
        coords = ", ".join(f"{x!r} {y!r}" for x, y in row.geometry.coords)
        w.writerow([row.from_id, row.to_id, row.option, row.departure_time, repr(row.total_duration),
                    row.total_distance, row.segment, row.mode, repr(row.segment_duration), repr(row.wait),
                    row.distance, row.route, f"LINESTRING ({coords})"])
print(r.dtypes.to_dict())
