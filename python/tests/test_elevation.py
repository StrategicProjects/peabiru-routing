"""Elevation: networks built with a .tif raster. data/spo_hills.tif is
synthetic (smooth hills between 743 and 834 m over the Sao Paulo fixture,
made with terra for these tests), not measured data."""

import os
import shutil
import warnings

import pytest

import peabiru

HILLS = os.path.join(os.path.dirname(__file__), "data", "spo_hills.tif")


def _dir(tmp_path, rasters=None):
    for f in ("spo_osm.pbf", "spo_gtfs.zip"):
        shutil.copy(os.path.join(peabiru.sample_data_path(), f), tmp_path)
    for name, src in (rasters if rasters is not None else {"spo_hills.tif": HILLS}).items():
        shutil.copy(src, tmp_path / name)
    return tmp_path


def _times(network, points, mode):
    return peabiru.travel_time_matrix(network, points, points, mode=mode, n_threads=2)


def test_slope_costs_from_a_raster(tmp_path, network, points):
    d = _dir(tmp_path)
    tobler = peabiru.build_network(d, n_threads=2)
    assert tobler.elevation == "TOBLER"
    assert tobler.info()["elevation"] == "TOBLER"
    assert "elevation: TOBLER" in repr(tobler)
    minetti = peabiru.build_network(d, elevation="minetti", n_threads=2)
    assert minetti.elevation == "MINETTI"
    none = peabiru.build_network(d, elevation="NONE", n_threads=2)
    assert none.elevation == "NONE" and none.has_tif
    for mode in ("WALK", "BICYCLE"):
        t0 = _times(none, points, mode)
        assert t0.equals(_times(network, points, mode))
        t1 = _times(tobler, points, mode)
        t2 = _times(minetti, points, mode)
        assert not t0["travel_time_p50"].equals(t1["travel_time_p50"])
        assert not t1["travel_time_p50"].equals(t2["travel_time_p50"])
    assert _times(tobler, points, "CAR").equals(_times(network, points, "CAR"))


def test_a_scenario_drops_the_slope_costs_with_a_warning(tmp_path, network, points):
    tobler = peabiru.build_network(_dir(tmp_path), n_threads=2)
    with pytest.warns(UserWarning, match="due to missing elevation. See issue #555"):
        s = peabiru.travel_time_matrix(
            tobler, points, points, mode="WALK", carspeed_scale=0.9, n_threads=2
        )
    assert s.equals(_times(network, points, "WALK"))
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        peabiru.travel_time_matrix(
            network, points, points, mode="WALK", carspeed_scale=0.9, n_threads=2
        )


def test_rasters_r5r_cannot_read_are_errors(tmp_path):
    a = tmp_path / "a"
    a.mkdir()
    with pytest.raises(RuntimeError, match="ends in .tiff"):
        peabiru.build_network(_dir(a, {"hills.tiff": HILLS}), n_threads=2)
    b = tmp_path / "b"
    b.mkdir()
    with pytest.raises(RuntimeError, match=r"2 elevation rasters \(a.tif, b.tif\)"):
        peabiru.build_network(_dir(b, {"a.tif": HILLS, "b.tif": HILLS}), n_threads=2)


def test_an_upper_case_name_is_not_a_raster(tmp_path):
    network = peabiru.build_network(_dir(tmp_path, {"HILLS.TIF": HILLS}), n_threads=2)
    assert network.elevation == "NONE"
    assert not network.has_tif
