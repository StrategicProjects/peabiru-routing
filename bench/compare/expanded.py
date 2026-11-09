"""Compare two expanded / arrival travel-time matrices in r5r's CSV format.

Usage: python expanded.py reference.csv candidate.csv [max_examples]

Rows are keyed by (from_id, to_id, departure_time, draw_number), except for
arrival matrices (at most one row per pair), which are keyed by
(from_id, to_id) so that departure_time and draw_number are compared as
columns. Every other column present in both files is compared exactly:
numbers as numbers (so 25 == 25.0), empty or NA as NA. Also reports whether
the rows appear in the same order. Exits 1 unless everything is equal.
"""
import csv
import sys


def load(path):
    with open(path, newline="") as f:
        r = csv.DictReader(f)
        return r.fieldnames, list(r)


def norm(v):
    if v is None or v in ("", "NA"):
        return None
    try:
        return float(v)
    except ValueError:
        return v


def main(ref_path, cand_path, max_examples=5):
    rcols, ref = load(ref_path)
    ccols, cand = load(cand_path)
    pair_keyed = len({(r["from_id"], r["to_id"]) for r in ref}) == len(ref)
    key_cols = ["from_id", "to_id"] if pair_keyed else ["from_id", "to_id", "departure_time", "draw_number"]
    key = lambda row: tuple(row[c] for c in key_cols)
    rk = {key(r): r for r in ref}
    ck = {key(r): r for r in cand}
    if len(rk) != len(ref) or len(ck) != len(cand):
        print(f"duplicate keys: reference {len(ref) - len(rk)}, candidate {len(cand) - len(ck)}")
    both = rk.keys() & ck.keys()
    print(f"keyed by {', '.join(key_cols)}")
    print(f"reference rows {len(ref):,}  candidate rows {len(cand):,}  both {len(both):,}")
    only_r = sorted(rk.keys() - ck.keys())
    only_c = sorted(ck.keys() - rk.keys())
    print(f"only in reference {len(only_r):,}  only in candidate {len(only_c):,}")
    for k in only_r[:max_examples]:
        print("  only ref:", dict(rk[k]))
    for k in only_c[:max_examples]:
        print("  only cand:", dict(ck[k]))
    cols = [c for c in rcols if c in ccols and c not in key_cols]
    missing = [c for c in rcols if c not in ccols]
    if missing:
        print(f"columns missing in candidate: {missing}")
    all_ok = not only_r and not only_c and len(rk) == len(ref) and len(ck) == len(cand)
    bad_rows = set()
    for c in cols:
        bad = [k for k in both if norm(rk[k][c]) != norm(ck[k][c])]
        bad_rows.update(bad)
        n = len(both)
        share = 100.0 * (n - len(bad)) / n if n else 100.0
        print(f"{c:14s} equal {n - len(bad):>12,} / {n:,} ({share:.4f}%)")
        all_ok &= not bad
    print(f"rows equal in every column {len(both) - len(bad_rows):,} / {len(both):,}")
    for k in sorted(bad_rows)[:max_examples]:
        diff = {c: (rk[k][c], ck[k][c]) for c in cols if norm(rk[k][c]) != norm(ck[k][c])}
        print("  diff", k, diff)
    same_order = [key(r) for r in ref] == [key(r) for r in cand]
    print(f"same row order: {same_order}")
    all_ok &= same_order
    print("RESULT:", "IDENTICAL" if all_ok else "DIFFERENT")
    return 0 if all_ok else 1


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    sys.exit(main(sys.argv[1], sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 5))
