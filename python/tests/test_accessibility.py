import numpy as np
import pandas as pd
import pytest

import peabiru


def acc(network, points, **kw):
    kw.setdefault("n_threads", 2)
    return peabiru.accessibility(network, points, points, **kw)


def test_columns_types_and_row_order(network, points, departure):
    a = acc(network, points, opportunities_colnames=["schools", "jobs"], mode="TRANSIT",
            departure_datetime=departure, cutoffs=[30, 15], percentiles=[50, 25], seed=1)
    assert list(a.columns) == ["id", "opportunity", "percentile", "cutoff", "accessibility"]
    assert a["id"].dtype == object and a["opportunity"].dtype == object
    assert a["percentile"].dtype == np.int32 and a["cutoff"].dtype == np.int32
    assert a["accessibility"].dtype == np.float64
    assert len(a) == len(points) * 8
    assert list(a["id"].iloc[:8]) == [points["id"].iloc[0]] * 8
    assert list(a["opportunity"].iloc[:8]) == ["schools"] * 4 + ["jobs"] * 4
    assert list(a["percentile"].iloc[:4]) == [25, 25, 50, 50]
    assert list(a["cutoff"].iloc[:4]) == [15, 30, 15, 30]
    wide = a["accessibility"][a["cutoff"] == 30].to_numpy() - a["accessibility"][
        a["cutoff"] == 15
    ].to_numpy()
    assert (wide >= 0).all()


@pytest.mark.parametrize("mode", ["WALK", "BICYCLE", "CAR"])
def test_every_decay_function_runs(network, points, mode):
    for f, v in [("step", None), ("exponential", None), ("linear", 10), ("logistic", 8)]:
        a = acc(network, points, opportunities_colnames="jobs", mode=mode,
                decay_function=f, decay_value=v, cutoffs=[10, 20])
        assert list(a.columns) == ["id", "opportunity", "percentile", "cutoff", "accessibility"]
        assert (a["accessibility"] >= 0).all()
    a = acc(network, points, opportunities_colnames="jobs", mode=mode,
            decay_function="fixed_exponential", decay_value=0.001)
    assert list(a.columns) == ["id", "opportunity", "percentile", "accessibility"]


def test_decay_arguments_are_checked(network, points):
    def call(**kw):
        kw.setdefault("cutoffs", 30)
        return acc(network, points, opportunities_colnames="jobs", mode="WALK", **kw)

    with pytest.raises(ValueError, match="must not be None"):
        call(cutoffs=None)
    with pytest.raises(ValueError, match="must be None"):
        call(decay_function="fixed_exponential", decay_value=0.1)
    with pytest.raises(ValueError, match="must be None"):
        call(decay_function="step", decay_value=1)
    with pytest.raises(ValueError, match="must not be None"):
        call(decay_function="linear")
    with pytest.raises(TypeError, match="integerish"):
        call(decay_function="linear", decay_value=2.5)
    with pytest.raises(ValueError, match="greater than or equal to 1"):
        call(decay_function="logistic", decay_value=0.5)
    with pytest.raises(ValueError, match="between 0 and 1"):
        call(decay_function="fixed_exponential", decay_value=1, cutoffs=None)
    with pytest.raises(ValueError, match="Must be a subset"):
        call(decay_function="gaussian")
    with pytest.raises(ValueError, match="<= 120"):
        call(cutoffs=121)
    with pytest.raises(ValueError, match="length <= 12"):
        call(cutoffs=list(range(1, 14)))
    with pytest.raises(ValueError, match="duplicated"):
        call(cutoffs=[30, 30])
    with pytest.raises(ValueError, match="cannot be shorter"):
        call(cutoffs=60, max_trip_duration=30)
    with pytest.raises(ValueError, match="missing elements"):
        acc(network, points, opportunities_colnames="nothing", cutoffs=30)
    with pytest.raises(TypeError, match="numeric"):
        acc(network, points.assign(jobs="x"), opportunities_colnames="jobs", cutoffs=30)
    with pytest.raises(ValueError, match="finite"):
        acc(network, points.assign(jobs=np.inf), opportunities_colnames="jobs", cutoffs=30)


def test_step_ignores_trips_beyond_the_largest_cutoff(network, points):
    a = acc(network, points, opportunities_colnames="jobs", cutoffs=20, max_trip_duration=120)
    b = acc(network, points, opportunities_colnames="jobs", cutoffs=20, max_trip_duration=20)
    pd.testing.assert_frame_equal(a, b)


def test_missing_opportunities_count_as_zero(network, points):
    pts = points.astype({"jobs": float})
    pts.loc[0:2, "jobs"] = np.nan
    with pytest.warns(UserWarning, match="3 missing value"):
        a = acc(network, pts, opportunities_colnames="jobs", cutoffs=30)
    pts.loc[0:2, "jobs"] = 0
    b = acc(network, pts, opportunities_colnames="jobs", cutoffs=30)
    pd.testing.assert_frame_equal(a, b)


def test_nullable_opportunity_columns(network, points):
    pts = points.astype({"jobs": "Int64"})
    a = acc(network, pts, opportunities_colnames="jobs", cutoffs=30)
    b = acc(network, points, opportunities_colnames="jobs", cutoffs=30)
    pd.testing.assert_frame_equal(a, b)
