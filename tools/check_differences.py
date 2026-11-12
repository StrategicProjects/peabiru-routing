#!/usr/bin/env python3
"""Check that the three "coming from r5r" pages list the same differences.

The deliberate differences between peabiru and r5r 2.5.0 are listed once per
package, in the terms of each language:

- R:          r/vignettes/articles/coming-from-r5r.Rmd
- Python:     python/docs/coming-from-r5.md
- JavaScript: site/js/src/differences.md

Each item is preceded by a marker, ``<!-- diff:<id> -->``. This script checks
that every page has the markers of every difference that applies to its
language (ITEMS below), and no others, so that a difference added to or
removed from one page is not forgotten in the others. Run from the
repository's root; exit status 1 on any mismatch.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

PAGES = {
    "R": Path("r/vignettes/articles/coming-from-r5r.Rmd"),
    "Python": Path("python/docs/coming-from-r5.md"),
    "JavaScript": Path("site/js/src/differences.md"),
}

ALL = frozenset(PAGES)

# id -> the languages whose page must list it
ITEMS: dict[str, frozenset[str]] = {
    # general
    "no-java": ALL,
    "result-type": ALL,
    "network-in-memory": ALL,
    "n-threads-build": ALL,
    "verbose-progress": ALL,
    "error-classes": ALL,
    "deprecated-args": ALL,
    # R reprojects sf inputs; the Python and JavaScript pages say "R only"
    "sf-any-crs": ALL,
    # where peabiru does not depend on the run or the machine
    "gtfs-errors-order": ALL,
    "elevation-strictmath": ALL,
    "street-expanded": ALL,
    "platform-order": ALL,
    # warnings peabiru adds
    "elevation-wrapped": ALL,
    # errors where r5r fails later or routes on wrong values
    "errors-instead-of-failures": ALL,
    "scenario-wrap": ALL,
    # refusals
    "refuse-fare-calculators": ALL,
    "refuse-elevation-formats": ALL,
    "refuse-multiple-pbf": ALL,
    "refuse-pareto-output-dir": frozenset({"R", "Python"}),
    "car-egress-history": ALL,
    # files and geometries
    "write-fare-zip": ALL,
    "isochrone-make-valid": frozenset({"Python", "JavaScript"}),
}

MARKER = re.compile(r"<!--\s*diff:([a-z0-9-]+)\s*-->")


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    bad = False
    for lang, rel in PAGES.items():
        path = root / rel
        found = MARKER.findall(path.read_text(encoding="utf-8"))
        want = {i for i, langs in ITEMS.items() if lang in langs}
        dups = sorted({i for i in found if found.count(i) > 1})
        unknown = sorted(set(found) - set(ITEMS))
        missing = sorted(want - set(found))
        extra = sorted(set(found) & set(ITEMS) - want)
        for what, ids in (("missing", missing), ("not for this language", extra),
                          ("unknown", unknown), ("repeated", dups)):
            if ids:
                bad = True
                print(f"{rel}: {what}: {', '.join(ids)}")
        if not (missing or extra or unknown or dups):
            print(f"{rel}: {len(found)} differences, as expected")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
