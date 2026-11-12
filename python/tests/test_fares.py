"""Fare structures, fares in the routing functions and pareto_frontier().
Parity with r5r is checked by bench/compare/fares_parity.R and
bench/compare/fares_py.py."""

import math
import os

import pandas as pd
import pytest

import peabiru


def test_setup_fare_structure(network):
    fs = peabiru.setup_fare_structure(network, 4.4)
    assert list(fs) == [
        "max_discounted_transfers", "transfer_time_allowance", "fare_cap",
        "fares_per_type", "fares_per_transfer", "fares_per_route", "debug_settings",
    ]
    assert fs["max_discounted_transfers"] == 1
    assert fs["transfer_time_allowance"] == 120
    assert math.isinf(fs["fare_cap"])
    types = fs["fares_per_type"]["type"].tolist()
    assert sorted(types) == sorted(set(fs["fares_per_route"]["fare_type"]))
    assert (fs["fares_per_type"]["fare"] == 4.4).all()
    assert len(fs["fares_per_transfer"]) == len(types) ** 2
    assert fs["debug_settings"] == {"output_file": "", "trip_info": "MODE"}
    g = peabiru.setup_fare_structure(network, 2, by="generic", debug_path="x.csv")
    assert g["fares_per_type"]["type"].tolist() == ["GENERIC"]
    assert g["debug_settings"] == {"output_file": "x.csv", "trip_info": "ROUTE"}
    with pytest.raises(ValueError, match=">= 0"):
        peabiru.setup_fare_structure(network, -1)
    with pytest.raises(ValueError, match="subset"):
        peabiru.setup_fare_structure(network, 1, by="ROUTE")


def test_write_and_read_round_trip(network, tmp_path):
    fs = peabiru.setup_fare_structure(network, 4.4, by="AGENCY_NAME")
    fs["fares_per_route"].loc[0, "route_long_name"] = 'a, b "c"'
    fs["fare_cap"] = 7.5
    path = os.path.join(tmp_path, "fares.zip")
    assert peabiru.write_fare_structure(fs, path) == path
    back = peabiru.read_fare_structure(path)
    for k in ("fares_per_type", "fares_per_transfer", "fares_per_route"):
        pd.testing.assert_frame_equal(back[k], fs[k])
    assert back["fare_cap"] == 7.5
    assert back["debug_settings"] == fs["debug_settings"]
    with pytest.raises(ValueError, match="zip"):
        peabiru.write_fare_structure(fs, os.path.join(tmp_path, "fares.csv"))


def test_structures_are_checked(network, points, departure):
    fs = peabiru.setup_fare_structure(network, 4.4)

    def ttm(f, **kw):
        return peabiru.travel_time_matrix(
            network, points, points, mode=["WALK", "TRANSIT"], departure_datetime=departure,
            fare_structure=f, seed=1, n_threads=2, **kw,
        )

    missing = dict(fs)
    missing["fares_per_route"] = fs["fares_per_route"].iloc[1:]
    with pytest.raises(ValueError, match="no row for agency_id"):
        ttm(missing)
    with pytest.raises(peabiru.UnsupportedError):
        ttm({"type": "boston"})
    extra = dict(fs)
    extra["fares_per_route"] = fs["fares_per_route"].assign(note="x")
    with pytest.raises(ValueError, match="cannot read"):
        ttm(extra)
    with pytest.raises(ValueError, match="200 Monte Carlo draws"):
        ttm(fs, time_window=40)


def test_fares_limit_trips(network, points, departure):
    fs = peabiru.setup_fare_structure(network, 4.4)

    def run(**kw):
        return peabiru.travel_time_matrix(
            network, points, points, mode=["WALK", "TRANSIT"], departure_datetime=departure,
            seed=42, n_threads=2, draws_per_minute=1, fare_structure=fs, **kw,
        )

    free, none = run(), run(max_fare=0)
    m = free.merge(none, on=["from_id", "to_id"], how="left")
    ok = m["travel_time_p50_y"].isna() | (m["travel_time_p50_x"] <= m["travel_time_p50_y"])
    assert ok.all()


def test_pareto_frontier(network, points, departure):
    fs = peabiru.setup_fare_structure(network, 4.4)
    pf = peabiru.pareto_frontier(
        network, points.iloc[:3], points, departure_datetime=departure,
        fare_structure=fs, fare_cutoffs=[0, 4.4, 8.8], seed=42, n_threads=2,
    )
    assert list(pf.columns) == ["from_id", "to_id", "percentile", "travel_time", "monetary_cost"]
    assert len(pf) > 0
    for _, g in pf.groupby(["from_id", "to_id"]):
        assert g["travel_time"].is_unique
    with pytest.raises(ValueError, match="required"):
        peabiru.pareto_frontier(network, points, points, departure_datetime=departure, fare_cutoffs=[1])
    with pytest.raises(ValueError, match="duplicated"):
        peabiru.pareto_frontier(network, points, points, departure_datetime=departure,
                                fare_structure=fs, fare_cutoffs=[1, 1])


def test_itineraries_with_fares(network, points, departure):
    fs = peabiru.setup_fare_structure(network, 4.4)
    di = peabiru.detailed_itineraries(
        network, points.iloc[:3], points.iloc[3:6], mode="WALK", departure_datetime=departure,
        fare_structure=fs, drop_geometry=True, n_threads=2,
    )
    assert "total_fare" in di.columns and "cumulative_fare" in di.columns
    assert (di["total_fare"] == 0).all()
