import os
import pickle
import shutil

import pytest

import peabiru


def test_core_version():
    assert peabiru.core_version() == peabiru.__version__
    assert isinstance(peabiru.core_version(), str)


def test_network_from_pbf_and_gtfs(network):
    assert isinstance(network, peabiru.Network)
    assert network.elevation == "NONE"
    assert not network.has_tif
    text = repr(network)
    assert text.startswith("<peabiru.Network>")
    assert "1 feed(s) (spo_gtfs)" in text
    info = network.info()
    assert info["feeds"] == ["spo_gtfs"]
    assert info["vertices"] > 0 and info["edges"] > 0 and info["frequencies"]


def test_street_only_network(tmp_path, points, departure):
    shutil.copy(os.path.join(peabiru.sample_data_path(), "spo_osm.pbf"), tmp_path)
    network = peabiru.build_network(tmp_path, n_threads=2)
    assert "transit:   none" in repr(network)
    with pytest.raises(ValueError, match="no transit services available"):
        peabiru.travel_time_matrix(
            network, points, points, mode="TRANSIT",
            departure_datetime=departure, n_threads=2,
        )
    ttm = peabiru.travel_time_matrix(network, points, points, mode="CAR", n_threads=2)
    assert len(ttm) > 0


def test_bad_data_paths(tmp_path):
    with pytest.raises(ValueError, match="does not exist"):
        peabiru.build_network(str(tmp_path / "missing"))
    with pytest.raises(ValueError, match="OSM .pbf file is required"):
        peabiru.build_network(tmp_path)
    pbf = os.path.join(peabiru.sample_data_path(), "spo_osm.pbf")
    shutil.copy(pbf, tmp_path / "a.pbf")
    shutil.copy(pbf, tmp_path / "b.pbf")
    with pytest.raises(ValueError, match="more than one .pbf"):
        peabiru.build_network(tmp_path)
    with pytest.raises(ValueError, match="Invalid value"):
        peabiru.build_network(peabiru.sample_data_path(), elevation="FLAT")
    with pytest.raises(TypeError, match="verbose"):
        peabiru.build_network(peabiru.sample_data_path(), verbose="yes")
    with pytest.raises(ValueError, match="n_threads"):
        peabiru.build_network(peabiru.sample_data_path(), n_threads=0)


def test_elevation_needs_none_with_a_raster(tmp_path):
    shutil.copy(os.path.join(peabiru.sample_data_path(), "spo_osm.pbf"), tmp_path)
    (tmp_path / "elevation.tif").write_bytes(bytes(10))
    with pytest.raises(peabiru.UnsupportedError, match="Elevation .* is not supported"):
        peabiru.build_network(tmp_path)
    network = peabiru.build_network(tmp_path, elevation="none", n_threads=2)
    assert network.has_tif
    assert network.elevation == "NONE"


def test_verbose_messages(capsys):
    peabiru.build_network(peabiru.sample_data_path(), verbose=True, n_threads=2)
    err = capsys.readouterr().err
    assert 'Using elevation = "NONE"' in err
    assert "Finished building network" in err


def test_network_cannot_be_pickled(network):
    with pytest.raises(TypeError, match="cannot be pickled"):
        pickle.dumps(network)


def test_unsupported_error_is_not_implemented():
    assert issubclass(peabiru.UnsupportedError, NotImplementedError)
