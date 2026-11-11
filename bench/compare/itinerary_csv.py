"""Compare two CSV files of detailed itineraries (one row per leg, the line as
WKT), as written by bench/r5r/itineraries_r5r.R (r5r), `peabiru itineraries`
(the CLI), bench/compare/itinerary_py.py and bench/compare/itinerary_js.mjs:
every field and every coordinate must be equal.

    python3 bench/compare/itinerary_csv.py <r5r.csv> <other.csv>

Exits with status 1 when they differ.
"""
import csv, sys, re
csv.field_size_limit(1 << 30)

def load(p):
    rows = list(csv.DictReader(open(p)))
    return rows

def coords(w):
    if w.endswith("EMPTY"):
        return []
    inner = w[w.index("(") + 1 : w.rindex(")")]
    return [tuple(float(v) for v in pt.split()) for pt in inner.split(", ")]

a = load(sys.argv[1])  # r5r
b = load(sys.argv[2])  # peabiru
cols = ["from_id", "to_id", "option", "departure_time", "total_duration", "total_distance", "segment",
        "mode", "segment_duration", "wait", "distance", "route"]
numeric = {"total_duration", "segment_duration", "wait"}
print(f"rows r5r {len(a)} peabiru {len(b)}")
ka = {(r["from_id"], r["to_id"], r["option"], r["segment"]): r for r in a}
kb = {(r["from_id"], r["to_id"], r["option"], r["segment"]): r for r in b}
order_ok = [ (r["from_id"], r["to_id"], r["option"], r["segment"]) for r in a] == [ (r["from_id"], r["to_id"], r["option"], r["segment"]) for r in b]
print("same row order:", order_ok)
diff_pairs = {}
ncoord = 0
for k in sorted(set(ka) | set(kb)):
    ra, rb = ka.get(k), kb.get(k)
    if ra is None or rb is None:
        diff_pairs.setdefault((k[0], k[1]), []).append("missing in " + ("r5r" if ra is None else "peabiru"))
        continue
    for c in cols:
        x, y = ra[c], rb[c]
        same = float(x) == float(y) if c in numeric else x == y
        if not same:
            diff_pairs.setdefault((k[0], k[1]), []).append(f"seg {k[3]} {c}: {x} vs {y}")
    ca, cb = coords(ra["geometry"]), coords(rb["geometry"])
    ncoord += len(ca)
    if ca != cb:
        n = sum(1 for p, q in zip(ca, cb) if p != q) + abs(len(ca) - len(cb))
        diff_pairs.setdefault((k[0], k[1]), []).append(f"seg {k[3]} geometry: {len(ca)} vs {len(cb)} pts, {n} differ")
pairs = {(r["from_id"], r["to_id"]) for r in a} | {(r["from_id"], r["to_id"]) for r in b}
print(f"pairs with rows {len(pairs)}, coordinates compared {ncoord}, pairs differing {len(diff_pairs)}")
for k, v in list(diff_pairs.items())[:15]:
    print(k, v[:4])
sys.exit(1 if diff_pairs or not order_ok or len(a) != len(b) else 0)
