import os
import pickle
import shutil
import warnings
import zipfile

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


def test_a_raster_is_not_read_with_elevation_none(tmp_path):
    shutil.copy(os.path.join(peabiru.sample_data_path(), "spo_osm.pbf"), tmp_path)
    (tmp_path / "elevation.tif").write_bytes(bytes(10))
    with pytest.raises(RuntimeError, match="elevation.tif: not a TIFF file"):
        peabiru.build_network(tmp_path)
    network = peabiru.build_network(tmp_path, elevation="none", n_threads=2)
    assert network.has_tif
    assert network.elevation == "NONE"


def test_a_name_that_merely_contains_tif_is_not_a_raster(tmp_path):
    # r5r looks for ".tif" as a regular expression, so this name counts for
    # the swap of origins and destinations, but R5 finds no raster to read
    shutil.copy(os.path.join(peabiru.sample_data_path(), "spo_osm.pbf"), tmp_path)
    (tmp_path / "identificacao_zonas.csv").write_text("zone\n")
    network = peabiru.build_network(tmp_path, n_threads=2)
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


def test_the_service_count_takes_a_calendar_date_only(network):
    total, active = network._core.transit_availability(20190513)
    assert total >= active > 0
    for bad in (20190230, 20191301, 0):
        with pytest.raises(ValueError, match="calendar date"):
            network._core.transit_availability(bad)


# A small feed around the sample streets; each test breaks it in one way.
FEED = {
    "agency.txt": "agency_id,agency_name,agency_url,agency_timezone\n"
    "A,A,http://a,America/Sao_Paulo\n",
    "stops.txt": "stop_id,stop_name,stop_lat,stop_lon\n"
    "A,A,-23.5563,-46.6290\nB,B,-23.5577,-46.6321\n",
    "routes.txt": "route_id,route_type\nR,3\n",
    "trips.txt": "route_id,service_id,trip_id\nR,S,T1\n",
    "stop_times.txt": "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n"
    "T1,14:00:00,14:00:00,A,1\nT1,14:05:00,14:05:00,B,2\n",
    "calendar.txt": "service_id,monday,tuesday,wednesday,thursday,friday,saturday,"
    "sunday,start_date,end_date\nS,1,1,1,1,1,1,1,20190101,20191231\n",
}


def refused(path, feeds):
    """r5r's build_network() fails on feeds with errors of priority HIGH."""
    return (
        f"High priority GTFS errors found; network build failed ({feeds}). "
        f'Use peabiru.get_gtfs_errors("{os.path.realpath(path)}") to see them.'
    )


def network_dir(tmp_path, feed, folder=""):
    """A data directory with the sample streets and `feed` as bus.zip."""
    shutil.copy(os.path.join(peabiru.sample_data_path(), "spo_osm.pbf"), tmp_path)
    with zipfile.ZipFile(tmp_path / "bus.zip", "w") as z:
        for name, text in feed.items():
            z.writestr(folder + name, text)
    return tmp_path


def test_feed_in_a_subfolder_is_refused(tmp_path):
    path = network_dir(tmp_path, FEED, folder="gtfs/")
    with pytest.raises(RuntimeError) as e:
        peabiru.build_network(path, n_threads=2)
    assert str(e.value) == refused(path, "bus.zip: 6 errors")
    errors = peabiru.get_gtfs_errors(path)
    assert list(errors.type[errors.priority == "HIGH"]) == ["TableInSubdirectoryError"] * 6


def test_feed_without_agency_is_refused(tmp_path):
    feed = {k: v for k, v in FEED.items() if k != "agency.txt"}
    path = network_dir(tmp_path, feed)
    with pytest.raises(RuntimeError) as e:
        peabiru.build_network(path, n_threads=2)
    assert str(e.value) == refused(path, "bus.zip: 1 error")


def test_trips_with_missing_stop_or_route_are_refused(tmp_path):
    feed = dict(FEED)
    feed["trips.txt"] += "R,S,T2\nX,S,T3\n"
    feed["stop_times.txt"] += (
        "T2,14:00:00,14:00:00,A,1\nT2,14:05:00,14:05:00,Z,2\n"
        "T3,14:00:00,14:00:00,A,1\nT3,14:05:00,14:05:00,B,2\n"
    )
    path = network_dir(tmp_path, feed)
    for n in (1, 2):
        with pytest.raises(RuntimeError) as e:
            peabiru.build_network(path, n_threads=n)
        assert str(e.value) == refused(path, "bus.zip: 2 errors")


def test_feeds_with_lower_priority_errors_are_built(tmp_path):
    feed = dict(FEED)
    # a duplicated stop (MEDIUM) and an agency URL that is not one (LOW)
    feed["stops.txt"] += "B,B,-23.5577,-46.6321\n"
    feed["agency.txt"] = feed["agency.txt"].replace("http://a", "www.a")
    path = network_dir(tmp_path, feed)
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        peabiru.build_network(path, n_threads=2)
    priorities = set(peabiru.get_gtfs_errors(path).priority)
    assert {"MEDIUM", "LOW"} <= priorities and "HIGH" not in priorities

def test_clean_feeds_give_no_warning(tmp_path):
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
        peabiru.build_network(network_dir(tmp_path, FEED), n_threads=2)
