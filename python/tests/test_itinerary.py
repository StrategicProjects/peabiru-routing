import numpy as np
import pandas as pd
import pytest

import peabiru

COLS = [
    "from_id", "from_lat", "from_lon", "to_id", "to_lat", "to_lon", "option",
    "departure_time", "total_duration", "total_distance", "segment", "mode",
    "segment_duration", "wait", "distance", "route",
]


def test_columns_types_and_geometry(network, points, departure):
    gpd = pytest.importorskip("geopandas")
    it = peabiru.detailed_itineraries(
        network, points.iloc[:5], points.iloc[5:10], mode="WALK",
        departure_datetime=departure, n_threads=2,
    )
    assert isinstance(it, gpd.GeoDataFrame)
    assert list(it.columns) == COLS + ["geometry"]
    assert it.crs == "EPSG:4326"
    assert len(it) > 0
    for c in ["option", "total_distance", "segment", "distance"]:
        assert it[c].dtype == np.int32
    assert set(it["departure_time"]) == {"14:00:00"}
    assert (it["mode"] == "WALK").all()
    assert set(it.geometry.geom_type) == {"LineString"}
    df = peabiru.detailed_itineraries(
        network, points.iloc[:5], points.iloc[5:10], mode="WALK",
        departure_datetime=departure, drop_geometry=True, n_threads=2,
    )
    assert type(df) is pd.DataFrame
    pd.testing.assert_frame_equal(df, pd.DataFrame(it.drop(columns="geometry")))


def test_pairs_all_to_all_and_output_dir(network, points, departure, tmp_path):
    a = peabiru.detailed_itineraries(
        network, points.iloc[:3], points.iloc[3:5], mode="CAR", departure_datetime=departure,
        all_to_all=True, drop_geometry=True,
    )
    pairs = list(dict.fromkeys(zip(a["from_id"], a["to_id"])))
    want = [(o, d) for o in points["id"][:3] for d in points["id"][3:5]]
    assert pairs == [(str(o), str(d)) for o, d in want]
    with pytest.warns(UserWarning, match="'origins' was expanded"):
        peabiru.detailed_itineraries(
            network, points.iloc[:1], points.iloc[3:5], mode="CAR",
            departure_datetime=departure, drop_geometry=True,
        )
    with pytest.raises(ValueError, match="same number of rows"):
        peabiru.detailed_itineraries(
            network, points.iloc[:3], points.iloc[3:5], mode="CAR", departure_datetime=departure,
        )
    out = peabiru.detailed_itineraries(
        network, points.iloc[:3], points.iloc[3:6], mode="WALK", departure_datetime=departure,
        drop_geometry=True, output_dir=tmp_path,
    )
    assert out == tmp_path
    files = sorted(p.name for p in tmp_path.iterdir())
    assert files and all(f.startswith("from_") and "_to_" in f for f in files)
    # with the geometry: the WKT text last, quoted
    geo = tmp_path / "geo"
    geo.mkdir()
    peabiru.detailed_itineraries(
        network, points.iloc[:3], points.iloc[3:6], mode="WALK", departure_datetime=departure,
        output_dir=geo,
    )
    for f in geo.iterdir():
        lines = f.read_text().splitlines()
        assert lines[0].endswith(",route,geometry")
        assert all(line.endswith(')"') and ',"LINESTRING (' in line for line in lines[1:])


def test_unsupported_options(network, points, departure):
    def di(**kw):
        return peabiru.detailed_itineraries(
            network, points.iloc[:1], points.iloc[1:2], departure_datetime=departure, **kw
        )

    with pytest.raises(peabiru.UnsupportedError):
        di(mode="WALK", osm_link_ids=True)
    with pytest.raises(ValueError, match="drop_geometry"):
        di(mode="WALK", osm_link_ids=True, drop_geometry=True)
    with pytest.raises(ValueError, match="Must be 0"):
        di(mode="WALK", suboptimal_minutes=5)
    with pytest.raises(peabiru.UnsupportedError):
        di(mode="WALK", fare_structure={})
    # the sample's GTFS is frequency-based, which r5r refuses here
    with pytest.raises(ValueError, match="frequencies"):
        di(mode=["WALK", "TRANSIT"])
