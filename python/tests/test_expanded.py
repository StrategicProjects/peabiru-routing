import datetime as dt
from zoneinfo import ZoneInfo

import numpy as np
import pandas as pd
import pytest

import peabiru

ALL = [
    "from_id", "to_id", "departure_time", "draw_number", "access_time",
    "wait_time", "ride_time", "transfer_time", "egress_time", "routes",
    "n_rides", "total_time",
]


def test_expanded_columns_and_types(network, points, departure):
    e = peabiru.expanded_travel_time_matrix(
        network, points.iloc[:3], points, mode="TRANSIT", departure_datetime=departure,
        time_window=3, breakdown=True, seed=1, n_threads=2,
    )
    assert list(e.columns) == ALL
    assert e["departure_time"].dtype == object
    assert e["routes"].dtype == object
    assert e["draw_number"].dtype == np.int32
    assert e["access_time"].dtype == np.float64
    assert str(e["n_rides"].dtype) == "Int32"
    assert e["total_time"].dtype == np.float64
    assert set(e["departure_time"]) == {"14:00:00", "14:01:00", "14:02:00"}
    # 5 draws per minute for frequency-based GTFS
    assert set(e["draw_number"]) <= {1, 2, 3, 4, 5}
    walk = e["routes"] == "[WALK]"
    assert (e.loc[walk, "access_time"] == 0).all()
    ok = e["total_time"].notna() & (e["n_rides"] > 0).fillna(False)
    assert ok.any()
    parts = e[["access_time", "wait_time", "ride_time", "transfer_time", "egress_time"]].sum(
        axis=1
    )
    assert ((parts[ok] - e.loc[ok, "total_time"]).abs() < 0.51).all()

    nobd = peabiru.expanded_travel_time_matrix(
        network, points.iloc[:3], points, mode="TRANSIT", departure_datetime=departure,
        time_window=3, seed=1, n_threads=2,
    )
    assert list(nobd.columns) == [
        "from_id", "to_id", "departure_time", "draw_number", "routes", "total_time",
    ]
    pd.testing.assert_frame_equal(nobd, e[list(nobd.columns)])


def test_trips_beyond_the_limit_are_missing(network, points, departure):
    e = peabiru.expanded_travel_time_matrix(
        network, points.iloc[:3], points, mode="TRANSIT", departure_datetime=departure,
        time_window=2, max_trip_duration=8, breakdown=True, seed=1, n_threads=2,
    )
    assert (e["total_time"].dropna() <= 8).all()
    na = e["total_time"].isna()
    assert na.any()
    assert e.loc[na, "routes"].map(lambda v: v is None).all()
    assert e.loc[na, "n_rides"].isna().all()
    assert e.loc[na, "wait_time"].isna().all()


def test_arrival_keeps_the_latest_departure_arriving_in_time(network, points):
    tz = ZoneInfo("America/Sao_Paulo")
    arrival = dt.datetime(2019, 5, 13, 15, 0, tzinfo=tz)
    a = peabiru.arrival_travel_time_matrix(
        network, points.iloc[:3], points, mode="TRANSIT", arrival_datetime=arrival,
        max_trip_duration=40, breakdown=True, seed=1, n_threads=2,
    )
    assert list(a.columns) == ALL
    assert not a.duplicated(["from_id", "to_id"]).any()
    dep = pd.to_datetime("2019-05-13 " + a["departure_time"])
    arr = pd.Timestamp("2019-05-13 15:00:00")
    assert ((dep >= arr - pd.Timedelta(minutes=40)) & (dep < arr)).all()
    assert (dep + pd.to_timedelta((a["total_time"] * 60).round(), unit="s") <= arr).all()
    # a naive datetime is the same clock time
    naive = peabiru.arrival_travel_time_matrix(
        network, points.iloc[:3], points, mode="TRANSIT",
        arrival_datetime=arrival.replace(tzinfo=None), max_trip_duration=40,
        breakdown=True, seed=1, n_threads=2,
    )
    pd.testing.assert_frame_equal(a, naive)


def test_arrival_window_crossing_midnight(network, points):
    a = peabiru.arrival_travel_time_matrix(
        network, points.iloc[:2], points.iloc[:5], mode="TRANSIT",
        arrival_datetime=dt.datetime(2019, 5, 14, 0, 20), max_trip_duration=30,
        seed=1, n_threads=2,
    )
    # departures from 23:50 on the 13th, written past 24:00 after midnight
    assert len(a) > 0
    assert ((a["departure_time"] >= "23:50:00") & (a["departure_time"] < "24:20:00")).all()


def test_arrival_arithmetic_on_aware_datetimes_is_absolute():
    from peabiru._assign import minus_minutes

    # 2018-11-04 00:30 in Sao Paulo is right after the DST jump (00:00 -> 01:00
    # did not exist: clocks went from 23:59:59 to 01:00); R's POSIXct
    # arithmetic is on the absolute time
    tz = ZoneInfo("America/Sao_Paulo")
    t = dt.datetime(2018, 11, 4, 1, 30, tzinfo=tz)
    back = minus_minutes(t, 60)
    assert (back.hour, back.minute, back.day) == (23, 30, 3)
    # naive: clock-time arithmetic
    back = minus_minutes(t.replace(tzinfo=None), 60)
    assert (back.hour, back.minute, back.day) == (0, 30, 4)


def test_unsupported_and_invalid(network, points, departure):
    with pytest.raises(peabiru.UnsupportedError, match="WALK alone"):
        peabiru.expanded_travel_time_matrix(network, points, points, mode="WALK")
    with pytest.raises(peabiru.UnsupportedError, match="BICYCLE alone"):
        peabiru.arrival_travel_time_matrix(
            network, points, points, mode="BICYCLE", arrival_datetime=departure
        )
    with pytest.raises(ValueError, match="'arrival_datetime' is required"):
        peabiru.arrival_travel_time_matrix(network, points, points, mode="TRANSIT")
    with pytest.raises(TypeError, match="arrival_datetime"):
        peabiru.arrival_travel_time_matrix(
            network, points, points, mode="TRANSIT", arrival_datetime="15:00"
        )
    # by car, a record without public transport has the routes "[CAR]"
    car = peabiru.expanded_travel_time_matrix(
        network, points, points, mode=["CAR", "TRANSIT"], departure_datetime=departure,
        max_car_time=5, seed=1,
    )
    assert "[CAR]" in set(car["routes"]) and "[WALK]" not in set(car["routes"])
    with pytest.raises(TypeError, match="breakdown"):
        peabiru.expanded_travel_time_matrix(
            network, points, points, mode="TRANSIT", departure_datetime=departure,
            breakdown=None,
        )


def test_arrival_with_max_trip_duration_over_a_day(network, points, departure):
    peabiru.arrival_travel_time_matrix(
        network, points.iloc[:1], points.iloc[:2], mode="TRANSIT",
        arrival_datetime=departure, max_trip_duration=1500, seed=1,
    )
