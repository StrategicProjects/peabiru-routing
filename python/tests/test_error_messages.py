"""Errors raised by the engine name the function's arguments, not the
engine's parameters, and print numbers in short form."""

import inspect
import math
import os

import numpy as np
import pandas as pd
import pytest

import peabiru
from peabiru._checks import user_message

TRANSIT = ["WALK", "TRANSIT"]


def test_speeds_are_named_as_the_arguments(network, points, departure):
    with pytest.raises(ValueError) as e:
        peabiru.travel_time_matrix(network, points, points, mode="WALK", walk_speed=0.0012345678901234)
    msg = str(e.value)
    assert msg.startswith("walk_speed must be at least")
    assert "walk_speed_kmh" not in msg
    assert msg.endswith("got 0.00123457")

    with pytest.raises(ValueError, match=r"^bike_speed must be at least .* got 0\.000333333$"):
        peabiru.travel_time_matrix(network, points, points, mode="BICYCLE", bike_speed=1 / 3000)

    with pytest.raises(ValueError, match=r"^walk_speed must be at least"):
        peabiru.accessibility(
            network, points, points, opportunities_colnames="jobs", mode=TRANSIT,
            departure_datetime=departure, cutoffs=30, walk_speed=1 / 3000,
        )
    with pytest.raises(ValueError, match=r"^walk_speed must be at least"):
        peabiru.expanded_travel_time_matrix(
            network, points, points, mode=TRANSIT, departure_datetime=departure,
            walk_speed=1 / 3000,
        )


def test_destination_limit_names_the_function(network, points, departure):
    n = 5001
    many = pd.DataFrame(
        {"id": [str(i) for i in range(n)], "lon": np.full(n, -46.63), "lat": np.full(n, -23.556)}
    )
    with pytest.raises(
        ValueError,
        match=r"^expanded_travel_time_matrix\(\) supports at most 5000 destinations, got 5001$",
    ):
        peabiru.expanded_travel_time_matrix(
            network, points.iloc[:1], many, mode=TRANSIT, departure_datetime=departure
        )
    with pytest.raises(
        ValueError,
        match=r"^arrival_travel_time_matrix\(\) supports at most 5000 destinations, got 5001$",
    ):
        peabiru.arrival_travel_time_matrix(
            network, points.iloc[:1], many, mode=TRANSIT, arrival_datetime=departure
        )


def test_arrival_window_is_max_trip_duration(network, points, departure):
    # the arrival matrix searches over a window of max_trip_duration
    # minutes; it has no time_window argument to name
    with pytest.raises(ValueError) as e:
        peabiru.arrival_travel_time_matrix(
            network, points, points, mode=TRANSIT, arrival_datetime=departure,
            max_trip_duration=120, draws_per_minute=2**31 - 1,
        )
    msg = str(e.value)
    assert msg.startswith("max_trip_duration x draws_per_minute (120 x 2147483647")
    assert "time_window" not in msg


def test_user_message():
    # the engine's parameters have the arguments' names
    assert user_message("max_trip_duration must be from 1 to 35791394 minutes, got 0", "f") == (
        "max_trip_duration must be from 1 to 35791394 minutes, got 0"
    )
    # whole words only, numbers that are already short are kept
    assert user_message("my_time_window is 0.0036 or 120", "f", {"time_window": "x"}) == (
        "my_time_window is 0.0036 or 120"
    )
    assert user_message("got 1e300", "f") == "got 1e+300"
    assert user_message("got 3.333333333333333e-4", "f") == "got 0.000333333"
    assert user_message("time_window must be at least 1", "f", {"time_window": "max_trip_duration"}) == (
        "max_trip_duration must be at least 1"
    )


def test_checks_before_routing(network, departure, tmp_path):
    pts = peabiru.sample_points().iloc[:3].copy()
    pts["id"] = pts["id"].astype(object)
    pts.loc[1, "id"] = None
    for f in (peabiru.travel_time_matrix, peabiru.expanded_travel_time_matrix):
        with pytest.raises(ValueError, match=r"'destinations\$id' cannot be missing"):
            f(network, pts.iloc[:1], pts, mode=TRANSIT, departure_datetime=departure)
    with pytest.raises(ValueError, match="'origins' has no rows"):
        peabiru.isochrone(network, pts.iloc[:0], mode="WALK", cutoffs=10,
                          departure_datetime=departure)
    path = os.path.join(tmp_path, "never.zip")
    with pytest.raises((TypeError, ValueError), match="fare_structure"):
        peabiru.write_fare_structure({"fare_cap": 1}, path)
    assert not os.path.exists(path)


def test_engine_refusals_are_unsupported_errors():
    from peabiru._checks import core_errors

    with pytest.raises(peabiru.UnsupportedError, match="^not supported: x"):
        with core_errors("f"):
            raise ValueError("not supported: x")
    with pytest.raises(ValueError, match="^other$"):
        with core_errors("f"):
            raise ValueError("other")


def test_the_network_is_the_first_argument_and_is_checked(points):
    takes_network = [
        "travel_time_matrix", "accessibility", "expanded_travel_time_matrix",
        "arrival_travel_time_matrix", "isochrone", "detailed_itineraries", "trip_options",
        "pareto_frontier", "setup_fare_structure", "find_snap", "street_network_bbox",
        "street_network_to_sf", "transit_network_to_sf", "check_transit_availability",
        "get_gtfs_errors",
    ]
    for name in takes_network:
        assert next(iter(inspect.signature(getattr(peabiru, name)).parameters)) == "network", name
    with pytest.raises(TypeError, match="^Assertion on 'network' failed: Must be a 'peabiru.Network'"):
        peabiru.travel_time_matrix("not a network", points, points, mode="WALK")


def test_build_network_uses_all_cores_by_default():
    assert inspect.signature(peabiru.build_network).parameters["n_threads"].default == math.inf
