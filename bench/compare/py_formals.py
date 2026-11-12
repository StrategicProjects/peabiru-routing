"""Repository check: the arguments of the Python functions against r5r 2.5.0's.

For every function r5r exports, the Python function of the same name has
the same arguments, in the same order, with the Python equivalents of r5r's
defaults. r5r's formals are in ``r5r-formals.csv`` (made by
``tools/r5r_formals.R`` from r5r itself). The differences are listed here
and described in ``python/docs/coming-from-r5.md``.

Run with pytest from the repository root, against the installed package::

    python -m pytest bench/compare/py_formals.py

A missing fixture is a failure, not a skip.
"""

from __future__ import annotations

import csv
import inspect
import math
from pathlib import Path

import peabiru

HERE = Path(__file__).resolve().parent
# wherever the repository keeps the fixture
CANDIDATES = [
    HERE / "r5r-formals.csv",
    HERE / "fixtures" / "r5r-formals.csv",
    HERE / "testthat" / "fixtures" / "r5r-formals.csv",
]

# r5r's functions that manage Java: nothing to manage without it
NO_JAVA = {"setup_r5", "stop_r5", "download_r5", "r5r_cache", "r5r_sitrep"}
# r5r's arguments under peabiru's names
RENAMED = {"r5r_network": "network"}
# r5r's deprecated arguments, which peabiru does not have
DEPRECATED = {"r5r_core", "sample_size"}
# arguments Python adds (after r5r's)
ADDED = {"build_network": ["n_threads"]}
# required in r5r, but after arguments with defaults: None by default in
# Python, and leaving them out is an error
REQUIRED_AS_NONE = {
    ("arrival_travel_time_matrix", "arrival_datetime"),
    ("pareto_frontier", "fare_structure"),
    ("pareto_frontier", "fare_cutoffs"),
}
# what the package exports besides the functions r5r has too
OWN = {
    "Network", "UnsupportedError", "__version__", "core_version", "sample_data_path",
    "sample_points", "trip_options", "TripOptions",
}


def _rows():
    found = [p for p in CANDIDATES if p.exists()]
    assert found, "r5r-formals.csv was not found in " + ", ".join(map(str, CANDIDATES))
    by: dict = {}
    with found[0].open(encoding="utf-8") as f:
        for r in csv.DictReader(f):
            by.setdefault(r["fun"], []).append(r)
    return by


def _r_default(fun: str, arg: str, text: str):
    """The Python value of an r5r default."""
    if fun == "street_network_bbox" and arg == "output":
        return "polygon"  # match.arg(): the first choice
    if text == "NULL":
        return None
    if text in ("TRUE", "FALSE"):
        return text == "TRUE"
    if text == "Inf":
        return math.inf
    if text.startswith("c("):
        return [_r_default(fun, arg, t.strip()) for t in text[2:-1].split(",")]
    if text.startswith('"'):
        return text.strip('"')
    return float(text.rstrip("L"))


def test_exports_are_r5rs_and_peabirus_own():
    rows = _rows()
    assert set(peabiru.__all__) - set(rows) == OWN


def test_names_order_and_defaults_are_r5rs():
    rows = _rows()
    assert set(peabiru.__all__) >= set(rows) - NO_JAVA
    compared = 0
    for fun, rr in rows.items():
        if fun in NO_JAVA:
            assert not hasattr(peabiru, fun)
            continue
        params = inspect.signature(getattr(peabiru, fun)).parameters
        rr = [r for r in rr if r["argument"] not in DEPRECATED]
        r_names = [RENAMED.get(r["argument"], r["argument"]) for r in rr]
        assert list(params) == r_names + ADDED.get(fun, []), fun
        for r, arg in zip(rr, r_names):
            compared += 1
            p = params[arg]
            if r["required"] == "TRUE":
                expected = None if (fun, arg) in REQUIRED_AS_NONE else inspect.Parameter.empty
                assert p.default is expected, (fun, arg)
                continue
            want = _r_default(fun, arg, r["default"])
            got = p.default
            if isinstance(want, list):
                assert list(got) == want, (fun, arg)
            elif isinstance(want, bool) or want is None:
                assert got is want, (fun, arg, got, want)
            else:
                assert got == want and not isinstance(got, bool), (fun, arg, got, want)
    print(f"{compared} arguments compared")
    assert compared > 200
