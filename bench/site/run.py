#!/usr/bin/env python3
"""Benchmarks for the documentation site: peabiru against r5r and r5py.

Runs the same tasks, on the same inputs and with the same parameters, on every
engine that is available, and writes one tidy results file,
site/data/benchmarks.json (metadata + one record per engine, city, task and
thread count), plus the same records as site/data/benchmarks.csv.
bench/site/charts.py turns it into the charts and tables of the sites.

    python3 bench/site/run.py                       # every engine, both cities
    python3 bench/site/run.py --engines peabiru-cli,r5r --cities spo --threads 10

Engines (each one is skipped cleanly, with the reason recorded, when it
cannot run):

  peabiru-cli  the `peabiru` binary (cargo build --release -p peabiru-cli, or
               PEABIRU_BIN)
  peabiru-r    the R package (library from PEABIRU_R_LIB, else R's default)
  peabiru-py   the Python package (interpreter from PEABIRU_PYTHON, else
               python/.venv/bin/python, else python3)
  r5r          r5r with R5 7.6 on a JDK 21 (JAVA_HOME, default Homebrew's
               openjdk@21), JVM heap R5R_XMX (default 6G)
  r5py         r5py (interpreter from R5PY_PYTHON), R5 jar from R5PY_JAR

Tasks (r5r's names and defaults; departure 2019-05-13 14:00 America/Sao_Paulo,
time_window 10, percentile 50, max_trip_duration 120, walk 3.6 km/h, bike
12 km/h, max_lts 2, max_rides 3, draws_per_minute 5, seed 42; points are r5r's
hexagonal grid of the city, all to all):

  build          build_network() on a directory with the .pbf and the GTFS zips
  ttm_walk       travel_time_matrix(mode = "WALK")
  ttm_bicycle    travel_time_matrix(mode = "BICYCLE")
  ttm_car        travel_time_matrix(mode = "CAR")
  ttm_transit    travel_time_matrix(mode = c("WALK", "TRANSIT"))
  accessibility  accessibility(mode = c("WALK", "TRANSIT"), "jobs",
                 decay_function = "step", cutoffs = c(30, 60))
  expanded       expanded_travel_time_matrix(mode = c("WALK", "TRANSIT"),
                 breakdown = TRUE), first 50 origins x all destinations

Timing: each task runs in a fresh process that builds the network and then
runs the task once untimed (warm-up: JIT for the JVM, caches for everyone)
and REPS times timed; the record keeps the median, min and max of the timed
runs. The peabiru CLI times build and run inside the binary; it runs REPS + 1
processes of build + one run and drops the first one (warm file cache). Build time is the median over the task processes of that
city. Peak RSS is the maximum resident set size of the task's process
(network build + warm-up + timed runs), from /usr/bin/time.
"""

import argparse
import datetime
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent

CITIES = {
    "poa": {"name": "Porto Alegre", "gtfs": ["poa_eptc.zip", "poa_trensurb.zip"]},
    "spo": {"name": "São Paulo", "gtfs": ["spo.zip"]},
}
TASKS = ["ttm_walk", "ttm_bicycle", "ttm_car", "ttm_transit", "accessibility", "expanded"]
ENGINES = ["peabiru-cli", "peabiru-r", "peabiru-py", "r5r", "r5py"]

PARAMS = {
    "date": "2019-05-13",
    "time": "14:00:00",
    "tz": "America/Sao_Paulo",
    "time_window": 10,
    "percentiles": [50],
    "max_trip_duration": 120,
    "max_walk_time": "Inf",
    "walk_speed": 3.6,
    "bike_speed": 12,
    "max_lts": 2,
    "max_rides": 3,
    "draws_per_minute": 5,
    "seed": 42,
    "accessibility": {"opportunities": "jobs", "decay_function": "step", "cutoffs": [30, 60]},
    "expanded": {"origins": 50, "breakdown": True},
}

TASK_LABELS = {
    "build": "network build",
    "ttm_walk": "travel-time matrix, walk",
    "ttm_bicycle": "travel-time matrix, bicycle",
    "ttm_car": "travel-time matrix, car",
    "ttm_transit": "travel-time matrix, walk + transit",
    "accessibility": "accessibility, walk + transit",
    "expanded": "expanded matrix, walk + transit (50 origins)",
}


def log(msg):
    print(msg, file=sys.stderr, flush=True)


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def timed(cmd, env=None, cwd=None):
    """Run cmd under /usr/bin/time; return (stdout, stderr, rc, peak RSS MiB)."""
    flag = "-l" if sys.platform == "darwin" else "-v"
    p = sh(["/usr/bin/time", flag] + cmd, env=env, cwd=cwd)
    rss = None
    m = re.search(r"(\d+)\s+maximum resident set size", p.stderr)
    if m:  # macOS: bytes
        rss = int(m.group(1)) / 2**20
    m = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", p.stderr)
    if m:  # GNU time: KiB
        rss = int(m.group(1)) / 1024
    return p.stdout, p.stderr, p.returncode, rss


def r5r_extdata():
    p = sh(["Rscript", "-e", 'cat(system.file("extdata", package = "r5r"))'])
    path = p.stdout.strip()
    if p.returncode != 0 or not path:
        sys.exit("r5r is needed for its sample data (extdata/poa, extdata/spo)")
    return Path(path)


def prepare_inputs(work, extdata, city):
    """A directory with the city's .pbf and GTFS zips (no elevation raster),
    and the hexgrid points; r5r gets its own copy because it writes into it."""
    src = extdata / city
    files = [f"{city}_osm.pbf"] + CITIES[city]["gtfs"]
    dirs = {}
    for who in ("shared", "r5r", "r5py"):
        d = work / who / city
        if d.exists():
            shutil.rmtree(d)
        d.mkdir(parents=True)
        for f in files:
            shutil.copy2(src / f, d / f)
        dirs[who] = d
    dedupe_gtfs(dirs["r5py"])
    points = work / f"{city}_hexgrid.csv"
    shutil.copy2(src / f"{city}_hexgrid.csv", points)
    return dirs, points


def dedupe_gtfs(d):
    """r5py (R5 7.5.1-r5py) refuses a feed with duplicated primary keys, which
    r5r's R5 7.6 only logs; the Sao Paulo sample repeats identical rows in
    calendar.txt and agency.txt. Give r5py copies with exact duplicate lines
    removed (the first occurrence is kept): the same data, read once."""
    import zipfile
    for z in d.glob("*.zip"):
        tmp = z.with_suffix(".tmp")
        changed = False
        with zipfile.ZipFile(z) as zin, zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                data = zin.read(info.filename)
                if info.filename.endswith(".txt"):
                    seen, keep = set(), []
                    for line in data.splitlines(keepends=True):
                        key = line.rstrip(b"\r\n")
                        if key and key in seen:
                            changed = True
                            continue
                        seen.add(key)
                        keep.append(line)
                    data = b"".join(keep)
                zout.writestr(info.filename, data)
        if changed:
            tmp.replace(z)
        else:
            tmp.unlink()


# ---------------------------------------------------------------- engines


class Skip(Exception):
    pass


def peabiru_cli_bin():
    b = os.environ.get("PEABIRU_BIN")
    if b:
        return b
    p = sh(["cargo", "build", "-q", "--release", "-p", "peabiru-cli"], cwd=ROOT)
    if p.returncode != 0:
        raise Skip("cargo build failed: " + p.stderr.strip()[-300:])
    meta = json.loads(sh(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=ROOT).stdout)
    return str(Path(meta["target_directory"]) / "release" / "peabiru")


def run_cli(binary, city, dirs, points, task, threads, reps, outdir):
    """One task with the CLI. Returns dict(build_s=[..], times=[..], rows, rss)."""
    d, pts = str(dirs["shared"]), str(points)
    out = str(outdir / f"cli_{city}_{task}_{threads}.csv")
    P = PARAMS
    common = ["--date", P["date"], "--time", P["time"], "--tw", str(P["time_window"]),
              "--max-trip", str(P["max_trip_duration"]), "--max-rides", str(P["max_rides"]),
              "--draws", str(P["draws_per_minute"]), "--seed", str(P["seed"]),
              "--threads", str(threads)]
    res = {"build_s": [], "times": [], "rows": None, "rss": None}

    def parse(stdout):
        b = re.search(r"build ([0-9.]+)s", stdout)
        r = re.search(r"route ([0-9.]+)s", stdout)
        n = re.search(r"(?:rows|reached) (\d+)", stdout)
        return (float(b.group(1)) if b else None, float(r.group(1)) if r else None,
                int(n.group(1)) if n else None)

    if task.startswith("ttm_") and task != "ttm_transit":
        mode = task[4:].upper()
        # ttm repeats in-process and reports the median; run reps+1 so that
        # the first (warm-up) run is excluded from a separate invocation
        cmds = [[binary, "ttm", d, pts, mode, out, str(threads), "1"] for _ in range(reps + 1)]
        full_build = False
    elif task == "ttm_transit":
        cmds = [[binary, "transit", d, pts, out, "--percentiles", "50",
                 "--max-walk", str(P["max_trip_duration"])] + common + ["--repeats", "1"]
                for _ in range(reps + 1)]
        full_build = True
    elif task == "accessibility":
        a = P["accessibility"]
        acc_common = [c for c in common]
        cmds = [[binary, "accessibility", d, pts, out, "--opportunities", a["opportunities"],
                 "--mode", "WALK,TRANSIT", "--decay", a["decay_function"],
                 "--cutoffs", ",".join(map(str, a["cutoffs"])), "--percentiles", "50",
                 "--max-walk", "Inf", "--walk-speed", str(P["walk_speed"]),
                 "--bike-speed", str(P["bike_speed"]), "--max-lts", str(P["max_lts"])]
                + acc_common + ["--repeats", "1"] for _ in range(reps + 1)]
        full_build = True
    elif task == "expanded":
        cmds = [[binary, "expanded", d, pts, out, "--breakdown", "--origins",
                 str(P["expanded"]["origins"]), "--max-walk", str(P["max_trip_duration"])]
                + common for _ in range(reps + 1)]
        full_build = True
    else:
        raise Skip(f"unknown task {task}")
    rss = 0.0
    for i, cmd in enumerate(cmds):
        stdout, stderr, rc, r = timed(cmd)
        if rc != 0:
            raise Skip(f"peabiru {task} failed: {stderr.strip()[-300:]}")
        b, t, n = parse(stdout)
        rss = max(rss, r or 0)
        if full_build and b is not None:
            res["build_s"].append(b)
        if i > 0:  # the first process is the warm-up (file cache)
            res["times"].append(t)
        res["rows"] = n
    res["rss"] = rss
    res["rss_scope"] = "one process: build + one run"
    return res


def run_script(cmd, env):
    stdout, stderr, rc, rss = timed(cmd, env=env)
    lines = [ln for ln in stdout.splitlines() if ln.startswith("{")]
    if rc != 0 or not lines:
        # drop /usr/bin/time's report, keep the engine's own error
        err = re.split(r"\n\s*[0-9.]+ real ", "\n" + stderr)[0].strip()
        msg = (err or stdout.strip())[-400:]
        if rc == 3:  # the engine script's "skip" exit code
            raise Skip(msg)
        raise Skip(f"failed (rc {rc}): {msg}")
    out = json.loads(lines[-1])
    if out.get("skip"):
        raise Skip(out["skip"])
    return {"build_s": [out["build_s"]], "times": out["times"], "rows": out.get("rows"),
            "rss": rss, "version": out.get("version"),
            "rss_scope": "one process: build + warm-up + timed runs"}


def script_args(city, dirs_key, dirs, points, task, threads, reps):
    return ["--city", city, "--dir", str(dirs[dirs_key]), "--points", str(points),
            "--task", task, "--threads", str(threads), "--reps", str(reps),
            "--params", json.dumps(PARAMS)]


# ---------------------------------------------------------------- main


def machine_info():
    info = {"os": platform.platform(), "python": platform.python_version()}
    if sys.platform == "darwin":
        def sysctl(k):
            return sh(["sysctl", "-n", k]).stdout.strip()
        info.update(model=sysctl("hw.model"), cpu=sysctl("machdep.cpu.brand_string"),
                    cores=int(sysctl("hw.ncpu") or 0),
                    perf_cores=int(sysctl("hw.perflevel0.physicalcpu") or 0),
                    ram_gib=round(int(sysctl("hw.memsize") or 0) / 2**30, 1),
                    macos=sh(["sw_vers", "-productVersion"]).stdout.strip())
    else:
        info.update(cpu=platform.processor(), cores=os.cpu_count())
    load = os.getloadavg()
    info["load_avg_at_start"] = [round(x, 2) for x in load]
    return info


def versions():
    v = {}
    git = sh(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT)
    v["peabiru_commit"] = git.stdout.strip() or None
    dirty = sh(["git", "status", "--porcelain", "--", "crates"], cwd=ROOT)
    v["peabiru_core_dirty"] = bool(dirty.stdout.strip())
    v["rustc"] = sh(["rustc", "--version"]).stdout.strip() or None
    v["R"] = sh(["Rscript", "-e", "cat(R.version.string)"]).stdout.strip() or None
    return v


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--engines", default=",".join(ENGINES))
    ap.add_argument("--cities", default="poa,spo")
    ap.add_argument("--tasks", default=",".join(TASKS))
    ap.add_argument("--threads", default="1,10", help="comma-separated thread counts")
    ap.add_argument("--reps", type=int, default=3)
    ap.add_argument("--work", default=str(ROOT / "bench" / "data" / "site"),
                    help="scratch directory for inputs and outputs (gitignored)")
    ap.add_argument("--out", default=str(ROOT / "site" / "data" / "benchmarks.json"))
    a = ap.parse_args()
    engines = a.engines.split(",")
    cities = a.cities.split(",")
    tasks = a.tasks.split(",")
    threads = [int(t) for t in a.threads.split(",")]
    work = Path(a.work)
    work.mkdir(parents=True, exist_ok=True)
    outdir = work / "outputs"
    outdir.mkdir(exist_ok=True)

    extdata = r5r_extdata()
    started = datetime.datetime.now().astimezone()
    meta = {
        "generated": started.isoformat(timespec="seconds"),
        "machine": machine_info(),
        "versions": versions(),
        "parameters": PARAMS,
        "tasks": {t: TASK_LABELS[t] for t in ["build"] + tasks},
        "cities": {},
        "engines": {},
        "methodology": __doc__.split("Timing:")[1].strip(),
    }
    records = []

    inputs = {}
    for c in cities:
        dirs, points = prepare_inputs(work, extdata, c)
        with open(points) as f:
            n = sum(1 for _ in f) - 1
        inputs[c] = (dirs, points)
        meta["cities"][c] = {"name": CITIES[c]["name"], "points": n,
                             "gtfs": CITIES[c]["gtfs"], "pbf": f"{c}_osm.pbf"}

    env_base = dict(os.environ)
    java_home = os.environ.get(
        "JAVA_HOME_21", "/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home")

    for engine in engines:
        status = {"status": "ok", "reason": None, "version": None}
        meta["engines"][engine] = status
        try:
            env = dict(env_base)
            if engine == "peabiru-cli":
                binary = peabiru_cli_bin()
                status["version"] = sh([binary, "--version"]).stdout.strip()
                runner = lambda c, t, n: run_cli(binary, c, inputs[c][0], inputs[c][1], t, n, a.reps, outdir)  # noqa: E731
            elif engine in ("peabiru-r", "r5r"):
                if engine == "r5r":
                    env["JAVA_HOME"] = java_home
                    env.setdefault("R5R_XMX", "6G")
                elif os.environ.get("PEABIRU_R_LIB"):
                    env["R_LIBS"] = os.environ["PEABIRU_R_LIB"]
                script = [shutil.which("Rscript") or "Rscript", str(HERE / "engine_r.R"), "--engine", engine]
                key = "r5r" if engine == "r5r" else "shared"
                runner = lambda c, t, n, script=script, env=env, key=key: run_script(  # noqa: E731
                    script + script_args(c, key, inputs[c][0], inputs[c][1], t, n, a.reps), env)
            elif engine in ("peabiru-py", "r5py"):
                if engine == "peabiru-py":
                    py = os.environ.get("PEABIRU_PYTHON")
                    venv = ROOT / "python" / ".venv" / "bin" / "python"
                    py = py or (str(venv) if venv.exists() else sys.executable)
                    script = HERE / "engine_peabiru_py.py"
                    key = "shared"
                else:
                    py = os.environ.get("R5PY_PYTHON")
                    if not py:
                        raise Skip("R5PY_PYTHON is not set (an interpreter with r5py installed)")
                    env["JAVA_HOME"] = java_home
                    script = HERE / "engine_r5py.py"
                    key = "r5py"
                probe = sh([py, str(script), "--probe"], env=env)
                if probe.returncode != 0:
                    raise Skip((probe.stdout + probe.stderr).strip()[-400:] or "probe failed")
                status["version"] = probe.stdout.strip().splitlines()[-1]
                runner = lambda c, t, n, py=py, script=script, env=env, key=key: run_script(  # noqa: E731
                    [py, str(script)] + script_args(c, key, inputs[c][0], inputs[c][1], t, n, a.reps), env)
            else:
                raise Skip(f"unknown engine {engine}")
        except Skip as e:
            status.update(status="skipped", reason=str(e))
            log(f"[{engine}] skipped: {e}")
            continue

        for c in cities:
            for n in threads:
                builds = []
                for t in tasks:
                    log(f"[{engine}] {c} {t} threads={n}")
                    rec = {"engine": engine, "city": c, "task": t, "threads": n}
                    try:
                        r = runner(c, t, n)
                    except Skip as e:
                        rec.update(status="skipped", note=str(e))
                        log(f"   skipped: {e}")
                        records.append(rec)
                        continue
                    if r.get("version") and not status["version"]:
                        status["version"] = r["version"]
                    times = [x for x in r["times"] if x is not None]
                    builds += [x for x in r["build_s"] if x is not None]
                    rec.update(status="ok", seconds=round(statistics.median(times), 4),
                               seconds_min=round(min(times), 4), seconds_max=round(max(times), 4),
                               reps=len(times), rows=r.get("rows"),
                               peak_rss_mib=round(r["rss"], 1) if r.get("rss") else None,
                               rss_scope=r.get("rss_scope"))
                    log(f"   {rec['seconds']} s  rows {rec['rows']}  rss {rec['peak_rss_mib']} MiB")
                    records.append(rec)
                if builds:
                    records.append({"engine": engine, "city": c, "task": "build", "threads": n,
                                    "status": "ok", "seconds": round(statistics.median(builds), 4),
                                    "seconds_min": round(min(builds), 4),
                                    "seconds_max": round(max(builds), 4), "reps": len(builds),
                                    "rows": None, "peak_rss_mib": None,
                                    "rss_scope": None})

    meta["finished"] = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
    meta["machine"]["load_avg_at_end"] = [round(x, 2) for x in os.getloadavg()]
    out = Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with open(out, "w") as f:
        json.dump({"meta": meta, "records": records}, f, indent=1, ensure_ascii=False)
        f.write("\n")
    cols = ["engine", "city", "task", "threads", "status", "seconds", "seconds_min", "seconds_max",
            "reps", "rows", "peak_rss_mib", "rss_scope", "note"]
    with open(out.with_suffix(".csv"), "w") as f:
        f.write(",".join(cols) + "\n")
        for r in records:
            vals = []
            for k in cols:
                v = r.get(k)
                s = "" if v is None else str(v)
                if any(ch in s for ch in ',"\n'):
                    s = '"' + s.replace('"', '""') + '"'
                vals.append(s)
            f.write(",".join(vals) + "\n")
    log(f"wrote {out} and {out.with_suffix('.csv')}")


if __name__ == "__main__":
    tempfile.tempdir = None
    main()
