"""Repository check: the Python package's argument errors against the cases
of bench/compare/argument_errors.json.

Every case of the file is called, and the error compared with R's message
turned into Python's terms by the file's "python" rules, unless the case is
listed there as worded differently on purpose. The file is written by
bench/compare/argument_errors.R from the R package, whose messages
bench/compare/error_parity.R compares with r5r's.

Run with pytest from the repository root, against the installed package::

    python -m pytest bench/compare/py_argument_errors.py

A missing file is a failure, not a skip.
"""

import datetime as dt
import json
import math
import os
import re
import warnings

import pytest

import peabiru

CASES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "argument_errors.json")

# R's values; its numbers are doubles (-1 is a float in R)
VALUES = dict(NULL_=None, chr="a", chr2=["a", "b"], neg=-1.0, zero=0.0, frac=1.5, big=1e10,
              inf=math.inf, nan=math.nan, lgl=True, num2=[1.0, 2.0], empty=[])


def load():
    with open(CASES, encoding="utf-8") as f:
        return json.load(f)


def python_text(spec, msg):
    for pattern, repl in spec["rules"]["python"]:
        msg = re.sub(pattern, repl.replace("$1", r"\1"), msg)
    return msg


@pytest.fixture(scope="module")
def network():
    return peabiru.build_network(peabiru.sample_data_path(), n_threads=2)


@pytest.fixture
def calls(network):
    pts = peabiru.sample_points().iloc[:3]
    d = dt.datetime(2019, 5, 13, 14, 0)
    fs = peabiru.setup_fare_structure(network, 4.4)
    transit = dict(origins=pts, destinations=pts, mode=["WALK", "TRANSIT"],
                   departure_datetime=d, max_trip_duration=30)
    return dict(
        travel_time_matrix=transit,
        expanded_travel_time_matrix=transit,
        arrival_travel_time_matrix=dict(origins=pts, destinations=pts, mode=["WALK", "TRANSIT"],
                                        arrival_datetime=d, max_trip_duration=30),
        accessibility=dict(origins=pts, destinations=pts, opportunities_colnames="jobs",
                           mode="WALK", departure_datetime=d, cutoffs=20, max_trip_duration=30),
        detailed_itineraries=dict(origins=pts, destinations=pts, mode="WALK",
                                  departure_datetime=d, max_trip_duration=30),
        isochrone=dict(origins=pts.iloc[:1], mode="WALK", cutoffs=10, departure_datetime=d),
        find_snap=dict(points=pts),
        check_transit_availability=dict(dates="2019-05-13"),
        setup_fare_structure=dict(base_fare=4.8),
        pareto_frontier=dict(transit, fare_structure=fs, fare_cutoffs=[0, 4.8, 9.6]),
    )


def test_messages(network, calls):
    spec = load()
    deliberate = spec["deliberate"]["python"]
    wrong = []
    compared = 0
    for c in spec["cases"]:
        key = f"{c['fun']}:{c['arg']}:{c['value']}"
        if any(re.fullmatch(k, key) for k in deliberate):
            continue
        compared += 1
        args = dict(calls[c["fun"]])
        args[c["arg"]] = VALUES[c["value"]]
        expected = python_text(spec, c["message"])
        try:
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                getattr(peabiru, c["fun"])(network, **args)
            got = "(no error)"
        except (ValueError, TypeError, NotImplementedError) as e:
            got = str(e)
        except Exception as e:  # noqa: BLE001 - reported as a difference
            got = f"{type(e).__name__}: {e}"
        if got != expected:
            wrong.append(f"{key}\n  expected: {expected}\n  got:      {got}")
    out = os.environ.get("PEABIRU_ARGUMENT_ERRORS_OUT")
    if out:
        with open(out, "w", encoding="utf-8") as f:
            f.write("\n".join(wrong))
    print(f"{compared} of {len(spec['cases'])} cases compared, {len(wrong)} differ")
    assert not wrong, f"{len(wrong)} of {len(spec['cases'])} differ:\n" + "\n".join(wrong[:40])


def test_deliberate_cases_still_exist():
    spec = load()
    keys = [f"{c['fun']}:{c['arg']}:{c['value']}" for c in spec["cases"]]
    for k in spec["deliberate"]["python"]:
        assert any(re.fullmatch(k, key) for key in keys), k
