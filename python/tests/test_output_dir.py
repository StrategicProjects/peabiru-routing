"""``output_dir``: results written to one CSV file per origin."""

import datetime as dt
import math
import os
import re
from pathlib import Path

import numpy as np
import pandas as pd
import pytest

import peabiru

EOL = "\r\n" if os.name == "nt" else "\n"


def read_dir(path, origin_ids):
    """The files read back as one DataFrame of strings, in origin order."""
    parts = []
    for i in origin_ids:
        f = Path(path) / f"from_{i}.csv"
        if f.exists():
            parts.append(pd.read_csv(f, dtype=str, keep_default_na=False))
    return pd.concat(parts, ignore_index=True)


def as_strings(df):
    """An in-memory result as the files write it (NA as an empty field)."""
    out = pd.DataFrame(index=df.index)
    for c in df.columns:
        col = df[c]
        if pd.api.types.is_float_dtype(col):
            s = col.map(lambda v: "" if math.isnan(v) else repr(float(v)))
        else:
            s = col.astype(object).map(lambda v: "" if v is None or v is pd.NA else str(v))
        out[c] = s
    return out


def same_values(disk, mem):
    """Disk and memory hold the same rows; doubles compared as numbers."""
    assert list(disk.columns) == list(mem.columns)
    assert len(disk) == len(mem)
    mem = as_strings(mem)
    for c in disk.columns:
        a, b = disk[c].tolist(), mem[c].tolist()
        if any("." in v for v in a):
            a = [float(v) if v else None for v in a]
            b = [float(v) if v else None for v in b]
        assert a == b, c


@pytest.fixture
def transit(departure):
    return dict(mode=["WALK", "TRANSIT"], departure_datetime=departure, seed=42, n_threads=2)


def test_street_matrix_files_hold_the_result(network, points, tmp_path):
    ret = peabiru.travel_time_matrix(
        network, points, points, mode="BICYCLE", percentiles=[75, 25],
        n_threads=2, output_dir=str(tmp_path),
    )
    assert ret == str(tmp_path)
    mem = peabiru.travel_time_matrix(
        network, points, points, mode="BICYCLE", percentiles=[75, 25], n_threads=2
    )
    names = sorted(os.listdir(tmp_path))
    assert names == sorted(f"from_{i}.csv" for i in mem["from_id"].unique())
    same_values(read_dir(tmp_path, points["id"]), mem)
    text = (tmp_path / names[0]).read_bytes().decode()
    assert text.startswith("from_id,to_id,travel_time_p25,travel_time_p75" + EOL)
    assert text.endswith(EOL)


def test_transit_matrix_files_hold_the_result(network, points, tmp_path, transit):
    kw = dict(transit, percentiles=[5, 50, 95], max_trip_duration=40)
    peabiru.travel_time_matrix(network, points, points, output_dir=tmp_path, **kw)
    mem = peabiru.travel_time_matrix(network, points, points, **kw)
    assert mem["travel_time_p95"].isna().any()
    same_values(read_dir(tmp_path, points["id"]), mem)


@pytest.mark.parametrize("breakdown", [False, True])
def test_expanded_files_hold_the_result(network, points, tmp_path, transit, breakdown):
    kw = dict(transit, time_window=3, breakdown=breakdown, max_trip_duration=30)
    o = points.iloc[:6]
    peabiru.expanded_travel_time_matrix(network, o, points, output_dir=tmp_path, **kw)
    mem = peabiru.expanded_travel_time_matrix(network, o, points, **kw)
    assert mem["total_time"].isna().any()
    same_values(read_dir(tmp_path, o["id"]), mem)


def test_arrival_files_hold_the_result(network, points, tmp_path, departure):
    kw = dict(mode="TRANSIT", arrival_datetime=departure + dt.timedelta(hours=1),
              max_trip_duration=40, seed=1, n_threads=2)
    o = points.iloc[:4]
    peabiru.arrival_travel_time_matrix(network, o, points, output_dir=tmp_path, **kw)
    mem = peabiru.arrival_travel_time_matrix(network, o, points, **kw)
    same_values(read_dir(tmp_path, o["id"]), mem)


@pytest.mark.parametrize("decay", ["step", "fixed_exponential"])
def test_accessibility_files_hold_the_result(network, points, tmp_path, transit, decay):
    kw = dict(transit, opportunities_colnames=["jobs", "schools"], decay_function=decay,
              cutoffs=[30, 15] if decay == "step" else None,
              decay_value=0.05 if decay != "step" else None)
    peabiru.accessibility(network, points, points, output_dir=tmp_path, **kw)
    mem = peabiru.accessibility(network, points, points, **kw)
    disk = read_dir(tmp_path, points["id"])
    # one file per origin; the files keep the placeholder cutoff 0
    assert len(os.listdir(tmp_path)) == len(points)
    if decay == "fixed_exponential":
        assert (disk.pop("cutoff") == "0").all()
    same_values(disk, mem)


def test_walk_is_routed_from_the_origins(network, points, tmp_path):
    # more origins than destinations: in memory WALK is routed from the
    # destinations; the files are routed, and named, from the origins
    d = points.iloc[:3]
    peabiru.travel_time_matrix(network, points, d, n_threads=2, output_dir=tmp_path)
    disk = read_dir(tmp_path, points["id"])
    assert set(disk["from_id"]) <= set(points["id"])
    assert set(disk["to_id"]) <= set(d["id"])
    assert len(os.listdir(tmp_path)) == disk["from_id"].nunique() > 3


def test_ids_are_quoted_and_files_replaced(network, points, tmp_path):
    p = points.iloc[:3].copy()
    # a double quote cannot be in a file name on Windows
    quoted = "q x" if os.name == "nt" else 'q"x'
    p["id"] = ["a,b", quoted, "plain"]
    (tmp_path / "from_plain.csv").write_text("old\nold\nold\n")
    (tmp_path / "other.txt").write_text("kept")
    peabiru.travel_time_matrix(network, p, p, n_threads=2, output_dir=tmp_path)
    assert sorted(os.listdir(tmp_path)) == sorted(
        ["from_a,b.csv", f"from_{quoted}.csv", "from_plain.csv", "other.txt"]
    )
    text = (tmp_path / "from_a,b.csv").read_text()
    assert '\n"a,b","a,b",0\n' in text.replace(EOL, "\n")
    if os.name != "nt":
        assert '"q""x"' in (tmp_path / 'from_q"x.csv').read_text()
    assert "old" not in (tmp_path / "from_plain.csv").read_text()
    assert (tmp_path / "other.txt").read_text() == "kept"


def test_output_dir_is_checked(network, points, tmp_path):
    def call(d):
        return peabiru.travel_time_matrix(network, points, points, output_dir=d)

    missing = tmp_path / "nope"
    with pytest.raises(
        FileNotFoundError,
        match=re.escape(f"Assertion on 'output_dir' failed: Directory '{missing}' does not exist."),
    ):
        call(str(missing))
    with pytest.raises(FileNotFoundError, match="Directory '' does not exist"):
        call("")
    f = tmp_path / "file"
    f.write_text("x")
    with pytest.raises(NotADirectoryError, match="Directory expected, but file in place"):
        call(f)
    with pytest.raises(TypeError, match="output_dir"):
        call(1)
    with pytest.raises(TypeError, match="output_dir"):
        call([str(tmp_path)])
    p = points.iloc[:2].copy()
    p["id"] = [None, "x"]
    with pytest.raises(ValueError, match="id.* cannot be missing"):
        peabiru.travel_time_matrix(network, p, p, output_dir=tmp_path)
    # a path-like object is returned as given
    assert call(tmp_path) is tmp_path


def test_a_file_that_cannot_be_written_is_an_oserror(network, points, tmp_path):
    p = points.iloc[:2].copy()
    p["id"] = ["y", "x"]
    # a directory where the file would go
    (tmp_path / "from_y.csv").mkdir()
    with pytest.raises(OSError, match="from_y.csv"):
        peabiru.travel_time_matrix(network, p, p, n_threads=1, output_dir=tmp_path)


@pytest.mark.parametrize("bad", ["a/b", "../x", "a\\b"])
def test_ids_that_cannot_name_a_file_are_refused(network, points, tmp_path, bad):
    """The file names are built from the ids: an id with a path separator
    would name a file outside the directory, so it is refused before
    anything is routed or written."""
    pts = points.head(3).copy()
    pts.loc[pts.index[0], "id"] = bad
    with pytest.raises(ValueError, match="file names are built from the ids"):
        peabiru.travel_time_matrix(network, pts, pts, mode="WALK", output_dir=tmp_path)
    assert list(tmp_path.iterdir()) == []
    with pytest.raises(ValueError, match="file names are built from the ids"):
        peabiru.trip_options(network, pts, pts, mode="WALK", output_dir=tmp_path)
