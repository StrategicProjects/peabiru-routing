"""Repository check: the files ``output_dir`` writes against r5r 2.5.0's.

The same calls through ``peabiru`` (Python) and ``r5r`` (R) on the sample
network shipped with the Python package, each writing one CSV file per
origin; the files are compared byte for byte.

Run with pytest from the repository root, against the installed package
(needs R with r5r 2.5.0 and a JDK 21)::

    JAVA_HOME=/path/to/jdk21 python -m pytest bench/compare/py_output_dir.py

Where R, r5r or Java is missing the check fails: it is never skipped.
"""

import datetime as dt
import os
import shutil
import subprocess
from pathlib import Path

import pytest

import peabiru


@pytest.fixture(scope="module")
def network():
    return peabiru.build_network(peabiru.sample_data_path(), n_threads=2)


@pytest.fixture
def points():
    return peabiru.sample_points()


@pytest.fixture
def departure():
    return dt.datetime(2019, 5, 13, 14, 0)


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
    files. Fails where R, r5r or Java is missing."""
    assert shutil.which("Rscript") is not None, "Rscript not found"
    has = subprocess.run(
        ["Rscript", "-e", 'cat(requireNamespace("r5r", quietly = TRUE))'],
        capture_output=True, text=True,
    )
    assert has.stdout.strip() == "TRUE", "r5r is not installed"
    data = work / "data"
    shutil.copytree(peabiru.sample_data_path(), data)
    out = work / "r5r"
    out.mkdir()
    script = work / "r5r.R"
    script.write_text(R_SCRIPT)
    run = subprocess.run(
        ["Rscript", str(script), str(data), str(out)], capture_output=True, text=True
    )
    assert run.returncode == 0, f"r5r could not run (Java 21 missing?): {run.stderr[-300:]}"
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
    compared = 0
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
            compared += 1
    print(f"{compared} files compared, all identical")
