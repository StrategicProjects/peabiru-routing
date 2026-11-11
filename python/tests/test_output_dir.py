"""``output_dir``: results written to one CSV file per origin, as r5r writes
them."""

import datetime as dt
import math
import os
import shutil
import subprocess
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
    # one file per origin; the files keep r5r's placeholder cutoff 0
    assert len(os.listdir(tmp_path)) == len(points)
    if decay == "fixed_exponential":
        assert (disk.pop("cutoff") == "0").all()
    same_values(disk, mem)


def test_walk_is_routed_from_the_origins(network, points, tmp_path):
    # more origins than destinations: in memory r5r routes WALK from the
    # destinations; the files are routed, and named, from the origins
    d = points.iloc[:3]
    peabiru.travel_time_matrix(network, points, d, n_threads=2, output_dir=tmp_path)
    disk = read_dir(tmp_path, points["id"])
    assert set(disk["from_id"]) <= set(points["id"])
    assert set(disk["to_id"]) <= set(d["id"])
    assert len(os.listdir(tmp_path)) == disk["from_id"].nunique() > 3


def test_ids_are_quoted_and_files_replaced(network, points, tmp_path):
    p = points.iloc[:3].copy()
    p["id"] = ["a,b", 'q"x', "plain"]
    (tmp_path / "from_plain.csv").write_text("old\nold\nold\n")
    (tmp_path / "other.txt").write_text("kept")
    peabiru.travel_time_matrix(network, p, p, n_threads=2, output_dir=tmp_path)
    assert sorted(os.listdir(tmp_path)) == sorted(
        ["from_a,b.csv", 'from_q"x.csv', "from_plain.csv", "other.txt"]
    )
    text = (tmp_path / "from_a,b.csv").read_text()
    assert '\n"a,b","a,b",0\n' in text.replace(EOL, "\n")
    assert '"q""x"' in (tmp_path / 'from_q"x.csv').read_text()
    assert "old" not in (tmp_path / "from_plain.csv").read_text()
    assert (tmp_path / "other.txt").read_text() == "kept"


def test_output_dir_is_checked_as_r5r_checks_it(network, points, tmp_path):
    def call(d):
        return peabiru.travel_time_matrix(network, points, points, output_dir=d)

    missing = tmp_path / "nope"
    with pytest.raises(
        FileNotFoundError,
        match=f"Assertion on 'output_dir' failed: Directory '{missing}' does not exist.",
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
    with pytest.raises(ValueError, match="ids .* cannot be missing"):
        peabiru.travel_time_matrix(network, p, p, output_dir=tmp_path)
    # a path-like object is returned as given
    assert call(tmp_path) is tmp_path


def test_a_file_that_cannot_be_written_is_an_oserror(network, points, tmp_path):
    p = points.iloc[:2].copy()
    p["id"] = ["no/such/dir", "x"]
    with pytest.raises(FileNotFoundError, match="from_no/such/dir.csv"):
        peabiru.travel_time_matrix(network, p, p, n_threads=1, output_dir=tmp_path)


# r5r writing the same files: needs R with r5r and a JDK 21
R_SCRIPT = r"""
options(java.parameters = "-Xmx2G")
args <- commandArgs(trailingOnly = TRUE)
suppressPackageStartupMessages(library(r5r))
net <- suppressMessages(build_network(args[1]))
pts <- read.csv(file.path(args[1], "spo_points.csv"), colClasses = c(id = "character"))
dep <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")
out <- function(name) { d <- file.path(args[2], name); dir.create(d); d }
travel_time_matrix(net, pts, pts, mode = "WALK", percentiles = c(25, 75),
                   output_dir = out("walk"), n_threads = 2, progress = FALSE)
travel_time_matrix(net, pts, pts[1:5, ], mode = "WALK",
                   output_dir = out("walk_swap"), n_threads = 2, progress = FALSE)
travel_time_matrix(net, pts, pts, mode = c("WALK", "TRANSIT"), departure_datetime = dep,
                   percentiles = c(5, 50, 95), max_trip_duration = 40, seed = 42,
                   output_dir = out("transit"), n_threads = 2, progress = FALSE)
expanded_travel_time_matrix(net, pts[1:8, ], pts, mode = c("WALK", "TRANSIT"),
                            departure_datetime = dep, time_window = 3, breakdown = TRUE,
                            max_trip_duration = 30, seed = 42,
                            output_dir = out("expanded"), n_threads = 2, progress = FALSE)
arrival_travel_time_matrix(net, pts[1:8, ], pts, mode = c("WALK", "TRANSIT"),
                           arrival_datetime = dep + 3600, max_trip_duration = 40, seed = 1,
                           output_dir = out("arrival"), n_threads = 2, progress = FALSE)
accessibility(net, pts, pts, opportunities_colnames = c("jobs", "schools"),
              mode = c("WALK", "TRANSIT"), departure_datetime = dep,
              decay_function = "logistic", cutoffs = c(15, 30), decay_value = 5,
              seed = 42, output_dir = out("access"), n_threads = 2, progress = FALSE)
accessibility(net, pts, pts, opportunities_colnames = "jobs", mode = "CAR",
              decay_function = "fixed_exponential", decay_value = 0.05,
              output_dir = out("access_fixed"), n_threads = 2, progress = FALSE)
"""


def r5r_writes(work: Path) -> Path:
    """Run R_SCRIPT on a copy of the sample data; the directory of r5r's
    files. Skips the test where R, r5r or Java is missing."""
    if os.environ.get("PEABIRU_SKIP_R5R") or shutil.which("Rscript") is None:
        pytest.skip("Rscript not found")
    has = subprocess.run(
        ["Rscript", "-e", 'cat(requireNamespace("r5r", quietly = TRUE))'],
        capture_output=True, text=True,
    )
    if has.stdout.strip() != "TRUE":
        pytest.skip("r5r is not installed")
    data = work / "data"
    shutil.copytree(peabiru.sample_data_path(), data)
    out = work / "r5r"
    out.mkdir()
    script = work / "r5r.R"
    script.write_text(R_SCRIPT)
    run = subprocess.run(
        ["Rscript", str(script), str(data), str(out)], capture_output=True, text=True
    )
    if run.returncode != 0:
        pytest.skip(f"r5r could not run (Java 21 missing?): {run.stderr[-300:]}")
    return out


def test_files_are_byte_identical_to_r5rs(network, points, departure, tmp_path):
    ref = r5r_writes(tmp_path)
    mine = tmp_path / "peabiru"
    mine.mkdir()
    transit = dict(mode=["WALK", "TRANSIT"], departure_datetime=departure)
    calls = {
        "walk": (peabiru.travel_time_matrix, points, points,
                 dict(mode="WALK", percentiles=[25, 75])),
        "walk_swap": (peabiru.travel_time_matrix, points, points.iloc[:5], dict(mode="WALK")),
        "transit": (peabiru.travel_time_matrix, points, points,
                    dict(transit, percentiles=[5, 50, 95], max_trip_duration=40, seed=42)),
        "expanded": (peabiru.expanded_travel_time_matrix, points.iloc[:8], points,
                     dict(transit, time_window=3, breakdown=True, max_trip_duration=30,
                          seed=42)),
        "arrival": (peabiru.arrival_travel_time_matrix, points.iloc[:8], points,
                    dict(mode=["WALK", "TRANSIT"],
                         arrival_datetime=departure + dt.timedelta(hours=1),
                         max_trip_duration=40, seed=1)),
        "access": (peabiru.accessibility, points, points,
                   dict(transit, opportunities_colnames=["jobs", "schools"],
                        decay_function="logistic", cutoffs=[15, 30], decay_value=5,
                        seed=42)),
        "access_fixed": (peabiru.accessibility, points, points,
                         dict(opportunities_colnames="jobs", mode="CAR",
                              decay_function="fixed_exponential", decay_value=0.05)),
    }
    for name, (fun, o, d, kw) in calls.items():
        out = mine / name
        out.mkdir()
        assert fun(network, o, d, n_threads=2, output_dir=out, **kw) is out
        theirs = ref / name
        files = sorted(os.listdir(theirs))
        assert files, name
        assert sorted(os.listdir(out)) == files, name
        for f in files:
            assert (out / f).read_bytes() == (theirs / f).read_bytes(), f"{name}/{f}"
