#!/usr/bin/env python3
"""Benchmarks for the documentation site: peabiru among the packages people
use for travel-time matrices on OpenStreetMap and GTFS.

Runs the same tasks, on the same inputs and with the same parameters (where
a package lets them be set), on every engine that is available, compares
each engine's matrices with peabiru's, and writes one tidy results file,
<out>/benchmarks.json (metadata + one record per engine, city, task and
thread count), plus the same records as <out>/benchmarks.csv.
bench/site/charts.py turns it into the charts and tables of the sites.

    python3 bench/site/run.py                       # every engine, both cities
    python3 bench/site/run.py --engines peabiru-cli,r5r --cities spo --threads 10
    python3 bench/site/run.py --out /tmp/bench      # a test run: site/data untouched
    python3 bench/site/run.py --resume              # go on after an interrupted run

The results file is rewritten as each engine finishes, so a run that is
interrupted keeps the engines it completed, and --resume runs the others.

Engines (each one is skipped cleanly, with the reason recorded, when it
cannot run):

  same model (r5r's results):
  peabiru-cli  the `peabiru` binary (cargo build --release -p peabiru-cli, or
               PEABIRU_BIN)
  peabiru-r    the R package (library from PEABIRU_R_LIB, else R's default)
  peabiru-py   the Python package (interpreter from PEABIRU_PYTHON, else
               python/.venv/bin/python, else python3)
  r5r          r5r with R5 7.6 on a JDK 21 (JAVA_HOME_21, else JAVA_HOME,
               else Homebrew's openjdk@21), JVM heap R5R_XMX (default 6G)
  r5py         r5py (interpreter from R5PY_PYTHON), R5 jar from R5PY_JAR

  other packages (street matrices; their own network models, so their
  results differ from peabiru's and the differences are measured):
  dodgr        R: osmdata_sc + weight_streetnet + dodgr_times (the .pbf is
               converted once, untimed, with osmium to the .osm XML that
               dodgr_streetnet_sc() would download)
  cppRouting   R: the same dodgr graph, cpp_contract + get_distance_matrix
  networkx     Python: pyrosm graph, single_source_dijkstra_path_length
  igraph       Python: the same graph, Graph.distances
  pandana      Python: the same graph, Network.shortest_path_lengths
               (R engines use R_LIBS / BENCH_R_LIB; Python engines the
               interpreter in BENCH_PYTHON, else python3)

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
processes of build + one run and drops the first one (warm file cache). Build
time is the median over the task processes of that city (for the other
packages it includes reading the .pbf and building the mode's graph). Peak
RSS is the maximum resident set size of the task's process (network build +
warm-up + timed runs), from /usr/bin/time. An engine that would need more
than about ten minutes per run is run on the first N origins (all
destinations); the record says so in origins_used and the time is the time
of that subset, never extrapolated. Single-threaded engines (networkx,
igraph) are measured once, with the smallest thread count, and that record
is repeated for the other thread counts with measured_threads set.

Comparison: every engine that can write its matrix does so for its first
thread count; run.py compares it with peabiru's matrix (the CLI, same
parameters, same origins) and records, over the pairs of the origins used,
the pairs reached by both, by only one of them, and the median and 90th
percentile of the absolute difference in minutes (comparison field).
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
ENGINES = ["peabiru-cli", "peabiru-r", "peabiru-py", "r5r", "r5py",
           "dodgr", "cppRouting", "networkx", "igraph", "pandana"]
R_STREET = ("dodgr", "cppRouting")
PY_STREET = ("networkx", "igraph", "pandana")
SINGLE_THREADED = ("networkx", "igraph")
# Engines that would need more than about ten minutes per run on all origins
# get the first N origins (all destinations). Sized on an Apple M2 Pro;
# override with --origins-limit engine:city:N,...
ORIGIN_LIMITS = {}
# The matrices compared with peabiru's (the reference is peabiru's CLI)
COMPARED_TASKS = ("ttm_walk", "ttm_bicycle", "ttm_car", "ttm_transit")
STREET_TASKS = ("ttm_walk", "ttm_bicycle", "ttm_car")

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
    if os.environ.get("R5R_EXTDATA"):
        return Path(os.environ["R5R_EXTDATA"])
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
            "rss": rss, "version": out.get("version"), "notes": out.get("notes") or [],
            "origins_used": out.get("origins_used"), "threads_used": out.get("threads_used"),
            "rss_scope": "one process: build + warm-up + timed runs"}


def script_args(city, dirs_key, dirs, points, task, threads, reps, origins=None, result=None):
    a = ["--city", city, "--dir", str(dirs[dirs_key]), "--points", str(points),
         "--task", task, "--threads", str(threads), "--reps", str(reps),
         "--params", json.dumps(PARAMS)]
    if origins:
        a += ["--origins", str(origins)]
    if result:
        a += ["--result", str(result)]
    return a


# ---------------------------------------------------------------- comparison


def read_matrix(path, origins=None):
    """{(from_id, to_id): minutes} from a from_id,to_id,travel_time* CSV."""
    import csv
    m = {}
    with open(path, newline="") as f:
        rd = csv.reader(f)
        head = next(rd)
        i_f, i_t = head.index("from_id"), head.index("to_id")
        i_v = next(i for i, h in enumerate(head) if h.startswith("travel_time"))
        for row in rd:
            v = row[i_v]
            if v in ("", "NA", "nan"):
                continue
            if origins is not None and row[i_f] not in origins:
                continue
            m[(row[i_f], row[i_t])] = float(v)
    return m


def quantile(xs, q):
    """Linear-interpolation quantile of a sorted list (R's type 7)."""
    if not xs:
        return None
    h = (len(xs) - 1) * q
    lo = int(h)
    hi = min(lo + 1, len(xs) - 1)
    return xs[lo] + (h - lo) * (xs[hi] - xs[lo])


def compare(ref_path, res_path, origin_ids):
    """Pairs reached by peabiru and the engine, over the engine's origins."""
    ref = read_matrix(ref_path, origin_ids)
    res = read_matrix(res_path, origin_ids)
    both = ref.keys() & res.keys()
    diffs = sorted(res[k] - ref[k] for k in both)
    absd = sorted(abs(d) for d in diffs)
    r = lambda x: None if x is None else round(x, 2)  # noqa: E731
    return {"reference": "peabiru-cli", "origins": len(origin_ids),
            "pairs_peabiru": len(ref), "pairs_engine": len(res), "pairs_both": len(both),
            "only_peabiru": len(ref.keys() - res.keys()),
            "only_engine": len(res.keys() - ref.keys()),
            "median_abs_diff_min": r(quantile(absd, 0.5)),
            "p90_abs_diff_min": r(quantile(absd, 0.9)),
            "median_diff_min": r(quantile(diffs, 0.5)),
            "within_1min_pct": r(100 * sum(1 for d in absd if d < 1) / len(absd)) if absd else None}


class Reference:
    """peabiru's matrices (CLI, untimed), computed once per city and task."""

    def __init__(self, inputs, outdir):
        self.inputs, self.outdir, self.cache = inputs, outdir, {}
        self.binary, self.error = None, None

    def get(self, city, task):
        if (city, task) in self.cache:
            return self.cache[(city, task)]
        path = None
        try:
            if self.binary is None and self.error is None:
                self.binary = peabiru_cli_bin()
            if self.binary:
                path = self.outdir / f"ref_{city}_{task}.csv"
                d, pts = str(self.inputs[city][0]["shared"]), str(self.inputs[city][1])
                P = PARAMS
                if task == "ttm_transit":
                    cmd = [self.binary, "transit", d, pts, str(path), "--date", P["date"],
                           "--time", P["time"], "--tw", str(P["time_window"]), "--percentiles", "50",
                           "--max-walk", str(P["max_trip_duration"]),
                           "--max-trip", str(P["max_trip_duration"]),
                           "--max-rides", str(P["max_rides"]), "--draws", str(P["draws_per_minute"]),
                           "--seed", str(P["seed"]), "--threads", "0"]
                else:
                    cmd = [self.binary, "ttm", d, pts, task[4:].upper(), str(path), "0", "1"]
                p = sh(cmd)
                if p.returncode != 0:
                    raise Skip(p.stderr.strip()[-300:])
        except Skip as e:
            self.error, path = str(e), None
        self.cache[(city, task)] = path
        return path



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
        cpu = platform.processor() or None
        try:
            for line in open("/proc/cpuinfo"):
                if line.startswith("model name"):
                    cpu = line.split(":", 1)[1].strip()
                    break
        except OSError:
            pass
        info.update(cpu=cpu, cores=os.cpu_count())
        try:
            for line in open("/proc/meminfo"):
                if line.startswith("MemTotal"):
                    info["ram_gib"] = round(int(line.split()[1]) / 2**20, 1)
        except OSError:
            pass
        if hasattr(os, "sched_getaffinity"):
            info["cpus_available"] = len(os.sched_getaffinity(0))
        # limits of a container (docker run --cpus / --memory), cgroup v2
        try:
            quota, period = open("/sys/fs/cgroup/cpu.max").read().split()[:2]
            info["cgroup_cpus"] = None if quota == "max" else round(int(quota) / int(period), 2)
        except (OSError, ValueError):
            pass
        try:
            mem = open("/sys/fs/cgroup/memory.max").read().strip()
            info["cgroup_memory_gib"] = None if mem == "max" else round(int(mem) / 2**30, 1)
        except (OSError, ValueError):
            pass
    if os.environ.get("PEABIRU_BENCH_CONTAINER"):
        info["container"] = os.environ["PEABIRU_BENCH_CONTAINER"]
    load = os.getloadavg()
    info["load_avg_at_start"] = [round(x, 2) for x in load]
    return info


def versions():
    v = {}
    rev = sh(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT)
    v["peabiru_commit"] = rev.stdout.strip() or os.environ.get("PEABIRU_COMMIT") or None
    dirty = sh(["git", "status", "--porcelain", "--", "crates"], cwd=ROOT)
    v["peabiru_core_dirty"] = bool(dirty.stdout.strip())
    v["rustc"] = sh(["rustc", "--version"]).stdout.strip() or None
    v["R"] = sh(["Rscript", "-e", "cat(R.version.string)"]).stdout.strip() or None
    java = os.environ.get("JAVA_HOME_21") or os.environ.get("JAVA_HOME")
    if java and Path(java, "bin", "java").exists():
        p = sh([str(Path(java, "bin", "java")), "-version"])
        v["java"] = (p.stderr.strip().splitlines() or [None])[0]
    # pinned package sets of the container (bench/docker), when present
    for key, env in (("cran_snapshot", "CRAN_SNAPSHOT"), ("python_requirements", "BENCH_REQUIREMENTS")):
        if os.environ.get(env):
            val = os.environ[env]
            if key == "python_requirements" and Path(val).exists():
                val = [ln.strip() for ln in open(val) if ln.strip() and not ln.startswith("#")]
            v[key] = val
    return v


ENGINE_TIER = {"peabiru-cli": "same", "peabiru-r": "same", "peabiru-py": "same", "r5r": "same",
               "r5py": "same"}


def setup_engine(engine, inputs, env_base, java_home, reps, status):
    """Return runner(city, task, threads, origins, result) for an engine, or raise Skip."""
    env = dict(env_base)
    if engine == "peabiru-cli":
        binary = peabiru_cli_bin()
        status["version"] = sh([binary, "--version"]).stdout.strip()
        outdir = inputs["_outdir"]
        return lambda c, t, n, o, res: run_cli(binary, c, inputs[c][0], inputs[c][1], t, n, reps, outdir)
    if engine in ("peabiru-r", "r5r"):
        if engine == "r5r":
            env["JAVA_HOME"] = java_home
            env.setdefault("R5R_XMX", "6G")
        elif os.environ.get("PEABIRU_R_LIB"):
            env["R_LIBS"] = os.environ["PEABIRU_R_LIB"]
        script = [shutil.which("Rscript") or "Rscript", str(HERE / "engine_r.R"), "--engine", engine]
        key = "r5r" if engine == "r5r" else "shared"
        return lambda c, t, n, o, res: run_script(
            script + script_args(c, key, inputs[c][0], inputs[c][1], t, n, reps, result=res), env)
    if engine in R_STREET:
        if os.environ.get("BENCH_R_LIB"):
            env["R_LIBS"] = os.environ["BENCH_R_LIB"]
        script = [shutil.which("Rscript") or "Rscript", str(HERE / "engine_r_streets.R"),
                  "--engine", engine]
        probe = sh(script + ["--probe"], env=env)
        if probe.returncode != 0:
            raise Skip((probe.stdout + probe.stderr).strip()[-400:] or "probe failed")
        status["version"] = probe.stdout.strip().splitlines()[-1]

        def run_r_streets(c, t, n, o, res):
            osm = highways_osm(inputs, c)
            r = run_script(script + script_args(c, "shared", inputs[c][0], inputs[c][1], t, n,
                                                reps, o, res) + ["--osm", str(osm)], env)
            r["notes"] = r.get("notes", []) + [inputs["_osm_note"][c]]
            return r
        return run_r_streets
    if engine in PY_STREET:
        py = os.environ.get("BENCH_PYTHON") or sys.executable
        script = [py, str(HERE / "engine_py_streets.py"), "--engine", engine]
        probe = sh(script + ["--probe"], env=env)
        if probe.returncode != 0:
            raise Skip((probe.stdout + probe.stderr).strip()[-400:] or "probe failed")
        status["version"] = probe.stdout.strip().splitlines()[-1]
        return lambda c, t, n, o, res: run_script(
            script + script_args(c, "shared", inputs[c][0], inputs[c][1], t, n, reps, o, res), env)
    if engine in ("peabiru-py", "r5py"):
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
        return lambda c, t, n, o, res: run_script(
            [py, str(script)] + script_args(c, key, inputs[c][0], inputs[c][1], t, n, reps, result=res),
            env)
    raise Skip(f"unknown engine {engine}")


def highways_osm(inputs, city):
    """The city's ways with a highway tag as .osm XML, for osmdata_sc(): what
    dodgr_streetnet_sc() would download from Overpass. Made once per city with
    osmium, outside the timed processes (osmium alone peaks at about 1 GiB,
    which is not dodgr's memory)."""
    dirs = inputs[city][0]
    out = dirs["shared"].parent.parent / "osm" / f"{city}_highways.osm"
    notes = inputs.setdefault("_osm_note", {})
    if out.exists() and city in notes:
        return out
    out.parent.mkdir(parents=True, exist_ok=True)
    if not shutil.which("osmium"):
        raise Skip("osmium (osmium-tool) is not on the PATH: it converts the .pbf for osmdata")
    pbf = dirs["shared"] / f"{city}_osm.pbf"
    t0 = datetime.datetime.now()
    p = sh(["osmium", "tags-filter", str(pbf), "w/highway", "-o", str(out), "-f", "osm",
            "--overwrite"])
    if p.returncode != 0:
        raise Skip("osmium tags-filter failed: " + p.stderr.strip()[-300:])
    secs = (datetime.datetime.now() - t0).total_seconds()
    notes[city] = (f"input: osmium tags-filter w/highway to .osm XML, made once "
                   f"before the runs ({secs:.1f} s, not in the build time)")
    return out


def parse_limits(text):
    lim = dict(ORIGIN_LIMITS)
    for item in filter(None, (text or "").split(",")):
        e, c, n = item.split(":")
        lim[(e, c)] = int(n)
    return lim


def resume_from(path, meta):
    """The records of an interrupted run and its metadata, to go on from: the
    engines it finished are not run again. Results measured with other
    parameters, versions, inputs or limits are refused, not mixed."""
    if not path.exists():
        return meta, []
    with open(path) as f:
        prev = json.load(f)
    old = prev["meta"]
    loads = ("load_avg_at_start", "load_avg_at_end")
    for key in ("parameters", "versions", "cities", "tasks", "machine"):
        a, b = ({k: v for k, v in m.get(key, {}).items() if k not in loads} for m in (old, meta))
        if a != b:
            sys.exit(f"--resume: {path} was measured with other {key} ({a} then, {b} now); "
                     "start again without --resume or with another --out")
    done = {e: s for e, s in old["engines"].items() if s["status"] == "ok"}
    meta["resumed"] = old.get("resumed", []) + [meta["generated"]]  # when each later part began
    meta["generated"] = old["generated"]
    meta["machine"]["load_avg_at_start"] = old["machine"].get("load_avg_at_start")
    meta["engines"] = done
    return meta, [r for r in prev["records"] if r["engine"] in done]


def write_outputs(outdir, meta, records, quiet=False):
    """benchmarks.json and benchmarks.csv, engines in the order of ENGINES
    whatever the order they ran in."""
    order = {e: i for i, e in enumerate(ENGINES)}
    rank = lambda e: order.get(e, len(order))  # noqa: E731
    meta = dict(meta, engines={e: meta["engines"][e] for e in sorted(meta["engines"], key=rank)})
    records = sorted(records, key=lambda r: rank(r["engine"]))
    outdir.mkdir(parents=True, exist_ok=True)
    out = outdir / "benchmarks.json"
    with open(out, "w") as f:
        json.dump({"meta": meta, "records": records}, f, indent=1, ensure_ascii=False)
        f.write("\n")
    cols = ["engine", "city", "task", "threads", "status", "seconds", "seconds_min", "seconds_max",
            "reps", "rows", "peak_rss_mib", "rss_scope", "origins_used", "origins_total",
            "threads_used", "measured_threads", "cmp_pairs_both", "cmp_only_peabiru",
            "cmp_only_engine", "cmp_median_abs_diff_min", "cmp_p90_abs_diff_min",
            "cmp_median_diff_min", "cmp_within_1min_pct", "notes", "note"]
    with open(out.with_suffix(".csv"), "w") as f:
        f.write(",".join(cols) + "\n")
        for r in records:
            flat = dict(r)
            cmp_ = r.get("comparison") or {}
            for k in ("pairs_both", "only_peabiru", "only_engine", "median_abs_diff_min",
                      "p90_abs_diff_min", "median_diff_min", "within_1min_pct"):
                flat["cmp_" + k] = cmp_.get(k)
            if isinstance(flat.get("notes"), list):
                flat["notes"] = "; ".join(flat["notes"])
            vals = []
            for k in cols:
                v = flat.get(k)
                s = "" if v is None else str(v)
                if any(ch in s for ch in ',"\n'):
                    s = '"' + s.replace('"', '""') + '"'
                vals.append(s)
            f.write(",".join(vals) + "\n")
    if not quiet:
        log(f"wrote {out} and {out.with_suffix('.csv')}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--engines", default=",".join(ENGINES))
    ap.add_argument("--cities", default="poa,spo")
    ap.add_argument("--tasks", default=",".join(TASKS))
    ap.add_argument("--threads", default="1,10", help="comma-separated thread counts")
    ap.add_argument("--reps", type=int, default=3)
    ap.add_argument("--origins-limit", default="",
                    help="engine:city:N,... run that engine on the first N origins of the city")
    ap.add_argument("--no-compare", action="store_true", help="skip the comparison with peabiru")
    ap.add_argument("--resume", action="store_true",
                    help="go on from the benchmarks.json in --out: the engines it has are kept, "
                         "not run again")
    ap.add_argument("--work", default=str(ROOT / "bench" / "data" / "site"),
                    help="scratch directory for inputs and outputs (gitignored)")
    ap.add_argument("--out", default=str(ROOT / "site" / "data"),
                    help="directory for benchmarks.json and benchmarks.csv (default site/data)")
    a = ap.parse_args()
    engines = a.engines.split(",")
    cities = a.cities.split(",")
    tasks = a.tasks.split(",")
    threads = [int(t) for t in a.threads.split(",")]
    limits = parse_limits(a.origins_limit)
    work = Path(a.work)
    work.mkdir(parents=True, exist_ok=True)
    outdir = work / "outputs"
    outdir.mkdir(exist_ok=True)
    out = Path(a.out)
    if out.suffix == ".json":  # the old --out <file.json>
        out = out.parent

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

    inputs = {"_outdir": outdir}
    for c in cities:
        dirs, points = prepare_inputs(work, extdata, c)
        with open(points) as f:
            ids = [ln.split(",", 1)[0] for ln in f][1:]
        inputs[c] = (dirs, points, ids)
        meta["cities"][c] = {"name": CITIES[c]["name"], "points": len(ids),
                             "gtfs": CITIES[c]["gtfs"], "pbf": f"{c}_osm.pbf"}
    reference = Reference(inputs, outdir)
    if a.resume:
        meta, records = resume_from(out / "benchmarks.json", meta)
    kept = set(meta["engines"])

    env_base = dict(os.environ)
    java_home = (os.environ.get("JAVA_HOME_21") or os.environ.get("JAVA_HOME")
                 or "/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home")

    for engine in engines:
        if engine in kept:
            log(f"[{engine}] kept from the interrupted run")
            continue
        status = {"status": "ok", "reason": None, "version": None,
                  "tier": ENGINE_TIER.get(engine, "other")}
        meta["engines"][engine] = status
        try:
            runner = setup_engine(engine, inputs, env_base, java_home, a.reps, status)
        except Skip as e:
            status.update(status="skipped", reason=str(e))
            log(f"[{engine}] skipped: {e}")
            write_outputs(out, meta, records, quiet=True)
            continue

        compared = set()
        measured = {}  # (city, task) -> record of a single-threaded engine
        for c in cities:
            npts = len(inputs[c][2])
            n_orig = min(limits.get((engine, c), npts), npts)
            for n in threads:
                builds = []
                for t in tasks:
                    rec = {"engine": engine, "city": c, "task": t, "threads": n}
                    if engine in SINGLE_THREADED and (c, t) in measured:
                        src, src_builds = measured[(c, t)]
                        builds += src_builds
                        rec.update({k: v for k, v in src.items()
                                    if k not in ("threads", "comparison")})
                        rec["measured_threads"] = src["threads"]
                        records.append(rec)
                        continue
                    if engine in R_STREET + PY_STREET and t not in STREET_TASKS:
                        rec.update(status="skipped", note=f"{engine} routes on a street graph "
                                   "only; this task needs a public transport model")
                        records.append(rec)
                        continue
                    log(f"[{engine}] {c} {t} threads={n}"
                        + (f" origins={n_orig}/{npts}" if n_orig < npts else ""))
                    want_cmp = (not a.no_compare and t in COMPARED_TASKS
                                and (c, t) not in compared and engine != "peabiru-cli")
                    res_path = outdir / f"res_{engine}_{c}_{t}.csv" if want_cmp else None
                    if res_path and res_path.exists():
                        res_path.unlink()
                    try:
                        r = runner(c, t, n, n_orig if n_orig < npts else None, res_path)
                    except Exception as e:  # noqa: BLE001 - Skip, or a bug: record it, go on
                        msg = str(e) if isinstance(e, Skip) else f"error: {type(e).__name__}: {e}"
                        rec.update(status="skipped", note=msg)
                        log(f"   skipped: {msg}")
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
                    used = r.get("origins_used")
                    if used and used < npts:
                        rec.update(origins_used=used, origins_total=npts)
                    if r.get("threads_used") is not None:
                        rec["threads_used"] = r["threads_used"]
                    if r.get("notes"):
                        rec["notes"] = r["notes"]
                    if res_path and res_path.exists():
                        compared.add((c, t))
                        ref = reference.get(c, t)
                        if ref:
                            ids = set(inputs[c][2][: used or npts])
                            rec["comparison"] = compare(ref, res_path, ids)
                            log(f"   vs peabiru: {rec['comparison']}")
                        elif reference.error:
                            rec["comparison_note"] = f"no reference: {reference.error}"
                    log(f"   {rec['seconds']} s  rows {rec['rows']}  rss {rec['peak_rss_mib']} MiB")
                    records.append(rec)
                    if engine in SINGLE_THREADED:
                        measured[(c, t)] = (rec, [x for x in r["build_s"] if x is not None])
                if builds:
                    records.append({"engine": engine, "city": c, "task": "build", "threads": n,
                                    "status": "ok", "seconds": round(statistics.median(builds), 4),
                                    "seconds_min": round(min(builds), 4),
                                    "seconds_max": round(max(builds), 4), "reps": len(builds),
                                    "rows": None, "peak_rss_mib": None,
                                    "rss_scope": None})
                    if engine in SINGLE_THREADED and n != threads[0]:
                        records[-1]["measured_threads"] = threads[0]
        # what is done so far, for --resume if the run is interrupted
        write_outputs(out, meta, records, quiet=True)

    meta["finished"] = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
    meta["machine"]["load_avg_at_end"] = [round(x, 2) for x in os.getloadavg()]
    write_outputs(out, meta, records)


if __name__ == "__main__":
    tempfile.tempdir = None
    main()
