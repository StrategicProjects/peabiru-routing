"""Parity of the peabiru Python package with r5r 2.5.0.

The same calls through ``peabiru`` (Python) and ``r5r`` (R), compared value
for value: column names and order, row count and order, the type of every
column, and every value. Doubles are compared exactly: r5r's results are
written with 17 significant digits, which identify a double uniquely, and
parsed back with Python's correctly rounded ``float()``; integer and string
columns are compared as values, missing values by position. Any difference
is reported and makes the script exit with status 1.

Usage (from the repository root; r5r needs a JDK 21)::

    JAVA_HOME=/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home \\
        python/.venv/bin/python bench/compare/py_parity.py [poa] [spo]

Options (environment):
  N_THREADS   threads for both packages (default 4)
  R5R_XMX     JVM heap (default 6G)
  SCENARIOS   comma-separated scenario name prefixes to run (default all)
  WORK_DIR    directory for the networks and the reference files (default a
              fresh temporary directory); it must not exist, so a failed run
              is never compared against stale files
  KEEP        set to 1 to keep the reference files
  R5R_EXTDATA r5r's sample data (default system.file("extdata", package = "r5r"))

The networks are built from r5r's sample data (``extdata/<city>``: the OSM
.pbf and, for the transit network, the GTFS zips; never the elevation
raster) copied to fresh directories, so r5r's network.dat is never reused. The points are r5r's hexgrid of each city (POA 1227, SPO 323),
all to all, except where a scenario says otherwise; R parses the CSV and
writes the points it routes with 17 significant digits, and peabiru routes
those same doubles.
"""

from __future__ import annotations

import csv
import datetime as dt
import json
import os
import random
import shutil
import subprocess
import sys
import tempfile
import time
import warnings
from pathlib import Path
from typing import Any, Dict, List, Optional
from zoneinfo import ZoneInfo

import numpy as np
import pandas as pd

import peabiru

ROOT = Path(__file__).resolve().parents[2]
R_HELPER = Path(__file__).resolve().parent / "py_parity_r5r.R"


def r5r_extdata() -> Path:
    """r5r's sample data (``system.file("extdata", package = "r5r")``),
    or ``R5R_EXTDATA``."""
    env = os.environ.get("R5R_EXTDATA")
    if env:
        return Path(env)
    out = subprocess.run(
        ["Rscript", "-e", 'cat(system.file("extdata", package = "r5r"))'],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    if not out:
        raise SystemExit("r5r is not installed (or set R5R_EXTDATA)")
    return Path(out)
N_POINTS = {"poa": 1227, "spo": 323}
TZ = "America/Sao_Paulo"


def rows(a: int, b: int) -> Dict[str, Any]:
    """Hexgrid rows a..b (1-based, inclusive)."""
    return {"rows": [a, b]}


def when(s: str, tz: str = TZ, naive: bool = True) -> Dict[str, Any]:
    """A datetime: a POSIXct in ``tz`` for R; for Python a naive datetime
    with the same clock time, or (``naive=False``) an aware one in ``tz``."""
    return {"datetime": s, "tz": tz, "naive": naive}


def scenarios(n: int) -> List[Dict[str, Any]]:
    """The scenario list (the breadth of bench/compare/r_parity.R plus
    Python-specific inputs); ``n`` is the number of hexgrid points."""
    allp = [rows(1, n)]
    dep = when("2019-05-13 14:00:00")
    transit = ["WALK", "TRANSIT"]
    pct5 = [5, 25, 50, 75, 95]
    out: List[Dict[str, Any]] = []

    def add(name, fun, network, origins=None, destinations=None, int_ids=False, **args):
        out.append({
            "name": name, "fun": fun, "network": network,
            "origins": origins or allp, "destinations": destinations or allp,
            "int_ids": int_ids, "args": args,
        })

    ttm = "travel_time_matrix"
    add("ttm_transit", ttm, "transit", mode=transit, departure_datetime=dep,
        percentiles=pct5, seed=42)
    add("ttm_transit_tw1", ttm, "transit", mode=transit, departure_datetime=dep,
        time_window=1, percentiles=pct5, seed=42)
    add("ttm_transit_mwt15", ttm, "transit", mode=transit, departure_datetime=dep,
        max_walk_time=15, percentiles=pct5, seed=42)
    add("ttm_transit_v2", ttm, "transit", mode="TRANSIT",
        departure_datetime=when("2019-05-13 07:30:30"), time_window=5,
        percentiles=[1, 99, 50], max_trip_duration=60, max_rides=2,
        draws_per_minute=3, walk_speed=4.5, seed=7)
    for kind in ("street", "transit"):
        for m in ("WALK", "BICYCLE", "CAR"):
            add(f"ttm_{kind}_{m}", ttm, kind, mode=m, departure_datetime=dep)
    add("ttm_street_WALK_pct", ttm, "street", mode="WALK", departure_datetime=dep,
        percentiles=[75, 25], max_trip_duration=30, walk_speed=4.2)
    add("ttm_street_BICYCLE_v2", ttm, "street", mode="BICYCLE", departure_datetime=dep,
        bike_speed=15, max_lts=3, max_bike_time=20)
    add("walk_swap", ttm, "street", destinations=[rows(1, 100)], mode="WALK",
        departure_datetime=dep)

    # edge cases: a point without coordinates and one far from the network
    odd = [rows(1, 30), {"special": "na_point"}, {"special": "far_point"}]
    odd_dest = [rows(1, 60), {"special": "na_point"}, {"special": "far_point"}]
    add("edge_ttm_WALK", ttm, "street", origins=odd, destinations=[rows(1, 60)],
        mode="WALK", departure_datetime=dep)
    add("edge_ttm_transit", ttm, "transit", origins=odd, destinations=odd_dest,
        mode=transit, departure_datetime=dep, seed=42)
    add("edge_access_transit", "accessibility", "transit", origins=odd,
        destinations=odd_dest, opportunities_colnames="jobs", mode=transit,
        departure_datetime=dep, cutoffs=[30, 60], seed=42)

    # accessibility, every setting of bench/r5r/accessibility_settings.csv
    settings = ROOT / "bench" / "r5r" / "accessibility_settings.csv"
    with open(settings, newline="") as f:
        for s in csv.DictReader(f):
            def nums(x: str) -> Optional[List[float]]:
                return None if x == "" else [float(v) for v in x.split(";")]
            pct = nums(s["percentiles"])
            cut = nums(s["cutoffs"])
            dv = nums(s["decay_value"])
            add(f"access_{s['setting']}", "accessibility", s["network"],
                opportunities_colnames=s["opportunities"].split(";"),
                mode=s["mode"].split(";"), departure_datetime=dep,
                time_window=int(s["time_window"]),
                percentiles=[int(v) for v in pct] if pct else None,
                decay_function=s["decay"],
                cutoffs=[int(v) for v in cut] if cut else None,
                decay_value=dv[0] if dv else None,
                max_walk_time="Inf" if s["max_walk_time"] == "Inf" else int(s["max_walk_time"]),
                max_trip_duration=int(s["max_trip_duration"]), seed=42)

    # expanded and arrival matrices
    o50, o200 = [rows(1, 50)], [rows(1, 200)]
    ex, ar = "expanded_travel_time_matrix", "arrival_travel_time_matrix"
    add("expanded_breakdown", ex, "transit", origins=o50, mode=transit,
        departure_datetime=dep, breakdown=True, seed=42)
    add("expanded_nobd", ex, "transit", origins=o50, mode=transit,
        departure_datetime=dep, breakdown=False, seed=42)
    add("expanded_v2", ex, "transit", origins=o50, mode=transit,
        departure_datetime=when("2019-05-13 07:30:30"), time_window=5, breakdown=True,
        max_walk_time=15, max_trip_duration=60, max_rides=2, draws_per_minute=3, seed=7)
    add("arrival", ar, "transit", origins=o200, mode=transit,
        arrival_datetime=when("2019-05-13 16:00:00", naive=False), breakdown=True, seed=42)
    add("arrival_v2", ar, "transit", origins=o200, mode=transit,
        arrival_datetime=when("2019-05-13 08:00:00"), breakdown=False,
        max_walk_time=15, max_trip_duration=60, max_rides=2, draws_per_minute=5, seed=7)
    add("arrival_midnight", ar, "transit", origins=o50, mode=transit,
        arrival_datetime=when("2019-05-14 00:30:00", naive=False), breakdown=True, seed=42)

    # transit mode subsets, bicycle and car access and egress
    add("modes_ttm_bus", ttm, "transit", mode=["WALK", "BUS"], departure_datetime=dep,
        percentiles=pct5, seed=42)
    add("modes_ttm_rail_subway", ttm, "transit", mode=["RAIL", "SUBWAY"],
        departure_datetime=dep, time_window=5, max_rides=2, seed=42)
    add("modes_access_bus", "accessibility", "transit", opportunities_colnames="jobs",
        mode="BUS", departure_datetime=dep, cutoffs=[30, 60], seed=42)
    add("modes_expanded_rail", ex, "transit", origins=o50, mode=["WALK", "RAIL"],
        departure_datetime=dep, breakdown=True, seed=42)
    add("via_ttm_bike", ttm, "transit", mode=["BICYCLE", "TRANSIT"],
        departure_datetime=dep, max_bike_time=20, max_lts=3, bike_speed=14,
        percentiles=[25, 50, 75], seed=42)
    add("via_ttm_car", ttm, "transit", mode=["CAR", "TRANSIT"], departure_datetime=dep,
        max_car_time=15, seed=42)
    add("via_access_car", "accessibility", "transit", opportunities_colnames="jobs",
        mode=["CAR", "BUS"], departure_datetime=dep, cutoffs=[30, 45], max_car_time=10,
        seed=42)
    add("via_expanded_bike", ex, "transit", origins=o50, mode=["BICYCLE", "TRANSIT"],
        departure_datetime=dep, breakdown=True, max_bike_time=15, seed=42)
    add("via_arrival_car", ar, "transit", origins=o50, mode=["CAR", "TRANSIT"],
        arrival_datetime=when("2019-05-13 16:00:00", naive=False), breakdown=True,
        max_car_time=10, seed=42)
    add("egress_ttm_bike", ttm, "transit", mode=transit, mode_egress="BICYCLE",
        departure_datetime=dep, max_bike_time=15, seed=42)
    add("egress_ttm_car", ttm, "transit", mode=["BICYCLE", "TRANSIT"], mode_egress="CAR",
        departure_datetime=dep, max_car_time=10, max_bike_time=10, seed=42)
    add("egress_expanded_car", ex, "transit", origins=o50, mode=transit,
        mode_egress="CAR", departure_datetime=dep, breakdown=True, max_car_time=10,
        time_window=5, seed=42)

    # scenarios: tables written by scenario_files() (r5r's Porto Alegre files
    # as GeoJSON, made-up ways of the network)
    files = SCENARIO_FILES
    if files:
        add("scen_car_scale", ttm, "street", mode="CAR", carspeed_scale=0.7)
        add("scen_car_osm", ttm, "street", mode="CAR", new_carspeeds={"csv": files["osm"]},
            carspeed_scale=0.9)
        add("scen_car_kmh", ttm, "street", mode="CAR", new_carspeeds={"csv": files["kmh"]})
        add("scen_bike_lts", ttm, "street", mode="BICYCLE", new_lts={"csv": files["lts"]})
        add("scen_transit_bike", ttm, "transit", mode=["BICYCLE", "TRANSIT"],
            new_lts={"csv": files["lts"]}, departure_datetime=dep, max_bike_time=20, seed=42)
        add("scen_access_car", "accessibility", "transit", opportunities_colnames="jobs",
            mode=["CAR", "TRANSIT"], carspeed_scale=0.5, departure_datetime=dep,
            cutoffs=[30, 45], max_car_time=15, seed=42)
        add("scen_expanded_bike", ex, "transit", origins=o50, mode=["BICYCLE", "TRANSIT"],
            new_lts={"csv": files["lts"]}, departure_datetime=dep, breakdown=True,
            max_bike_time=15, seed=42)
        if "poly" in files:
            add("scen_poa_poly", ttm, "street", mode="CAR",
                new_carspeeds={"geojson": files["poly"]}, carspeed_scale=1.2)
            add("scen_poa_lines", ttm, "street", mode="BICYCLE",
                new_lts={"geojson": files["lines"]}, max_lts=3)

    # Python-specific inputs: shuffled points, integer ids, a datetime in
    # another time zone, arrival on an aware datetime in UTC
    rng = random.Random(20261109)
    perm_o = rng.sample(range(1, n + 1), min(n, 150))
    perm_d = rng.sample(range(1, n + 1), n)
    add("py_shuffled_ttm_transit", ttm, "transit", origins=[{"index": perm_o}],
        destinations=[{"index": perm_d}], mode=transit,
        departure_datetime=when("2019-05-13 14:20:00", naive=False),
        time_window=7, percentiles=[10, 90], seed=3)
    add("py_shuffled_access_linear", "accessibility", "transit",
        origins=[{"index": perm_o}], destinations=[{"index": perm_d}],
        opportunities_colnames=["population", "healthcare"], mode="TRANSIT",
        departure_datetime=dep, decay_function="linear", cutoffs=[25, 50],
        decay_value=7, percentiles=[50, 20], max_trip_duration=80, seed=11)
    add("py_int_ids_CAR", ttm, "street", origins=[rows(1, 80)],
        destinations=[rows(1, n)], int_ids=True, mode="CAR",
        departure_datetime=dep, max_car_time=25)
    add("py_int_ids_walk_swap", ttm, "street", origins=[rows(1, 120)],
        destinations=[rows(1, 40)], int_ids=True, mode="WALK",
        departure_datetime=dep, walk_speed=5)
    add("py_tz_utc_ttm", ttm, "transit", origins=[rows(1, 100)], mode=transit,
        departure_datetime=when("2019-05-13 15:10:00", tz="UTC", naive=False),
        percentiles=[50], seed=5)
    add("py_tz_utc_arrival", ar, "transit", origins=[rows(1, 40)], mode=transit,
        arrival_datetime=when("2019-05-13 19:00:00", tz="UTC", naive=False),
        max_trip_duration=45, breakdown=True, seed=9)
    add("py_access_BICYCLE_logistic", "accessibility", "street",
        origins=[{"index": perm_o}], opportunities_colnames="schools", mode="BICYCLE",
        decay_function="logistic", cutoffs=[10, 20, 35], decay_value=3.5,
        bike_speed=14, max_lts=4, percentiles=[50])
    return out


def to_py(value: Any) -> Any:
    if isinstance(value, dict) and "csv" in value:
        return pd.read_csv(value["csv"])
    if isinstance(value, dict) and "geojson" in value:
        import geopandas

        return geopandas.read_file(value["geojson"])
    if isinstance(value, dict) and "datetime" in value:
        t = dt.datetime.strptime(value["datetime"], "%Y-%m-%d %H:%M:%S")
        return t if value["naive"] else t.replace(tzinfo=ZoneInfo(value["tz"]))
    if value == "Inf":
        return float("inf")
    return value


def read_exact(path: Path) -> pd.DataFrame:
    """A CSV written by the R helper, every cell as a string."""
    return pd.read_csv(path, dtype=str, keep_default_na=False, na_filter=False)


def read_points(path: Path, int_ids: bool) -> pd.DataFrame:
    raw = read_exact(path)
    df = pd.DataFrame(index=raw.index)
    for c in raw.columns:
        if c == "id":
            df[c] = (
                pd.Series([int(v) for v in raw[c]], dtype=np.int64)
                if int_ids
                else pd.Series(raw[c].tolist(), dtype=object)
            )
        else:
            df[c] = [np.nan if v == "NA" else float(v) for v in raw[c]]
    return df


# the dtype peabiru must give each column, by R type and column name
NULLABLE_INT = ("travel_time_p", "n_rides")


def expected_dtype(column: str, rtype: str) -> str:
    if rtype == "character":
        return "object"
    if rtype == "double":
        return "float64"
    if rtype == "integer":
        return "Int32" if column.startswith(NULLABLE_INT) else "int32"
    return rtype


def compare(py: pd.DataFrame, ref: pd.DataFrame, types: Dict[str, str]) -> List[str]:
    out = []
    if list(py.columns) != list(ref.columns):
        out.append(f"columns: peabiru {list(py.columns)}, r5r {list(ref.columns)}")
    if len(py) != len(ref):
        out.append(f"rows: peabiru {len(py)}, r5r {len(ref)}")
        return out
    for c in [c for c in ref.columns if c in py.columns]:
        rtype = types[c]
        want = expected_dtype(c, rtype)
        if str(py[c].dtype) != want:
            out.append(f"column {c}: dtype {py[c].dtype}, expected {want} (r5r {rtype})")
            continue
        r = ref[c].to_numpy(dtype=object)
        r_na = r == "NA"
        p_na = py[c].isna().to_numpy()
        bad = r_na != p_na
        both = ~r_na & ~p_na
        if rtype == "double":
            rv = np.array([float(v) for v in r[both]], dtype=np.float64)
            pv = py[c].to_numpy(dtype=np.float64)[both]
            # bit for bit (also tells 0.0 from -0.0)
            neq = rv.view(np.int64) != pv.view(np.int64)
        elif rtype == "integer":
            rv = np.array([int(v) for v in r[both]], dtype=np.int64)
            pv = py[c].to_numpy(dtype=np.float64, na_value=np.nan)[both].astype(np.int64)
            neq = rv != pv
        else:
            pv = py[c].to_numpy(dtype=object)[both]
            neq = np.array([a != b for a, b in zip(r[both], pv)], dtype=bool)
        bad[np.flatnonzero(both)[neq]] = True
        if bad.any():
            i = int(np.flatnonzero(bad)[0])
            out.append(
                f"column {c}: {int(bad.sum())} values differ, first at row {i} "
                f"(peabiru {py[c].iloc[i]!r}, r5r {r[i]!r})"
            )
    return out


# per city, the scenario tables (see scenario_files())
SCENARIO_FILES: Dict[str, str] = {}

SCENARIO_R = r"""
a <- commandArgs(TRUE)
suppressPackageStartupMessages(library(sf))
set.seed(11)
ways <- unique(suppressWarnings(peabiru::street_network_to_sf(
  peabiru::build_network(a[1], n_threads = 2)))$edges$osm_id)
write.csv(data.frame(osm_id = sample(ways, 1000, replace = TRUE),
                     max_speed = sample(c(0, 0.25, 0.6, 1.5), 1000, replace = TRUE),
                     speed_type = "scale"), file.path(a[2], "osm.csv"), row.names = FALSE)
write.csv(data.frame(osm_id = sample(ways, 500), max_speed = round(runif(500, 0, 90), 1),
                     speed_type = "km/h"), file.path(a[2], "kmh.csv"), row.names = FALSE)
write.csv(data.frame(osm_id = sample(ways, 1000), lts = sample(1:4, 1000, replace = TRUE)),
          file.path(a[2], "lts.csv"), row.names = FALSE)
if (a[3] == "poa") {
  src <- system.file("extdata/poa", package = "r5r")
  st_write(readRDS(file.path(src, "poa_poly_congestion.rds")),
           file.path(a[2], "poly.geojson"), quiet = TRUE)
  st_write(readRDS(file.path(src, "poa_ls_lts.rds")),
           file.path(a[2], "lines.geojson"), quiet = TRUE)
}
"""


def scenario_files(street_dir: str, out: Path, city: str) -> Dict[str, str]:
    """The tables of the scenarios, written by R (with the peabiru R
    package, which lists the network's ways) into ``out``."""
    out.mkdir()
    script = out / "make.R"
    script.write_text(SCENARIO_R)
    r = subprocess.run(["Rscript", str(script), street_dir, str(out), city])
    if r.returncode != 0:
        raise SystemExit("could not write the scenario tables")
    names = ["osm", "kmh", "lts"] + (["poly", "lines"] if city == "poa" else [])
    ext = {"poly": "geojson", "lines": "geojson"}
    return {n: str(out / f"{n}.{ext.get(n, 'csv')}") for n in names}


def main() -> int:
    cities = sys.argv[1:] or ["poa", "spo"]
    assert all(c in N_POINTS for c in cities), cities
    n_threads = int(os.environ.get("N_THREADS", "4"))
    only = [s for s in os.environ.get("SCENARIOS", "").split(",") if s]
    if os.environ.get("WORK_DIR"):
        work = Path(os.environ["WORK_DIR"])
        work.mkdir(parents=True, exist_ok=False)
    else:
        work = Path(tempfile.mkdtemp(prefix="peabiru-py-parity-"))
    refs = work / "refs"
    extdata = r5r_extdata()

    spec: Dict[str, Any] = {"n_threads": n_threads, "cities": {}, "scenarios": []}
    per_city: Dict[str, List[Dict[str, Any]]] = {}
    for city in cities:
        src = extdata / city
        dirs = {}
        for kind in ("street", "transit"):
            d = work / f"{city}-{kind}"
            d.mkdir()
            shutil.copy(src / f"{city}_osm.pbf", d)
            if kind == "transit":
                for z in sorted(src.glob("*.zip")):
                    shutil.copy(z, d)
            dirs[kind] = str(d)
        spec["cities"][city] = {
            "hexgrid": str(src / f"{city}_hexgrid.csv"), **dirs,
        }
        SCENARIO_FILES.clear()
        SCENARIO_FILES.update(scenario_files(dirs["street"], work / f"{city}-scenarios", city))
        per_city[city] = [
            s for s in scenarios(N_POINTS[city])
            if not only or any(s["name"].startswith(p) for p in only)
        ]
    # the scenario list is the same for every city up to the number of
    # points; the helper takes one list, so run it once per city
    failures: List[str] = []
    summary = []
    for city in cities:
        city_spec = {**spec, "cities": {city: spec["cities"][city]},
                     "scenarios": per_city[city]}
        spec_path = work / f"scenarios_{city}.json"
        spec_path.write_text(json.dumps(city_spec, indent=1))
        print(f"== {city}: r5r references ({len(per_city[city])} scenarios) ==", flush=True)
        t0 = time.perf_counter()
        r = subprocess.run(["Rscript", str(R_HELPER), str(spec_path), str(refs)])
        if r.returncode != 0:
            print(f"the R helper failed (status {r.returncode})", file=sys.stderr)
            return 2
        print(f"r5r: {time.perf_counter() - t0:.1f} s", flush=True)

        nets = {
            kind: peabiru.build_network(spec["cities"][city][kind], n_threads=n_threads)
            for kind in ("street", "transit")
        }
        for s in per_city[city]:
            label = f"{city}/{s['name']}"
            stem = refs / f"{city}__{s['name']}__"
            err = Path(str(stem) + "error.txt")
            if err.exists():
                failures.append(f"{label}: r5r failed: {err.read_text().strip()}")
                print(f"{label:40s} r5r error", flush=True)
                continue
            o = read_points(Path(str(stem) + "origins.csv"), s["int_ids"])
            d = read_points(Path(str(stem) + "destinations.csv"), s["int_ids"])
            args = {k: to_py(v) for k, v in s["args"].items()}
            fun = getattr(peabiru, s["fun"])
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                t0 = time.perf_counter()
                py = fun(nets[s["network"]], o, d, n_threads=n_threads, **args)
                secs = time.perf_counter() - t0
            ref = read_exact(Path(str(stem) + "r5r.csv"))
            with open(str(stem) + "types.csv", newline="") as f:
                types = {row["column"]: row["type"] for row in csv.DictReader(f)}
            why = compare(py, ref, types)
            same = not why
            print(f"{label:40s} rows {len(ref):9d}  peabiru {secs:7.2f} s  identical {same}",
                  flush=True)
            for w in why:
                print(f"    {w}", flush=True)
            if why:
                failures.append(f"{label}: " + "; ".join(why))
            summary.append((city, s["name"], len(ref), same))
            if os.environ.get("KEEP") != "1":
                for suffix in ("r5r.csv",):
                    Path(str(stem) + suffix).unlink()
    n_same = sum(1 for x in summary if x[3])
    print(f"\n{n_same} of {len(summary)} comparisons identical "
          f"({len(failures)} failures)")
    for f in failures:
        print(f"  {f}")
    if failures or os.environ.get("KEEP") == "1":
        print(f"files kept in {work}")
    else:
        shutil.rmtree(work)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
