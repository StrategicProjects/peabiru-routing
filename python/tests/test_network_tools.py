"""find_snap(), street_network_bbox(), street_network_to_sf(),
transit_network_to_sf(), check_transit_availability() and get_gtfs_errors().
The values are the core's, tested there; these check the Python layer."""

import datetime as dt
import os
import shutil

import numpy as np
import pandas as pd
import pytest

import peabiru


def test_find_snap_columns_and_misses(network, points):
    far = pd.DataFrame({"id": ["far"], "lon": [-46.0], "lat": [-23.0]})
    pts = pd.concat([points[["id", "lon", "lat"]].head(3), far], ignore_index=True)
    x = peabiru.find_snap(network, pts)
    assert list(x.columns) == ["point_id", "lat", "lon", "snap_lat", "snap_lon", "distance", "found"]
    assert list(x["point_id"]) == list(pts["id"])
    assert list(x["found"]) == [True, True, True, False]
    assert x.loc[3, ["snap_lat", "snap_lon", "distance"]].isna().all()
    assert ((x["distance"][:3] >= 0) & (x["distance"][:3] < 300)).all()
    assert x["found"].dtype == bool
    assert not peabiru.find_snap(network, points, radius=0)["found"].any()
    assert not peabiru.find_snap(network, points, radius=float("nan"))["found"].any()
    walk = peabiru.find_snap(network, points, mode="WALK")
    car = peabiru.find_snap(network, points, mode="CAR")
    both = walk["found"] & car["found"]
    assert (car["distance"][both] >= walk["distance"][both] - 1e-9).all()


def test_find_snap_checks(network, points):
    with pytest.raises(ValueError, match="radius"):
        peabiru.find_snap(network, points, radius=-1)
    with pytest.raises(ValueError, match="finite"):
        peabiru.find_snap(network, points, radius=float("inf"))
    with pytest.raises(TypeError, match="radius"):
        peabiru.find_snap(network, points, radius="far")
    with pytest.raises(ValueError, match="subset"):
        peabiru.find_snap(network, points, mode="walk")
    with pytest.raises(TypeError, match="peabiru.Network"):
        peabiru.find_snap(None, points)


def test_street_network_bbox(network):
    v = peabiru.street_network_bbox(network, "vector")
    assert list(v) == ["xmin", "ymin", "xmax", "ymax"]
    assert peabiru.street_network_bbox(network, "bbox") == tuple(v.values())
    p = peabiru.street_network_bbox(network)
    assert p.crs.to_epsg() == 4326
    assert tuple(p.total_bounds) == tuple(v.values())
    with pytest.raises(ValueError, match="should be one of"):
        peabiru.street_network_bbox(network, "box")


def test_street_network_to_sf(network):
    s = peabiru.street_network_to_sf(network)
    v, e = s["vertices"], s["edges"]
    assert list(v.columns) == ["index", "park_and_ride", "bike_sharing", "geometry"]
    assert list(e.columns) == [
        "edge_index", "osm_id", "from_vertex", "to_vertex", "street_class", "length",
        "walk", "car", "car_speed", "bicycle", "bicycle_lts", "geometry",
    ]
    assert sorted(v["index"]) == sorted(set(e["from_vertex"]) | set(e["to_vertex"]))
    assert v.crs.to_epsg() == 4326 and e.crs.to_epsg() == 4326
    assert (e.geom_type == "LineString").all()
    assert set(e["street_class"]) <= {"MOTORWAY", "PRIMARY", "SECONDARY", "TERTIARY", "OTHER"}
    # each edge starts at its from_vertex
    xy = dict(zip(v["index"], zip(v.geometry.x, v.geometry.y)))
    for g, f in zip(e.geometry[:20], e["from_vertex"][:20]):
        assert g.coords[0] == xy[f]


def test_transit_network_to_sf(network):
    t = peabiru.transit_network_to_sf(network)
    s, r = t["stops"], t["routes"]
    assert list(s.columns) == ["stop_index", "stop_id", "stop_name", "linked_to_street", "geometry"]
    assert list(r.columns) == [
        "agency_id", "agency_name", "route_id", "long_name", "short_name", "mode", "geometry",
    ]
    assert list(s["stop_index"]) == list(range(len(s)))
    assert s["stop_id"].str.startswith("spo_gtfs:").all()
    assert list(s.geometry.is_empty) == list(~s["linked_to_street"])
    assert len(r) > 0 and (r["agency_name"] == "SPTRANS").all()
    assert not r.geometry.is_empty.any()


def test_check_transit_availability(network):
    d = peabiru.check_transit_availability(network)
    assert list(d.columns) == ["date", "total_services", "active_services", "pct_active"]
    assert d["date"].is_monotonic_increasing
    assert (d["active_services"] > 0).all()
    x = peabiru.check_transit_availability(network, dates=["2019-05-13", dt.date(2019, 5, 19)])
    assert list(x["date"].dt.strftime("%Y-%m-%d")) == ["2019-05-13", "2019-05-19"]
    assert np.array_equal(x["pct_active"], x["active_services"] / x["total_services"])
    one = peabiru.check_transit_availability(network, dates="2019-05-13")
    assert one.equals(x.head(1))
    r = peabiru.check_transit_availability(network, start_date="2019-05-01", end_date="2019-05-31")
    assert len(r) == 31
    assert r.iloc[12].equals(x.iloc[0])
    assert peabiru.check_transit_availability(network, dates="1990-01-01")["active_services"][0] == 0


def test_check_transit_availability_checks(network):
    with pytest.raises(ValueError, match="Incorrect date arguments"):
        peabiru.check_transit_availability(network, dates="2019-05-13", start_date="2019-05-01")
    with pytest.raises(ValueError, match="Incorrect date arguments"):
        peabiru.check_transit_availability(network, end_date="2019-05-01")
    with pytest.raises(ValueError, match="Invalid date format"):
        peabiru.check_transit_availability(network, dates="13/05/2019")
    with pytest.raises(ValueError, match="logically invalid"):
        peabiru.check_transit_availability(network, dates="2019-02-30")
    with pytest.raises(TypeError, match="character strings or Date"):
        peabiru.check_transit_availability(network, dates=[20190513])
    with pytest.raises(ValueError, match="before or the same"):
        peabiru.check_transit_availability(network, start_date="2019-05-31", end_date="2019-05-01")


def test_get_gtfs_errors(network, tmp_path):
    e = peabiru.get_gtfs_errors(network)
    assert list(e.columns) == ["file", "line", "type", "field", "id", "priority"]
    assert set(e["type"]) == {"DuplicateKeyError"}
    assert set(e["file"]) == {"agency", "calendar"}
    assert e["line"].dtype == np.int32
    assert peabiru.get_gtfs_errors(peabiru.sample_data_path()).equals(e)
    shutil.copy(os.path.join(peabiru.sample_data_path(), "spo_osm.pbf"), tmp_path)
    assert len(peabiru.get_gtfs_errors(tmp_path)) == 0
    with pytest.raises(ValueError, match="does not exist"):
        peabiru.get_gtfs_errors(tmp_path / "nope")
    with pytest.raises(TypeError, match="peabiru.Network"):
        peabiru.get_gtfs_errors(1)


def test_get_gtfs_errors_of_a_feed_gtfs_lib_fails_on(tmp_path):
    import zipfile

    src = zipfile.ZipFile(os.path.join(peabiru.sample_data_path(), "spo_gtfs.zip"))
    with zipfile.ZipFile(tmp_path / "bad.zip", "w") as z:
        for n in src.namelist():
            data = src.read(n)
            if n == "calendar_dates.txt":
                data = b"service_id,date,exception_type\nUSD,20190230,1\n"
            z.writestr(n, data)
        if "calendar_dates.txt" not in src.namelist():
            z.writestr("calendar_dates.txt", "service_id,date,exception_type\nUSD,20190230,1\n")
    with pytest.raises(RuntimeError, match="DateTimeParseException"):
        peabiru.get_gtfs_errors(tmp_path)
