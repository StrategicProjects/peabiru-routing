"""One benchmark task with the peabiru Python package, called by bench/site/run.py.

`--probe` prints the package version, or exits non-zero with the reason the
slot has to be skipped (module not importable, functions not there yet).
Otherwise prints one JSON line {"build_s", "times", "rows", "version"}.

The calls assume the Python API mirrors the R one (r5r's function and
argument names); a task the package does not support is reported as skipped.
"""

import json
import statistics  # noqa: F401
import sys
import time

FUNCS = ["build_network", "travel_time_matrix", "accessibility", "expanded_travel_time_matrix"]


def fail(msg, code=3):
    print(msg)
    sys.exit(code)


def arg(name):
    i = sys.argv.index("--" + name)
    return sys.argv[i + 1]


def main():
    try:
        import peabiru
    except Exception as e:  # noqa: BLE001
        fail(f"the peabiru Python module cannot be imported ({type(e).__name__}: {e})")
    missing = [f for f in FUNCS if not hasattr(peabiru, f)]
    version = f"peabiru {getattr(peabiru, '__version__', '?')} (Python)"
    if "--probe" in sys.argv:
        if "build_network" in missing or "travel_time_matrix" in missing:
            fail("the peabiru Python module has no build_network()/travel_time_matrix() yet")
        print(version)
        return
    import datetime
    import zoneinfo

    import pandas as pd

    P = json.loads(arg("params"))
    task, threads, reps = arg("task"), int(arg("threads")), int(arg("reps"))
    points = pd.read_csv(arg("points"), dtype={"id": str})
    dep = datetime.datetime.fromisoformat(f"{P['date']}T{P['time']}").replace(
        tzinfo=zoneinfo.ZoneInfo(P["tz"]))
    t0 = time.perf_counter()
    net = peabiru.build_network(arg("dir"), n_threads=threads)
    build_s = time.perf_counter() - t0
    common = dict(departure_datetime=dep, max_trip_duration=P["max_trip_duration"],
                  walk_speed=P["walk_speed"], bike_speed=P["bike_speed"], max_lts=P["max_lts"],
                  n_threads=threads)
    transit = dict(common, mode=["WALK", "TRANSIT"], time_window=P["time_window"],
                   max_rides=P["max_rides"], draws_per_minute=P["draws_per_minute"], seed=P["seed"])
    a = P["accessibility"]
    calls = {
        "ttm_walk": lambda: peabiru.travel_time_matrix(net, points, points, mode="WALK", **common),
        "ttm_bicycle": lambda: peabiru.travel_time_matrix(net, points, points, mode="BICYCLE", **common),
        "ttm_car": lambda: peabiru.travel_time_matrix(net, points, points, mode="CAR", **common),
        "ttm_transit": lambda: peabiru.travel_time_matrix(net, points, points,
                                                          percentiles=P["percentiles"], **transit),
        "accessibility": lambda: peabiru.accessibility(
            net, points, points, opportunities_colnames=[a["opportunities"]],
            decay_function=a["decay_function"], cutoffs=a["cutoffs"],
            percentiles=P["percentiles"], **transit),
        "expanded": lambda: peabiru.expanded_travel_time_matrix(
            net, points.iloc[: P["expanded"]["origins"]], points, breakdown=True, **transit),
    }
    if task not in calls:
        fail(f"unknown task {task}", 2)
    try:
        res = calls[task]()
    except (TypeError, AttributeError, NotImplementedError) as e:
        print(json.dumps({"skip": f"{task} not supported by this version: {e}"}))
        return
    times = []
    for _ in range(reps):
        t = time.perf_counter()
        res = calls[task]()
        times.append(time.perf_counter() - t)
    print(json.dumps({"build_s": build_s, "times": times, "rows": len(res), "version": version}))


if __name__ == "__main__":
    main()
