"""Compare two street networks edge by edge: r5r::street_network_to_sf() edges
(bench/r5r/edges_r5r.R) against `peabiru dump-edges`.

Usage: python edges.py r5r_edges.csv peabiru_edges.csv

Edges are matched by OSM way and end coordinates (1e-6 degrees). Reports edges
only one side has (different splitting) and attribute differences.
"""
import csv
import sys
from collections import Counter
def load(p):
    out = {}
    with open(p) as f:
        for r in csv.DictReader(f):
            k = (r["osm_id"], round(float(r["lon0"]), 6), round(float(r["lat0"]), 6), round(float(r["lon1"]), 6), round(float(r["lat1"]), 6))
            out.setdefault(k, []).append(r)
    return out
r5, pb = load(sys.argv[1]), load(sys.argv[2])
print("keys r5", len(r5), "peabiru", len(pb))
only_r5 = [k for k in r5 if k not in pb]; only_pb = [k for k in pb if k not in r5]
print("only in r5", len(only_r5), only_r5[:6]); print("only in peabiru", len(only_pb), only_pb[:6])
diff = Counter(); ex = {}
for k in r5.keys() & pb.keys():
    a, b = r5[k][0], pb[k][0]
    for col in ["walk", "car", "bicycle", "bicycle_lts"]:
        va, vb = a[col].upper(), b[col].upper()
        if va != vb:
            diff[(col, va, vb)] += 1; ex.setdefault((col, va, vb), (k, a["edge_index"], b["edge_index"]))
    if abs(float(a["length"]) - float(b["length"])) > 0.0015: diff[("length",)] += 1; ex.setdefault(("length",), (k, a["length"], b["length"]))
    if abs(float(a["car_speed"]) - float(b["car_speed"])) > 0.02: diff[("speed",)] += 1; ex.setdefault(("speed",), (k, a["car_speed"], b["car_speed"]))
for d, n in diff.most_common(): print(d, n, ex[d])
