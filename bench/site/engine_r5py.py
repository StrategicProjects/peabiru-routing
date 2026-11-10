"""One benchmark task with r5py, called by bench/site/run.py.

`--probe` prints the r5py version (and starts nothing heavy). Otherwise prints
one JSON line {"build_s", "times", "rows", "version"}.

r5py takes the same parameters under its own names (departure_time_window,
max_time, speed_walking, max_public_transport_rides,
max_bicycle_traffic_stress, ...). Differences that cannot be removed:
r5py has no `seed` (frequency-based networks give random results) and always
uses 60 Monte Carlo draws, where r5r uses time_window x draws_per_minute = 50;
r5py has no accessibility() and its detailed itineraries are a different
computation from r5r's expanded matrix, so those two tasks are skipped.

The R5 jar comes from R5PY_JAR (default: r5py's own download). JVM heap:
R5PY_MAX_MEMORY (default 6G).
"""

import json
import os
import sys
import time


def arg(name):
    i = sys.argv.index("--" + name)
    return sys.argv[i + 1]


def available_cpus():
    """The cores the JVM will use: the CPU affinity mask, capped by a cgroup
    CPU quota (docker run --cpus), as Java's availableProcessors() does."""
    n = len(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else os.cpu_count()
    try:
        quota, period = open("/sys/fs/cgroup/cpu.max").read().split()[:2]
        if quota != "max":
            n = min(n, max(1, -(-int(quota) // int(period))))
    except (OSError, ValueError):
        pass
    return n


def main():
    jar = os.environ.get("R5PY_JAR")
    sys.argv[1:1] = ["--max-memory", os.environ.get("R5PY_MAX_MEMORY", "6G")]
    if jar:
        sys.argv[1:1] = ["--r5-classpath", jar]
    import importlib.metadata

    version = f"r5py {importlib.metadata.version('r5py')}"
    if "--probe" in sys.argv:
        if jar:
            r5 = os.path.basename(jar)
        else:  # the jar r5py downloads by default, named in its classpath module
            import importlib.util
            import re

            spec = importlib.util.find_spec("r5py")
            src = open(os.path.join(spec.submodule_search_locations[0], "util", "classpath.py")).read()
            m = re.search(r"releases/download/v([^/]+)/", src)
            r5 = m.group(1) if m else "?"
        print(f"{version} (R5 {r5})")
        return
    task = arg("task")
    threads = int(arg("threads"))
    cores = available_cpus()
    if threads != cores:
        print(json.dumps({"skip": "r5py does not let the caller set the number of threads "
                                  f"(it uses all {cores} available cores)"}))
        return
    if task in ("accessibility", "expanded"):
        print(json.dumps({"skip": f"r5py has no equivalent of r5r's {task} computation"}))
        return
    import datetime
    import pathlib

    import geopandas as gpd
    import pandas as pd
    import r5py

    P = json.loads(arg("params"))
    reps = int(arg("reps"))
    d = pathlib.Path(arg("dir"))
    pts = pd.read_csv(arg("points"), dtype={"id": str})
    points = gpd.GeoDataFrame(pts[["id"]], geometry=gpd.points_from_xy(pts.lon, pts.lat), crs="EPSG:4326")
    # r5py caches built networks; drop the cache so that the build is timed
    from r5py.util.config import Config

    for f in list(Config().CACHE_DIR.glob("*.transport_network")) + list(Config().CACHE_DIR.glob("*.mapdb*")):
        f.unlink()
    t0 = time.perf_counter()
    net = r5py.TransportNetwork(next(d.glob("*.pbf")), sorted(str(p) for p in d.glob("*.zip")))
    build_s = time.perf_counter() - t0
    dep = datetime.datetime.fromisoformat(f"{P['date']}T{P['time']}")
    modes = {"ttm_walk": [r5py.TransportMode.WALK], "ttm_bicycle": [r5py.TransportMode.BICYCLE],
             "ttm_car": [r5py.TransportMode.CAR],
             "ttm_transit": [r5py.TransportMode.TRANSIT, r5py.TransportMode.WALK]}[task]
    kw = dict(departure=dep, departure_time_window=datetime.timedelta(minutes=P["time_window"]),
              percentiles=P["percentiles"], transport_modes=modes,
              max_time=datetime.timedelta(minutes=P["max_trip_duration"]),
              speed_walking=P["walk_speed"], speed_cycling=P["bike_speed"],
              max_public_transport_rides=P["max_rides"],
              max_bicycle_traffic_stress=P["max_lts"])

    def call():
        return r5py.TravelTimeMatrix(net, origins=points, destinations=points, **kw)

    res = call()  # warm-up
    times = []
    for _ in range(reps):
        t = time.perf_counter()
        res = call()
        times.append(time.perf_counter() - t)
    rows = int(res.iloc[:, 2].notna().sum())
    if "--result" in sys.argv:  # the matrix for run.py's comparison with peabiru
        out = res.iloc[:, :3].dropna()
        out.columns = ["from_id", "to_id", "travel_time"]
        out.to_csv(arg("result"), index=False)
    print(json.dumps({"build_s": build_s, "times": times, "rows": rows, "version": version}))


if __name__ == "__main__":
    main()
