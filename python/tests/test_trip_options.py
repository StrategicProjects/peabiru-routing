"""trip_options(): the multi-criteria planner.
The core checks the search against an exhaustive enumeration; these check
the Python side: columns, types, the merge over the window, the legs, the
limits and the argument checks."""

import numpy as np
import pandas as pd
import pytest

import peabiru

OPTION_COLS = [
    "from_id", "to_id", "option", "total_time", "fare", "transfers", "walk_time",
    "departure_time", "arrival_time", "n_departures", "first_departure",
    "last_departure", "description",
]


@pytest.fixture(scope="module")
def fares(network):
    f = peabiru.setup_fare_structure(network, base_fare=4.4)
    f["transfer_time_allowance"] = 60
    f["fares_per_transfer"]["fare"] = 6.0
    return f


def test_columns_types_and_merge(network, points, departure, fares):
    r = peabiru.trip_options(
        network, points.iloc[:4], points.iloc[4:8], departure_datetime=departure,
        time_window=5, fare_structure=fares, itineraries=False, by_departure=True,
        n_threads=2,
    )
    assert isinstance(r, peabiru.TripOptions)
    options, legs, deps = r
    assert legs is None
    assert list(options.columns) == OPTION_COLS
    for c in ["option", "transfers", "n_departures"]:
        assert options[c].dtype == np.int32
    assert len(options) >= 4
    for _, g in options.groupby(["from_id", "to_id"]):
        assert list(g["option"]) == list(range(1, len(g) + 1))
        assert g["total_time"].is_monotonic_increasing
    assert ((options["n_departures"] >= 1) & (options["n_departures"] <= 5)).all()
    assert list(deps.columns) == [
        "from_id", "to_id", "option", "departure_time", "arrival_time", "total_time",
    ]
    n = deps.groupby(["from_id", "to_id", "option"]).size()
    want = options.set_index(["from_id", "to_id", "option"])["n_departures"]
    assert (n.loc[want.index].to_numpy() == want.to_numpy()).all()


def test_no_option_of_a_minute_dominates_another(network, points, departure, fares):
    r = peabiru.trip_options(
        network, points.iloc[[0, 2, 8]], points.iloc[[19, 24, 26]],
        departure_datetime=departure, time_window=3, fare_structure=fares,
        itineraries=False, by_departure=True, n_threads=2,
    )
    v = r.departures.merge(
        r.options[["from_id", "to_id", "option", "fare", "transfers", "walk_time"]],
        on=["from_id", "to_id", "option"],
    )
    # minutes have one decimal place: a dominates b for sure only where its
    # rounded minutes are smaller (fares and transfers are exact)
    for _, g in v.groupby(["from_id", "to_id", "departure_time"]):
        m = g[["total_time", "walk_time", "fare", "transfers"]].to_numpy()
        for a in m:
            for b in m:
                sure = (a[:2] < b[:2]).all() and (a[2:] <= b[2:]).all()
                assert not sure


def test_legs_add_up(network, points, departure, fares):
    gpd = pytest.importorskip("geopandas")
    r = peabiru.trip_options(
        network, points.iloc[1:4], points.iloc[9:12], departure_datetime=departure,
        time_window=2, fare_structure=fares, n_threads=2,
    )
    legs = r.itineraries
    assert isinstance(legs, gpd.GeoDataFrame)
    assert legs.crs == "EPSG:4326"
    assert list(legs.columns) == [
        "from_id", "to_id", "option", "segment", "mode", "cumulative_fare",
        "segment_duration", "wait", "distance", "route", "geometry",
    ]
    key = ["from_id", "to_id", "option"]
    last = legs.groupby(key)["cumulative_fare"].last()
    o = r.options.set_index(key)
    assert np.allclose(last.loc[o.index].to_numpy(), o["fare"].to_numpy())
    rides = legs.assign(r=legs["route"] != "").groupby(key)["r"].sum()
    assert (np.maximum(rides.loc[o.index].to_numpy() - 1, 0) == o["transfers"].to_numpy()).all()
    df = peabiru.trip_options(
        network, points.iloc[1:4], points.iloc[9:12], departure_datetime=departure,
        time_window=2, fare_structure=fares, drop_geometry=True, n_threads=2,
    )
    assert type(df.itineraries) is pd.DataFrame
    pd.testing.assert_frame_equal(df.itineraries, pd.DataFrame(legs.drop(columns="geometry")))
    pd.testing.assert_frame_equal(df.options, r.options)


def test_criteria_limits_and_threads(network, points, departure, fares):
    def call(**kw):
        return peabiru.trip_options(
            network, points.iloc[:3], points.iloc[6:9], departure_datetime=departure,
            time_window=3, itineraries=False, **kw,
        ).options

    a = call(n_threads=1)
    assert "fare" not in a.columns
    pd.testing.assert_frame_equal(call(n_threads=3), a)
    t = call(criteria=["TIME"])
    fastest = lambda x: x.groupby(["from_id", "to_id"])["total_time"].min()  # noqa: E731
    pd.testing.assert_series_equal(fastest(t), fastest(a))
    assert (call(max_trip_duration=40)["total_time"] <= 40).all()
    assert (call(fare_structure=fares, max_fare=4.4)["fare"] <= 4.4).all()
    assert len(call(max_options=1)) <= 3
    with pytest.raises(ValueError, match="needs a fare_structure"):
        call(criteria=["FARE"])
    with pytest.raises(ValueError, match="criteria"):
        call(criteria=["TIME", "SPEED"])
    with pytest.raises(TypeError, match="criteria"):
        call(criteria=[])
    with pytest.raises(ValueError, match="max_options"):
        call(max_options=0)
    # exact walking keeps as many options or more
    assert len(call(walk_resolution=1)) >= len(a)
    with pytest.raises(ValueError, match="walk_resolution"):
        call(walk_resolution=0)


def test_street_only_and_output_dir(network, points, departure, tmp_path):
    w = peabiru.trip_options(
        network, points.iloc[:2], points.iloc[2:4], mode="WALK",
        departure_datetime=departure, time_window=2, drop_geometry=True,
    )
    assert (w.options["description"] == "WALK").all()
    assert (w.options["n_departures"] == 2).all()
    assert (w.options["walk_time"] == w.options["total_time"]).all()
    out = peabiru.trip_options(
        network, points.iloc[:2], points.iloc[2:4], departure_datetime=departure,
        time_window=2, drop_geometry=True, output_dir=tmp_path,
    )
    assert out == tmp_path
    files = sorted(p.name for p in tmp_path.iterdir())
    assert len(files) == 2
    x = pd.read_csv(tmp_path / files[0])
    assert {"option", "total_time", "segment", "mode"} <= set(x.columns)
