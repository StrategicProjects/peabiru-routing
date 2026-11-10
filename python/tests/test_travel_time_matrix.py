import datetime as dt
import math
import threading
import warnings

import numpy as np
import pandas as pd
import pytest

import peabiru


def ttm(network, points, **kw):
    kw.setdefault("n_threads", 2)
    return peabiru.travel_time_matrix(network, points, points, **kw)


@pytest.mark.parametrize("mode", ["WALK", "BICYCLE", "CAR"])
def test_street_matrices_have_r5rs_columns_and_types(network, points, mode):
    m = ttm(network, points, mode=mode)
    assert isinstance(m, pd.DataFrame)
    assert list(m.columns) == ["from_id", "to_id", "travel_time_p50"]
    assert m["from_id"].dtype == object and m["to_id"].dtype == object
    assert str(m["travel_time_p50"].dtype) == "Int32"
    assert (m["travel_time_p50"] <= 120).all()
    assert isinstance(m.index, pd.RangeIndex)
    # origin-major, in input order
    seen = list(dict.fromkeys(m["from_id"]))
    assert seen == [i for i in points["id"] if i in set(seen)]


def test_a_street_mode_reports_its_time_for_every_percentile(network, points):
    m = ttm(network, points, mode="BICYCLE", percentiles=[75, 10])
    assert list(m.columns) == ["from_id", "to_id", "travel_time_p10", "travel_time_p75"]
    assert m["travel_time_p10"].equals(m["travel_time_p75"])


def test_trip_limit_and_mode_limit_drop_pairs(network, points):
    full = ttm(network, points, mode="WALK")
    short = ttm(network, points, mode="WALK", max_trip_duration=10)
    walk10 = ttm(network, points, mode="WALK", max_walk_time=10)
    assert (short["travel_time_p50"] <= 10).all()
    pd.testing.assert_frame_equal(short, walk10)
    assert len(short) < len(full)


def test_walk_with_more_origins_keeps_the_callers_columns(network, points):
    m = peabiru.travel_time_matrix(network, points, points.iloc[:3], mode="WALK", n_threads=2)
    assert list(m.columns) == ["from_id", "to_id", "travel_time_p50"]
    assert set(m["to_id"]) <= set(points["id"].iloc[:3])
    assert set(m["from_id"]) == set(points["id"])
    # rows follow the routed origins, i.e. the destinations
    assert list(dict.fromkeys(m["to_id"])) == list(points["id"].iloc[:3])


def test_transit_matrices_have_one_column_per_percentile(network, points, departure):
    m = ttm(network, points, mode=["WALK", "TRANSIT"], departure_datetime=departure,
            percentiles=[95, 5, 50], seed=1)
    assert list(m.columns) == [
        "from_id", "to_id", "travel_time_p05", "travel_time_p50", "travel_time_p95",
    ]
    for c in m.columns[2:]:
        assert str(m[c].dtype) == "Int32"
    assert (m["travel_time_p05"] <= m["travel_time_p50"]).dropna().all()
    assert (m["travel_time_p50"] <= m["travel_time_p95"]).dropna().all()
    assert not m["travel_time_p05"].isna().any()
    again = ttm(network, points, mode="TRANSIT", departure_datetime=departure,
                percentiles=(5, 50, 95), seed=1, n_threads=1)
    pd.testing.assert_frame_equal(m, again)


def test_higher_percentiles_beyond_the_limit_are_na(network, points, departure):
    m = ttm(network, points, mode="TRANSIT", departure_datetime=departure,
            percentiles=[1, 99], max_trip_duration=12, seed=1)
    assert (m["travel_time_p01"] <= 12).all()
    assert m["travel_time_p99"].isna().any()
    assert (m["travel_time_p99"].dropna() <= 12).all()


def test_datetimes_are_wall_clock_times(network, points):
    naive = ttm(network, points, mode="TRANSIT", seed=1,
                departure_datetime=dt.datetime(2019, 5, 13, 14, 0))
    from zoneinfo import ZoneInfo

    for tz in (ZoneInfo("America/Sao_Paulo"), dt.timezone.utc, ZoneInfo("Asia/Tokyo")):
        aware = ttm(network, points, mode="TRANSIT", seed=1,
                    departure_datetime=dt.datetime(2019, 5, 13, 14, 0, tzinfo=tz))
        pd.testing.assert_frame_equal(naive, aware)
    stamp = ttm(network, points, mode="TRANSIT", seed=1,
                departure_datetime=pd.Timestamp("2019-05-13 14:00:00"))
    pd.testing.assert_frame_equal(naive, stamp)


def test_points_are_checked_as_r5r_checks_them(network, points):
    with pytest.raises(TypeError, match="must be either a 'DataFrame'"):
        peabiru.travel_time_matrix(network, points.to_dict(), points)
    with pytest.raises(ValueError, match=r"missing elements \{'lat'\}"):
        peabiru.travel_time_matrix(network, points[["id", "lon"]], points)
    with pytest.raises(ValueError, match="must not contain duplicated ids"):
        peabiru.travel_time_matrix(network, pd.concat([points, points.iloc[:1]]), points)
    with pytest.raises(TypeError, match=r"origins\$lon"):
        bad = points.assign(lon=points["lon"].astype(str))
        peabiru.travel_time_matrix(network, bad, points)
    num = points.assign(id=range(1, len(points) + 1))
    with pytest.warns(UserWarning, match="forcefully cast to character"):
        m = peabiru.travel_time_matrix(network, num, points, mode="CAR", n_threads=2)
    assert m["from_id"].iloc[0] == "1"
    na = points.copy()
    na.loc[1, "lon"] = np.nan
    with pytest.warns(UserWarning, match="missing lon/lat"):
        res = peabiru.travel_time_matrix(network, na, points, mode="CAR", n_threads=2)
    assert points["id"].iloc[1] not in set(res["from_id"])


def test_string_dtype_ids_and_nullable_coordinates(network, points):
    ref = ttm(network, points, mode="CAR")
    pts = points.astype({"id": "string", "lon": "Float64", "lat": "Float64"})
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        m = peabiru.travel_time_matrix(network, pts, pts, mode="CAR", n_threads=2)
    pd.testing.assert_frame_equal(m, ref)
    # a non-default index does not matter
    shifted = points.set_index(points.index + 100)
    pd.testing.assert_frame_equal(ttm(network, shifted, mode="CAR"), ref)


@pytest.mark.parametrize("dtype", ["float64", "Int64", "object"])
def test_missing_numeric_id_stays_missing(network, points, dtype):
    # r5r: as.character(NA) is NA, so the row's from_id is NA; the other
    # ids convert with astype(str) as before
    p = points.copy()
    p["id"] = pd.Series(np.arange(len(p)), dtype=dtype)
    p.loc[0, "id"] = None
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        m = peabiru.travel_time_matrix(network, p, points.iloc[:1], mode="WALK", n_threads=2)
    assert m["from_id"].iloc[0] is None
    expected = p["id"].iloc[1:].astype(str).tolist()
    assert m["from_id"].iloc[1:].tolist() == expected
    assert not set(m["from_id"].dropna()) & {"nan", "<NA>", "None"}


def test_geodataframe_points(network, points):
    gpd = pytest.importorskip("geopandas")
    geo = gpd.GeoDataFrame(
        points[["id"]], geometry=gpd.points_from_xy(points["lon"], points["lat"]),
        crs="EPSG:4326",
    )
    a = ttm(network, points, mode="CAR")
    b = peabiru.travel_time_matrix(network, geo, geo, mode="CAR", n_threads=2)
    pd.testing.assert_frame_equal(a, b)
    with pytest.raises(ValueError, match="EPSG 4326"):
        peabiru.travel_time_matrix(network, geo.to_crs(3857), geo)


def test_arguments_are_checked_as_r5r_checks_them(network, points, departure):
    def call(**kw):
        return ttm(network, points, **kw)

    with pytest.raises(ValueError, match="must pass a 'departure_datetime'"):
        call(mode="TRANSIT")
    with pytest.raises(ValueError, match="Please use only 1 of"):
        call(mode=["WALK", "CAR"])
    with pytest.raises(ValueError, match="unsupported by r5r"):
        call(mode="CAR_PARK")
    with pytest.raises(ValueError, match="Must be a subset"):
        call(mode="PLANE")
    with pytest.raises(TypeError, match="mode"):
        call(mode=["WALK", 1])
    with pytest.raises(ValueError, match="length <= 5"):
        call(percentiles=[1, 2, 3, 4, 5, 6])
    # R5 fails on an empty list too; every mode stops before routing
    for m in ("WALK", "CAR", "TRANSIT"):
        with pytest.raises(ValueError, match="'percentiles' failed: Must have length >= 1"):
            call(mode=m, percentiles=[], departure_datetime=departure)
    with pytest.raises(ValueError, match="<= 99"):
        call(percentiles=100)
    with pytest.raises(ValueError, match="Must be >= 1"):
        call(time_window=0)
    with pytest.raises(TypeError, match="count"):
        call(max_trip_duration=1.5)
    with pytest.raises(ValueError, match=">= 1"):
        call(max_walk_time=0)
    with pytest.raises(ValueError, match="greater than 0"):
        call(walk_speed=0)
    with pytest.raises(ValueError, match="between 1 and 4"):
        call(max_lts=5)
    with pytest.raises(TypeError, match="integerish"):
        call(seed=1.5)
    with pytest.raises(ValueError, match=">= 1"):
        call(n_threads=0)
    with pytest.raises(ValueError, match="requires a 'fare_structure'"):
        call(max_fare=5)
    with pytest.raises(TypeError, match="datetime"):
        call(departure_datetime="2019-05-13 14:00")
    with pytest.raises(TypeError, match="progress"):
        call(progress=None)
    with pytest.raises(TypeError, match="peabiru.Network"):
        peabiru.travel_time_matrix({}, points, points)
    # what R accepts as whole numbers
    a = call(mode="CAR", percentiles=50.0, max_trip_duration=30.0, n_threads=2.0)
    b = call(mode="CAR", percentiles=[50], max_trip_duration=30, n_threads=math.inf)
    pd.testing.assert_frame_equal(a, b)


def test_unsupported_options_are_errors(network, points, departure):
    def call(**kw):
        return ttm(network, points, departure_datetime=departure, **kw)

    U = peabiru.UnsupportedError
    with pytest.raises(U, match="BICYCLE access is not supported"):
        call(mode=["BICYCLE", "TRANSIT"])
    with pytest.raises(U, match="CAR access is not supported"):
        call(mode=["CAR", "TRANSIT"])
    with pytest.raises(U, match="BICYCLE egress is not supported"):
        call(mode="TRANSIT", mode_egress="BICYCLE")
    with pytest.raises(U, match="specific transit modes"):
        call(mode=["WALK", "BUS"])
    with pytest.raises(U, match="fare_structure.*not supported"):
        call(fare_structure={})
    with pytest.raises(U, match="car speeds.*not supported"):
        call(carspeed_scale=0.5)
    with pytest.raises(U, match="car speeds.*not supported"):
        call(new_carspeeds=pd.DataFrame({"osm_id": [1], "max_speed": [1]}))
    with pytest.raises(U, match="LTS.*not supported"):
        call(new_lts=pd.DataFrame({"osm_id": [1], "lts": [1]}))
    with pytest.raises(U, match="output_dir.*not supported"):
        call(output_dir="/tmp")
    # TRANSIT with other transit modes is all transit modes, as in r5r
    pd.testing.assert_frame_equal(
        call(mode=["walk", "TRANSIT", "BUS"], seed=1), call(mode="TRANSIT", seed=1)
    )


def test_concurrent_calls_release_the_gil(network, points, departure):
    """Routing from several Python threads at once gives the same result
    as routing alone (the GIL is released while the core runs)."""
    ref = ttm(network, points, mode="TRANSIT", departure_datetime=departure, seed=3,
              n_threads=1)
    out = [None] * 4

    def work(k):
        out[k] = ttm(network, points, mode="TRANSIT", departure_datetime=departure,
                     seed=3, n_threads=1)

    threads = [threading.Thread(target=work, args=(k,)) for k in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    for r in out:
        pd.testing.assert_frame_equal(ref, r)


def test_empty_result(network, points):
    far = pd.DataFrame({"id": ["a"], "lon": [0.0], "lat": [0.0]})
    m = peabiru.travel_time_matrix(network, far, points, mode="WALK", n_threads=2)
    assert len(m) == 0
    assert list(m.columns) == ["from_id", "to_id", "travel_time_p50"]
    assert str(m["travel_time_p50"].dtype) == "Int32"
