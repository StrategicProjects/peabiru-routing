"""Compare two travel-time matrices in r5r's long format.

Usage: python parity.py reference.csv candidate.csv

Files have from_id,to_id and one or more travel_time_pXX columns; every
percentile column present in both is compared. Pairs absent from a file are
unreachable there; an empty cell is NA. Reports reachability differences and
the share of pairs within 0 and 1 minute, per column. Exits 1 unless both
files have the same pairs and every compared column is equal on every pair
(NA included).
"""
import csv
import sys
from collections import Counter


def load(path):
    with open(path, newline="") as f:
        r = csv.DictReader(f)
        cols = [c for c in r.fieldnames if c.startswith("travel_time_p")]
        data = {}
        for row in r:
            data[(row["from_id"], row["to_id"])] = {c: (int(row[c]) if row[c] not in ("", "NA") else None) for c in cols}
        return cols, data


def main(ref_path, cand_path):
    rc, ref = load(ref_path)
    cc, cand = load(cand_path)
    cols = [c for c in rc if c in cc]
    both = ref.keys() & cand.keys()
    ok = bool(cols) and ref.keys() == cand.keys()
    print(f"reference pairs {len(ref):,}  candidate pairs {len(cand):,}  both {len(both):,}")
    print(f"only in reference {len(ref.keys() - cand.keys()):,}  only in candidate {len(cand.keys() - ref.keys()):,}")
    for c in cols:
        diffs = Counter()
        na = 0
        for k in both:
            a, b = ref[k][c], cand[k][c]
            if a is None or b is None:
                na += a is not b
                continue
            diffs[b - a] += 1
        n = sum(diffs.values())
        ok &= na == 0 and diffs[0] == n
        if not n:
            continue
        within1 = sum(v for d, v in diffs.items() if abs(d) <= 1)
        worst = sorted(diffs.items(), key=lambda kv: -abs(kv[0]))[:6]
        print(f"{c}: exact {diffs[0] / n:.4%}  within 1 min {within1 / n:.4%}  NA mismatches {na}  "
              f"largest: " + ", ".join(f"{d:+d}: {v:,}" for d, v in worst))
    print("RESULT:", "IDENTICAL" if ok else "DIFFERENT")
    return 0 if ok else 1


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    sys.exit(main(*sys.argv[1:3]))
