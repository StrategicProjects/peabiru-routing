"""Reproducers from the pre-release review (0.0.1).

Each test states the expected behaviour and is marked ``xfail(strict=True)``
while the product code does not meet it, so the suite stays green and a fix
turns the test into an unexpected pass (which then fails, prompting removal
of the marker). The r5r behaviour quoted was observed with r5r 2.5.0 and
R5 7.6 on the same sample network.
"""

import datetime as dt
import os
import signal
import sys
import threading
import time
import warnings

import numpy as np
import pandas as pd
import pytest

import peabiru


@pytest.mark.xfail(strict=True, reason="street modes return a matrix without time columns")
def test_empty_percentiles_is_an_error_for_street_modes(network, points):
    # r5r 2.5.0 fails (ArrayIndexOutOfBoundsException in R5) and peabiru
    # already fails for transit and for accessibility; a street-mode
    # travel_time_matrix() returns from_id/to_id only.
    with pytest.raises(ValueError):
        peabiru.travel_time_matrix(network, points, points, mode="WALK", percentiles=[])


@pytest.mark.xfail(strict=True, reason="core caps time_window at 1440 minutes")
def test_time_window_longer_than_a_day_is_accepted(network, points, departure):
    # r5r 2.5.0 returns a matrix for time_window = 2000 (same values as 1440
    # here: 0 and 7 minutes)
    ttm = peabiru.travel_time_matrix(
        network, points.iloc[:1], points.iloc[:2], mode="TRANSIT",
        departure_datetime=departure, time_window=2000, seed=1,
    )
    assert ttm["travel_time_p50"].tolist() == [0, 7]


@pytest.mark.xfail(strict=True, reason="arrival passes max_trip_duration as time_window, capped at 1440")
def test_arrival_with_max_trip_duration_over_a_day(network, points, departure):
    peabiru.arrival_travel_time_matrix(
        network, points.iloc[:1], points.iloc[:2], mode="TRANSIT",
        arrival_datetime=departure, max_trip_duration=1500, seed=1,
    )


@pytest.mark.xfail(
    condition=int(pd.__version__.split(".")[0]) < 3,
    strict=True,
    reason="with pandas < 3, astype(str) turns a missing id into the text 'nan'",
)
def test_missing_numeric_id_stays_missing(network, points):
    # r5r: as.character(NA) is NA, so the row's from_id is NA
    p = points.copy()
    p["id"] = np.arange(len(p), dtype=float)
    p.loc[0, "id"] = np.nan
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        ttm = peabiru.travel_time_matrix(network, p, points.iloc[:1], mode="WALK")
    assert pd.isna(ttm["from_id"].iloc[0])


@pytest.mark.skipif(sys.platform == "win32", reason="POSIX signals")
@pytest.mark.xfail(strict=True, reason="the core never checks for signals while routing")
def test_ctrl_c_interrupts_a_long_call(network, points):
    start = dt.datetime(2019, 5, 13, 0, 0)

    def fire():
        time.sleep(0.3)
        os.kill(os.getpid(), signal.SIGINT)

    threading.Thread(target=fire).start()
    t0 = time.perf_counter()
    with pytest.raises(KeyboardInterrupt):
        peabiru.travel_time_matrix(
            network, points, points, mode="TRANSIT", departure_datetime=start,
            time_window=1440, draws_per_minute=5, seed=1, n_threads=1,
        )
    assert time.perf_counter() - t0 < 2.0
