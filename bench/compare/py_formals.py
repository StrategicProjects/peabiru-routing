"""The arguments of every function are r5r 2.5.0's: the same names, in the
same order, with the Python equivalents of r5r's defaults.

r5r's formals are in ``r/tests/testthat/fixtures/r5r-formals.csv`` (made by
``tools/r5r_formals.R`` from r5r itself). The deliberate differences are
listed here, and documented in ``docs/coming-from-r5.md``.
"""

from __future__ import annotations

import csv
import inspect
import math
from pathlib import Path

import pytest

import peabiru

FORMALS = Path(__file__).resolve().parents[2] / "r" / "tests" / "testthat" / "fixtures" / "r5r-formals.csv"

# r5r's functions that manage Java: nothing to manage without it
NO_JAVA = {"setup_r5", "stop_r5", "download_r5", "r5r_cache", "r5r_sitrep"}
# arguments Python adds (after r5r's)
ADDED = {"build_network": ["n_threads"]}
# required in r5r, but after arguments with defaults: None by default in
# Python, and leaving them out is the error r5r gives
REQUIRED_AS_NONE = {
    ("arrival_travel_time_matrix", "arrival_datetime"),
    ("pareto_frontier", "fare_structure"),
    ("pareto_frontier", "fare_cutoffs"),
}


def _rows():
    if not FORMALS.exists():  # pragma: no cover - outside the repository
        pytest.skip("r5r-formals.csv is not here")
    by: dict = {}
    with FORMALS.open(encoding="utf-8") as f:
        for r in csv.DictReader(f):
            by.setdefault(r["fun"], []).append(r)
    return by


def _r_default(fun: str, arg: str, text: str):
    """The Python value of an r5r default, or a marker for the special
    ones."""
    if fun == "street_network_bbox" and arg == "output":
        return "polygon"  # match.arg(): the first choice
    if text == "deprecated()":
        return "deprecated"
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


# what the package exports besides r5r's functions: its own helpers, and
# trip_options() (peabiru's own planner, no r5r counterpart)
OWN = {
    "Network", "UnsupportedError", "__version__", "core_version", "sample_data_path",
    "sample_points", "trip_options", "TripOptions",
}


def test_exports_are_r5rs_and_peabirus_own():
    rows = _rows()
    assert set(peabiru.__all__) - set(rows) == OWN


def test_names_order_and_defaults_are_r5rs():
    rows = _rows()
    assert set(peabiru.__all__) >= set(rows) - NO_JAVA
    for fun, rr in rows.items():
        if fun in NO_JAVA:
            assert not hasattr(peabiru, fun)
            continue
        params = inspect.signature(getattr(peabiru, fun)).parameters
        r_names = [r["argument"] for r in rr if r["argument"] != "r5r_core"]
        assert list(params) == r_names + ADDED.get(fun, []), fun
        for r in rr:
            arg = r["argument"]
            if arg == "r5r_core":
                continue
            p = params[arg]
            if r["required"] == "TRUE":
                expected = None if (fun, arg) in REQUIRED_AS_NONE else inspect.Parameter.empty
                assert p.default is expected, (fun, arg)
                continue
            want = _r_default(fun, arg, r["default"])
            got = p.default
            if want == "deprecated":
                # isochrone(sample_size=): any value given warns, as in r5r
                assert got is not inspect.Parameter.empty, (fun, arg)
            elif isinstance(want, list):
                assert list(got) == want, (fun, arg)
            elif isinstance(want, bool) or want is None:
                assert got is want, (fun, arg, got, want)
            else:
                assert got == want and not isinstance(got, bool), (fun, arg, got, want)


def test_added_arguments_default_to_all_cores():
    assert inspect.signature(peabiru.build_network).parameters["n_threads"].default == math.inf
