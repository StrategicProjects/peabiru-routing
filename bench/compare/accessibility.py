"""Compare two accessibility tables in r5r's format.

Usage: python accessibility.py reference.csv candidate.csv

Files have id, opportunity, percentile, [cutoff,] accessibility, one row per
key. Values are parsed as doubles (r5r's baseline is written with %.17g and
peabiru writes the shortest round-trip form, so equal doubles compare equal).
Prints, per opportunity column and overall: rows, exact matches, max absolute
and max relative difference (with the worst row), and whether the row order
is the same.
"""
import csv
import sys


def load(path):
    with open(path, newline="") as f:
        r = csv.DictReader(f)
        keys = [c for c in r.fieldnames if c != "accessibility"]
        rows = []
        for row in r:
            rows.append((tuple(row[k] for k in keys), float(row["accessibility"])))
        return keys, rows


def main(ref_path, cand_path):
    rk, ref = load(ref_path)
    ck, cand = load(cand_path)
    if rk != ck:
        print(f"FAIL columns differ: {rk} vs {ck}")
        sys.exit(1)
    rd, cd = dict(ref), dict(cand)
    same_order = [k for k, _ in ref] == [k for k, _ in cand]
    only_ref, only_cand = len(rd.keys() - cd.keys()), len(cd.keys() - rd.keys())
    opp = rk.index("opportunity")
    groups = {}
    for k in rd.keys() & cd.keys():
        groups.setdefault(k[opp], []).append(k)
    total = [0, 0, 0.0, 0.0, None]
    lines = []
    for name in sorted(groups):
        n = exact = 0
        max_abs = max_rel = 0.0
        worst = None
        for k in groups[name]:
            a, b = rd[k], cd[k]
            n += 1
            if a == b:
                exact += 1
                continue
            d = abs(a - b)
            rel = d / max(abs(a), abs(b))
            if d > max_abs:
                max_abs, worst = d, (k, a, b)
            max_rel = max(max_rel, rel)
        lines.append(f"  {name:<12} rows {n:>6}  exact {exact:>6}  max abs {max_abs:.3g}  max rel {max_rel:.3g}"
                     + (f"  worst {worst[0]} r5r {worst[1]!r} peabiru {worst[2]!r}" if worst else ""))
        total[0] += n
        total[1] += exact
        if max_abs > total[2]:
            total[2], total[4] = max_abs, worst
        total[3] = max(total[3], max_rel)
    status = "OK" if (total[1] == total[0] == len(ref) == len(cand) and same_order) else "DIFF"
    print(f"{status} rows r5r {len(ref)} peabiru {len(cand)} compared {total[0]} exact {total[1]} "
          f"max abs {total[2]:.3g} max rel {total[3]:.3g} same order {same_order} "
          f"only r5r {only_ref} only peabiru {only_cand}")
    if status != "OK":
        print("\n".join(lines))


if __name__ == "__main__":
    main(*sys.argv[1:3])
