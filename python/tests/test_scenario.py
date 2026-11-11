"""Scenarios: new_carspeeds, carspeed_scale and new_lts (parity with r5r is
in bench/compare/py_parity.py)."""

import warnings

import pandas as pd
import pytest

import peabiru


def ttm(network, points, **kw):
    kw.setdefault("n_threads", 2)
    return peabiru.travel_time_matrix(network, points, points, max_trip_duration=60, **kw)


@pytest.fixture(scope="module")
def ways(network):
    pytest.importorskip("geopandas")
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        return sorted(set(peabiru.street_network_to_sf(network)["edges"]["osm_id"]))


def joined(a, b):
    return a.merge(b, on=["from_id", "to_id"])


def test_car_speed_scenarios(network, points, ways):
    base = ttm(network, points, mode="CAR")
    pd.testing.assert_frame_equal(ttm(network, points, mode="CAR", carspeed_scale=1), base)
    slow = ttm(network, points, mode="CAR", carspeed_scale=0.5)
    j = joined(base, slow)
    assert (j["travel_time_p50_y"] >= j["travel_time_p50_x"]).all()
    assert (j["travel_time_p50_y"] > j["travel_time_p50_x"]).any()
    every = pd.DataFrame({"osm_id": ways, "max_speed": 0.5, "speed_type": "scale"})
    pd.testing.assert_frame_equal(
        ttm(network, points, mode="CAR", new_carspeeds=every, carspeed_scale=0.5), slow
    )
    assert len(ttm(network, points, mode="CAR", carspeed_scale=0)) == 0
    pd.testing.assert_frame_equal(
        ttm(network, points, mode="WALK", carspeed_scale=0.5), ttm(network, points, mode="WALK")
    )


def test_lts_scenarios(network, points, ways):
    stressful = pd.DataFrame({"osm_id": ways, "lts": 4})
    base = ttm(network, points, mode="BICYCLE")
    j = joined(base, ttm(network, points, mode="BICYCLE", new_lts=stressful))
    assert (j["travel_time_p50_y"] >= j["travel_time_p50_x"]).all()
    assert (j["travel_time_p50_y"] > j["travel_time_p50_x"]).any()
    pd.testing.assert_frame_equal(
        ttm(network, points, mode="BICYCLE", new_lts=stressful, max_lts=4),
        ttm(network, points, mode="BICYCLE", max_lts=4),
    )


def test_scenario_arguments_are_checked_as_in_r5r(network, points):
    def call(**kw):
        return ttm(network, points, mode="CAR", **kw)

    with pytest.raises(TypeError, match="Must inherit from class 'DataFrame'"):
        call(new_carspeeds=1)
    with pytest.raises(ValueError, match="Element 1 is not >= 0"):
        call(carspeed_scale=-1)
    # r5r's check lets NaN through, then fails with an internal R error
    with pytest.raises(ValueError, match="must be a number that is not NaN"):
        call(carspeed_scale=float("nan"))
    with pytest.raises(TypeError, match="Must be of type 'numeric', not 'str'"):
        call(carspeed_scale="1")
    with pytest.raises(ValueError, match="32-bit float"):
        call(carspeed_scale=1e300)
    with pytest.raises(ValueError, match=r"missing elements \{'speed_type'\}"):
        call(new_carspeeds=pd.DataFrame({"osm_id": [1], "max_speed": [1]}))
    with pytest.raises(ValueError, match="Must be TRUE"):
        call(new_carspeeds=pd.DataFrame({"osm_id": [1], "max_speed": [1], "speed_type": ["kph"]}))
    with pytest.raises(ValueError, match="Contains only missing values"):
        empty = pd.Series([], dtype=float)
        call(new_carspeeds=pd.DataFrame({"osm_id": empty, "max_speed": empty, "speed_type": []}))
    # as R writes 1e5 (a double) for Java: "1e+05", not a Java long
    with pytest.raises(ValueError, match="is not an integer as R writes it"):
        call(new_carspeeds=pd.DataFrame({"osm_id": [1e5], "max_speed": [1.0], "speed_type": ["scale"]}))
    with pytest.raises(TypeError, match="Must be of type 'integer', not 'double'"):
        call(new_lts=pd.DataFrame({"osm_id": [1], "lts": [1.0]}))
    with pytest.raises(ValueError, match="speed of 0 or less"):
        call(carspeed_scale=1e9)
    # an id that is not in the network changes nothing
    pd.testing.assert_frame_equal(
        call(new_carspeeds=pd.DataFrame({"osm_id": [1], "max_speed": [0.5], "speed_type": ["scale"]})),
        call(),
    )


def test_geodataframe_scenarios(network, points):
    gpd = pytest.importorskip("geopandas")
    from shapely.geometry import LineString, box

    b = (points["lon"].min(), points["lat"].min(), points["lon"].max(), points["lat"].max())
    poly = gpd.GeoDataFrame(
        {"poly_id": ["a"], "scale": [0.5], "priority": [1]}, geometry=[box(*b)], crs=4326
    )
    pd.testing.assert_frame_equal(
        ttm(network, points, mode="CAR", new_carspeeds=poly, carspeed_scale=0.5),
        ttm(network, points, mode="CAR", carspeed_scale=0.5),
    )
    with pytest.raises(ValueError, match="must be WGS 84"):
        ttm(network, points, mode="CAR", new_carspeeds=poly.set_crs(3857, allow_override=True))
    line = gpd.GeoDataFrame(
        {"line_id": ["a"], "lts": [9], "priority": [1]},
        geometry=[LineString([(b[0], b[1]), (b[2], b[3])])],
        crs=4326,
    )
    with pytest.raises(ValueError, match="Element 1 is not <= 4"):
        ttm(network, points, mode="BICYCLE", new_lts=line)
    line["lts"] = 4
    assert isinstance(ttm(network, points, mode="BICYCLE", new_lts=line), pd.DataFrame)
