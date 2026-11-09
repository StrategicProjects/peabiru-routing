"""Compare two travel-time matrices in r5r's long format (from_id,to_id,travel_time_p50).

Usage: python parity.py reference.csv candidate.csv

Pairs absent from a file are unreachable there. Reports the share of pairs
reachable in both, the share within 0 and 1 minute, and the pairs only one
side reaches.
"""
import csv
import sys
from collections import Counter


def load(path):
    with open(path, newline="") as f:
        return {(r["from_id"], r["to_id"]): int(r["travel_time_p50"]) for r in csv.DictReader(f)}


def main(ref_path, cand_path):
    ref, cand = load(ref_path), load(cand_path)
    both = ref.keys() & cand.keys()
    diffs = Counter(cand[k] - ref[k] for k in both)
    n = len(both)
    exact = diffs[0]
    within1 = sum(c for d, c in diffs.items() if abs(d) <= 1)
    only_ref = len(ref.keys() - cand.keys())
    only_cand = len(cand.keys() - ref.keys())
    print(f"reference pairs {len(ref):,}  candidate pairs {len(cand):,}  both {n:,}")
    print(f"only in reference {only_ref:,}  only in candidate {only_cand:,}")
    if n:
        print(f"exact {exact / n:.4%}  within 1 min {within1 / n:.4%}")
        worst = sorted(diffs.items(), key=lambda kv: -abs(kv[0]))[:8]
        print("largest differences (candidate - reference: count):",
              ", ".join(f"{d:+d}: {c:,}" for d, c in worst))
        print("difference histogram:", ", ".join(f"{d:+d}: {c:,}" for d, c in sorted(diffs.items()) if abs(d) <= 3))


if __name__ == "__main__":
    main(*sys.argv[1:3])
