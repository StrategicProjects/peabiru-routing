"""isochrone(): r5r's columns, order and checks on the bundled sample."""

import datetime as dt
import warnings

import pytest

import peabiru

gpd = pytest.importorskip("geopandas")
shapely = pytest.importorskip("shapely")

DEP = dt.datetime(2019, 5, 13, 14, 0)


@pytest.fixture(scope="module")
def network():
    return peabiru.build_network(peabiru.sample_data_path(), n_threads=2)


@pytest.fixture(scope="module")
def points():
    return peabiru.sample_points()


def iso(network, origins, **kw):
    kw.setdefault("departure_datetime", DEP)
    return peabiru.isochrone(network, origins, n_threads=2, **kw)


def test_polygons_have_r5r_columns_and_order(network, points):
    o = points.iloc[:2]
    x = iso(network, o, mode="WALK", cutoffs=[20, 0, 10, 10], percentiles=[50, 25])
    assert isinstance(x, gpd.GeoDataFrame)
    assert list(x.columns) == ["id", "isochrone", "percentile", "polygons"]
    assert x.geometry.name == "polygons"
    assert x.crs.to_epsg() == 4326
    assert list(x["id"]) == [o["id"].iloc[0]] * 4 + [o["id"].iloc[1]] * 4
    assert list(x["percentile"]) == ["p25", "p25", "p50", "p50"] * 2
    assert list(x["isochrone"]) == [20.0, 10.0] * 4
    assert all(g.geom_type == "MultiPolygon" for g in x.geometry)
    # the area grows with the cutoff, and holds the smaller band
    for k in range(0, 8, 2):
        big, small = x.geometry.iloc[k], x.geometry.iloc[k + 1]
        assert big.area > small.area > 0
        assert shapely.make_valid(big).intersection(shapely.make_valid(small)).area == pytest.approx(
            small.area, rel=1e-9
        )


def test_lines_have_r5r_columns_and_order(network, points):
    o = points.iloc[:2]
    x = iso(network, o, mode="WALK", cutoffs=[0, 5, 10], polygon_output=False)
    assert list(x.columns) == [
        "id", "edge_index", "osm_id", "isochrone", "travel_time_p50", "from_vertex",
        "to_vertex", "street_class", "length", "walk", "car", "car_speed", "bicycle",
        "bicycle_lts", "geometry",
    ]
    assert all(isinstance(v, str) for v in x["edge_index"])
    assert str(x["travel_time_p50"].dtype) == "Int32"
    assert all(g.geom_type == "LineString" for g in x.geometry)
    assert ((x["travel_time_p50"] > 0) & (x["travel_time_p50"] <= x["isochrone"])).all()
    assert set(x["isochrone"]) <= {5.0, 10.0}
    assert list(dict.fromkeys(x["id"])) == list(o["id"])
    for _, r in x.groupby("id", sort=False):
        key = list(zip(-r["isochrone"], -r["travel_time_p50"].astype(int)))
        assert key == sorted(key)


def test_transit_with_a_seed_repeats(network, points):
    o = points.iloc[:1]
    a = iso(network, o, mode="TRANSIT", cutoffs=[0, 15, 25], seed=3)
    b = iso(network, o, mode=["WALK", "TRANSIT"], cutoffs=[0, 15, 25], seed=3)
    assert a.equals(b)
    lines = iso(network, o, mode="TRANSIT", cutoffs=[0, 15, 25], seed=3, polygon_output=False)
    assert len(lines) > 0


def test_unreachable_origin(network):
    import pandas as pd

    far = pd.DataFrame({"id": ["far"], "lon": [-46.0], "lat": [-23.0]})
    x = iso(network, far, mode="WALK", cutoffs=[0, 10])
    assert len(x) == 1 and x.geometry.iloc[0].is_empty
    lines = iso(network, far, mode="WALK", cutoffs=[0, 10], polygon_output=False)
    assert len(lines) == 0


def test_arguments_are_checked(network, points):
    o = points.iloc[:1]
    with pytest.raises(ValueError, match="at least one value greater than 0"):
        iso(network, o, mode="WALK", cutoffs=0)
    for bad in ([-5, 10], [10, float("nan")], [10, float("inf")]):
        with pytest.raises(ValueError, match="cutoffs"):
            iso(network, o, mode="WALK", cutoffs=bad)
    with pytest.raises(TypeError, match="cutoffs"):
        iso(network, o, mode="WALK", cutoffs="10")
    for z in (8, 13):
        with pytest.raises(ValueError, match="zoom"):
            iso(network, o, mode="WALK", zoom=z)
    with pytest.raises(TypeError, match="zoom"):
        iso(network, o, mode="WALK", zoom=10.5)
    with pytest.raises(TypeError, match="polygon_output"):
        iso(network, o, mode="WALK", polygon_output=None)
    with pytest.raises(ValueError, match="Percentiles not supported"):
        iso(network, o, mode="WALK", polygon_output=False, percentiles=50)
    with pytest.raises(ValueError, match="percentiles"):
        iso(network, o, mode="WALK", percentiles=100)
    # bicycle access and car egress around public transport route
    assert len(iso(network, o, mode=["BICYCLE", "TRANSIT"], mode_egress="CAR",
                   seed=1)) > 0
    with pytest.raises(ValueError, match="departure_datetime"):
        peabiru.isochrone(network, o, mode="TRANSIT")
    with pytest.warns(UserWarning, match="sample_size"):
        iso(network, o, mode="WALK", cutoffs=[0, 5], sample_size=0.5)
    # max_trip_duration is ignored
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        a = iso(network, o, mode="WALK", cutoffs=[0, 5], max_trip_duration=1)
    assert a.equals(iso(network, o, mode="WALK", cutoffs=[0, 5]))
