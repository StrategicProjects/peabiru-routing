#!/usr/bin/env python3
"""Charts and tables for the documentation sites, from benchmarks.json.

    python3 bench/site/charts.py [DATA]            # DATA: a directory with
                                                   # benchmarks.json, or the file
    python3 bench/site/charts.py /tmp/bench --preview /tmp/bench/site

DATA defaults to site/data. Writes:
  <site>/assets/charts/bench-<city>-t<threads>.svg   time and peak memory per
                                                     task and engine
  and regenerates the blocks between
      <!-- BEGIN bench:NAME -->  and  <!-- END bench:NAME -->
  in <site>/index.html and <site>/benchmarks.html (NAME: headline, meta,
  charts, tables). <site> is site/ itself, or with --preview DIR a copy of the
  two pages and their assets in DIR, so that a test run can be looked at
  without touching the published pages. Every number on those pages comes
  from the results file through this script; nothing is typed by hand.
"""

import argparse
import html
import json
import math
import re
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent

ENGINE_ORDER = ["peabiru-cli", "peabiru-r", "peabiru-py", "r5r", "r5py",
                "dodgr", "cppRouting", "networkx", "igraph", "pandana"]
ENGINE_LABEL = {
    "peabiru-cli": "peabiru CLI",
    "peabiru-r": "peabiru (R)",
    "peabiru-py": "peabiru (Python)",
    "r5r": "r5r",
    "r5py": "r5py",
    "dodgr": "dodgr",
    "cppRouting": "cppRouting",
    "networkx": "networkx",
    "igraph": "igraph",
    "pandana": "pandana",
}
ENGINE_LANG = {
    "peabiru-cli": "Rust", "peabiru-r": "R", "peabiru-py": "Python", "r5r": "R (Java)",
    "r5py": "Python (Java)", "dodgr": "R", "cppRouting": "R", "networkx": "Python",
    "igraph": "Python", "pandana": "Python",
}
# peabiru, the R5 family (same model), every other package
FAMILY = {e: ("peabiru" if e.startswith("peabiru") else "r5" if e in ("r5r", "r5py") else "other")
          for e in ENGINE_ORDER}
FAMILY_LABEL = {"peabiru": "peabiru", "r5": "R5 (r5r, r5py)", "other": "other packages"}
# light, dark: the brand palette of site/assets/brand.css
FAMILY_COLOR = {
    "peabiru": ("#b4532a", "#e98a5c"),  # route (terracotta)
    "r5": ("#3d4f56", "#c6d0d2"),       # slate
    "other": ("#2f6b4f", "#6fbf95"),    # forest
}
TASK_ORDER = ["build", "ttm_walk", "ttm_bicycle", "ttm_car", "ttm_transit", "accessibility", "expanded"]
TASK_SHORT = {
    "build": "network build",
    "ttm_walk": "matrix, walk",
    "ttm_bicycle": "matrix, bicycle",
    "ttm_car": "matrix, car",
    "ttm_transit": "matrix, walk + transit",
    "accessibility": "accessibility",
    "expanded": "expanded (50 origins)",
}
TASK_LONG = {
    "build": "Network build",
    "ttm_walk": "Travel-time matrix, walking",
    "ttm_bicycle": "Travel-time matrix, cycling",
    "ttm_car": "Travel-time matrix, car",
    "ttm_transit": "Travel-time matrix, walk + public transport",
    "accessibility": "Accessibility, walk + public transport",
    "expanded": "Expanded matrix, walk + public transport (first 50 origins)",
}
COMPARED_TASKS = ("ttm_walk", "ttm_bicycle", "ttm_car", "ttm_transit")
# Why each package's matrices differ from peabiru's (qualitative, by design;
# the sizes of the differences come from the data).
WHY = {
    "peabiru-r": "The R package calls the same Rust core as the command line, so its matrices are "
                 "the same.",
    "peabiru-py": "The Python package calls the same Rust core as the command line, so its "
                  "matrices are the same.",
    "r5r": "Same model: peabiru ports R5's street and transit logic, so the difference is zero "
           "(also checked pair by pair, for many more parameters, by the parity suites in "
           "<code>bench/compare/</code>).",
    "r5py": "The same R5 engine (its own 7.5.1 build), so street matrices are expected to agree. "
            "It has no seed and always uses 60 Monte Carlo draws, so transit on a frequency-based "
            "feed (São Paulo) changes from run to run.",
    "dodgr": "Its own network model: points snap to the nearest vertex (R5 snaps to the nearest "
             "point of an edge and adds the walk to it), its own rules for which ways each mode "
             "may use, routes chosen by preference-weighted time, car speeds from its own table "
             "per road class, waits at traffic lights, and no turn restrictions here.",
    "cppRouting": "Routes the very same dodgr graph with contraction hierarchies, so its "
                  "differences from peabiru are dodgr's; it differs from dodgr only where two "
                  "routes take the same time.",
    "networkx": "pyrosm's network (OSMnx's way filters, simplified, largest connected part), "
                "points snapped to the nearest node, one constant speed per mode (no "
                "traffic-stress limit for bicycles) and OSMnx's imputed car speeds with no turn "
                "restrictions.",
    "igraph": "The same graph and weights as networkx, so the same differences; only ties "
              "between equal-time routes can differ.",
    "pandana": "The same graph and weights as networkx; it snaps with its own nearest-node "
               "search, and routes with contraction hierarchies.",
}


def fmt_s(x):
    if x is None:
        return "–"
    if x < 0.1:
        return f"{x:.3f}"
    if x < 10:
        return f"{x:.2f}"
    return f"{x:.1f}"


def fmt_mib(x):
    if x is None:
        return "–"
    return f"{x:,.0f}"


def fmt_mem(x):
    """MiB or GiB, for prose."""
    if x is None:
        return "–"
    return f"{x / 1024:.1f} GiB" if x >= 1024 else f"{x:,.0f} MiB"


def fmt_mem_range(lo, hi):
    if lo == hi:
        return fmt_mem(lo)
    if lo >= 1024 and hi >= 1024:
        return f"{lo / 1024:.1f}–{hi / 1024:.1f} GiB"
    if hi < 1024:
        return f"{lo:,.0f}–{hi:,.0f} MiB"
    return f"{fmt_mem(lo)}–{fmt_mem(hi)}"


def fmt_min(x):
    if x is None:
        return "–"
    return f"{x:.1f}" if abs(x) < 10 else f"{x:.0f}"


def load(path):
    p = Path(path)
    if p.is_dir():
        p = p / "benchmarks.json"
    d = json.load(open(p))
    recs = [r for r in d["records"] if r.get("status") == "ok"]
    return d["meta"], recs, d["records"]


def index(recs):
    return {(r["engine"], r["city"], r["task"], r["threads"]): r for r in recs}


def engines_present(recs):
    present = {r["engine"] for r in recs}
    return [e for e in ENGINE_ORDER if e in present] + sorted(present - set(ENGINE_ORDER))


def label(e):
    return ENGINE_LABEL.get(e, e)


def subset(r):
    return bool(r.get("origins_used")) and r.get("origins_used") < (r.get("origins_total") or 0)


def esc(s):
    return html.escape(str(s))


# ------------------------------------------------------------------ charts


def log_ticks(lo, hi):
    ticks = []
    e = math.floor(math.log10(lo))
    while 10**e <= hi * 1.0001:
        for m in (1, 2, 5):
            v = m * 10**e
            if lo <= v <= hi:
                ticks.append(v)
        e += 1
    return ticks


def tick_label(v):
    return f"{v:,.0f}" if v >= 1 else f"{v:g}"


def chart(meta, idx, city, threads, engines, out):
    """Small multiples: one block per task, one row per engine that ran it
    (fastest first), dots on shared log scales for time and memory. Colour
    says the family (peabiru, R5, other packages); the name is on the row."""
    blocks = []
    for t in TASK_ORDER:
        rows = [idx[(e, city, t, threads)] for e in engines if (e, city, t, threads) in idx]
        rows = [r for r in rows if r.get("seconds")]
        if rows:
            blocks.append((t, sorted(rows, key=lambda r: r["seconds"])))
    if not blocks:
        return False
    cityname = meta["cities"][city]["name"]
    npts = meta["cities"][city]["points"]
    row_h, head_h, top, left = 21, 30, 118, 210
    pw, gap = 270, 84
    W = left + 2 * pw + gap + 40
    H = top + sum(head_h + row_h * len(rows) for _, rows in blocks) + 50

    def scale(key):
        vals = [r.get(key) for _, rows in blocks for r in rows if r.get(key)]
        if not vals:
            return None
        lo = 10 ** math.floor(math.log10(min(vals)))
        hi = 10 ** math.ceil(math.log10(max(vals)))
        if hi / lo < 100:
            hi = lo * 100
        return lo, hi

    scales = {"seconds": scale("seconds"), "peak_rss_mib": scale("peak_rss_mib")}
    x0s = {"seconds": left, "peak_rss_mib": left + pw + gap}

    def X(key, v):
        lo, hi = scales[key]
        return x0s[key] + (math.log10(v) - math.log10(lo)) / (math.log10(hi) - math.log10(lo)) * pw

    s = []
    y1 = H - 50
    for key, title, unit in (("seconds", "Time (median of the timed runs)", "seconds"),
                             ("peak_rss_mib", "Peak memory of the process", "MiB")):
        if not scales[key]:
            continue
        x0 = x0s[key]
        s.append(f'<text class="t h" x="{x0}" y="{top - 38}">{esc(title)}</text>')
        ticks = log_ticks(*scales[key])
        for v in ticks:
            x = X(key, v)
            major = abs(math.log10(v) - round(math.log10(v))) < 1e-9
            s.append(f'<line class="{"grid" if major else "grid2"}" x1="{x:.1f}" y1="{top - 14}" '
                     f'x2="{x:.1f}" y2="{y1}"/>')
            if major or len(ticks) < 8:
                for y in (top - 18, y1 + 15):
                    s.append(f'<text class="t xs" x="{x:.1f}" y="{y}" text-anchor="middle">'
                             f'{tick_label(v)}</text>')
        s.append(f'<text class="t xs" x="{x0 + pw / 2}" y="{y1 + 32}" text-anchor="middle">'
                 f'{unit}, log scale</text>')
    y = top
    for bi, (t, rows) in enumerate(blocks):
        s.append(f'<text class="t h s" x="20" y="{y + 20}">{esc(TASK_SHORT[t])}</text>')
        s.append(f'<line class="sep" x1="16" y1="{y + 4}" x2="{W - 16}" y2="{y + 4}"/>')
        y += head_h
        for i, r in enumerate(rows):
            e = r["engine"]
            yc = y + row_h * i + row_h / 2
            if i % 2 == 0:
                s.append(f'<rect class="band" x="16" y="{y + row_h * i}" width="{W - 32}" height="{row_h}"/>')
            mark = []
            if subset(r):
                mark.append(f'{r["origins_used"]:,} origins')
            if r.get("measured_threads") or (r.get("threads_used") == 1 and threads > 1):
                mark.append("1 thread")
            name = label(e) + (f" ({', '.join(mark)})" if mark else "")
            s.append(f'<text class="t s" x="32" y="{yc + 4.5}">{esc(name)}</text>')
            for key, unit in (("seconds", "s"), ("peak_rss_mib", "MiB")):
                v = r.get(key)
                if not v or not scales[key]:
                    continue
                x = X(key, v)
                txt = fmt_s(v) if key == "seconds" else fmt_mib(v)
                s.append(f'<circle class="f-{FAMILY.get(e, "other")}" cx="{x:.1f}" cy="{yc}" r="5.5">'
                         f'<title>{esc(label(e))}: {txt} {unit}</title></circle>')
                right = x + 9 + 7 * len(txt) < x0s[key] + pw + 30
                tx = x + 9 if right else x - 9
                s.append(f'<text class="t xs v" x="{tx:.1f}" y="{yc + 4}" '
                         f'text-anchor="{"start" if right else "end"}">{txt}</text>')
        y += row_h * len(rows)
    legend = []
    lx = 20
    fams = [f for f in FAMILY_LABEL if any(FAMILY.get(r["engine"], "other") == f
                                           for _, rows in blocks for r in rows)]
    for f in fams:
        legend.append(f'<circle class="f-{f}" cx="{lx + 6}" cy="54" r="5.5"/>'
                      f'<text class="t s" x="{lx + 17}" y="58.5">{esc(FAMILY_LABEL[f])}</text>')
        lx += 40 + 7.2 * len(FAMILY_LABEL[f])
    light = "\n    ".join(f".f-{f} {{ fill: {c[0]}; }}" for f, c in FAMILY_COLOR.items())
    dark = "\n      ".join(f".f-{f} {{ fill: {c[1]}; }}" for f, c in FAMILY_COLOR.items())
    thr = f"{threads} thread{'s' if threads > 1 else ''}"
    title = f"{cityname}: time and peak memory by task and engine, {thr}"
    desc = (f"Small multiples, one block per task and one row per engine, fastest first, for "
            f"{cityname} ({npts} points, all to all unless the row says otherwise) with {thr}: "
            "left, median seconds of the timed runs; right, peak resident memory of the process; "
            "log scales; colour marks peabiru, the R5 family and other packages. Generated from "
            "benchmarks.json by bench/site/charts.py.")
    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" role="img" aria-labelledby="title desc">
  <title id="title">{esc(title)}</title>
  <desc id="desc">{esc(desc)}</desc>
  <style>
    .panel {{ fill: #fffdf9; stroke: #e1d8c8; }}
    .t {{ font-family: 'Helvetica Neue', Helvetica, Arial, sans-serif; font-size: 13px; fill: #1f2a2e; }}
    .s {{ font-size: 12.5px; }}
    .xs {{ font-size: 11px; fill: #4a5a5f; }}
    .v {{ fill: #1f2a2e; font-variant-numeric: tabular-nums; }}
    .h {{ font-weight: 700; }}
    .band {{ fill: #f6f1e8; }}
    .sep {{ stroke: #d9cfbe; stroke-width: 1; }}
    .grid {{ stroke: #d9cfbe; stroke-width: 1; }}
    .grid2 {{ stroke: #ece4d6; stroke-width: 1; }}
    circle {{ stroke: #fffdf9; stroke-width: 1.5; }}
    {light}
    @media (prefers-color-scheme: dark) {{
      .panel {{ fill: #1d2427; stroke: #34403f; }}
      .t {{ fill: #ece6da; }}
      .xs {{ fill: #a9b4b6; }}
      .v {{ fill: #ece6da; }}
      .band {{ fill: #222a2d; }}
      .sep, .grid {{ stroke: #3e4a4c; }}
      .grid2 {{ stroke: #2c3538; }}
      circle {{ stroke: #1d2427; }}
      {dark}
    }}
  </style>
  <rect class="panel" x="0.5" y="0.5" width="{W - 1}" height="{H - 1}" rx="12"/>
  <text class="t h" x="20" y="30">{esc(cityname)} · {npts:,} × {npts:,} points · {thr}</text>
  {"".join(legend)}
  {"".join(s)}
</svg>
'''
    out.write_text(svg)
    return True


# ------------------------------------------------------------------ html


def pick_peabiru(idx, c, t, threads):
    for e in ("peabiru-r", "peabiru-py", "peabiru-cli"):
        r = idx.get((e, c, t, threads))
        if r:
            return r
    return None


def headline(meta, idx, engines, link=True):
    """A neutral summary: peabiru's own time and memory on the headline tasks,
    next to the range of the other packages that ran the same task."""
    if not idx:
        return "<p>No benchmark results yet: run <code>bench/site/run.py</code>.</p>"
    threads = max(k[3] for k in idx)
    c = "poa" if "poa" in meta["cities"] else next(iter(meta["cities"]))
    n = meta["cities"][c]["points"]
    tiles, notes = [], []
    reps = None
    for t, what in (("ttm_transit", "walk + public transport matrix"),
                    ("ttm_walk", "walking matrix")):
        pb = pick_peabiru(idx, c, t, threads)
        if not pb:
            continue
        reps = pb.get("reps")
        row = []
        tiles.append(row)
        row.append(
            f'<div class="stat"><span class="stat-v">{fmt_s(pb["seconds"])} s</span>'
            f'<span class="stat-l">{esc(label(pb["engine"]))}, {what}'
            f'{" · " + fmt_mem(pb["peak_rss_mib"]) if pb.get("peak_rss_mib") else ""}</span></div>')
        others = [idx[(e, c, t, threads)] for e in engines
                  if not e.startswith("peabiru") and (e, c, t, threads) in idx]
        full = [r for r in others if not subset(r)]
        part = [r for r in others if subset(r)]
        if full:
            lo = min(r["seconds"] for r in full)
            hi = max(r["seconds"] for r in full)
            mems = [r["peak_rss_mib"] for r in full if r.get("peak_rss_mib")]
            rng = fmt_s(lo) if lo == hi else f"{fmt_s(lo)}–{fmt_s(hi)}"
            who = ", ".join(label(r["engine"]) for r in sorted(full, key=lambda r: r["seconds"]))
            mem = " · " + fmt_mem_range(min(mems), max(mems)) if mems else ""
            row.append(
                f'<div class="stat stat-other"><span class="stat-v">{rng} s</span>'
                f'<span class="stat-l">{len(full)} other package{"s" if len(full) > 1 else ""} on '
                f'the same task{mem}<br><span class="stat-who">{esc(who)}</span></span></div>')
        if part:
            notes.append(f"{', '.join(label(r['engine']) for r in part)} ran the {what} on a subset "
                         "of origins and {} not in the range".format("is" if len(part) == 1 else "are"))
    if not tiles:
        return "<p>No benchmark results for peabiru yet: run <code>bench/site/run.py</code>.</p>"
    more = '; <a href="benchmarks.html">methodology, all tasks and how the results differ</a>' if link else ""
    extra = (" " + "; ".join(notes) + ".") if notes else ""
    model = ""
    if any(FAMILY.get(e) == "other" for e in engines):
        model = (" Only r5r and r5py compute the same matrices as peabiru; the street packages use "
                 "network models of their own, so their times answer a similar but not identical "
                 "question (the differences are measured on the benchmarks page).")
    rows = "".join(f'<div class="stats">{"".join(r)}</div>' for r in tiles)
    return f'''{rows}
<p class="stat-note">{esc(meta["cities"][c]["name"])}, {n:,} × {n:,} points, {threads} threads; median of
{reps or "the"} timed runs after a warm-up; memory is the peak of the whole process.{model}{extra}
Measured {esc(meta["generated"][:10])}{more}.</p>'''


def meta_block(meta):
    m = meta["machine"]
    v = meta["versions"]
    eng = []
    for e in engines_present([{"engine": k} for k in meta["engines"]]):
        s = meta["engines"].get(e)
        if s["status"] == "ok":
            eng.append(f"<li><strong>{esc(label(e))}</strong>: {esc(s.get('version') or '?')}</li>")
        else:
            eng.append(f"<li><strong>{esc(label(e))}</strong>: not run ({esc(s.get('reason') or '')})</li>")
    if m.get("macos"):
        machine = (f"{esc(m.get('cpu', '?'))} ({m.get('cores', '?')} cores"
                   + (f", {m['perf_cores']} performance" if m.get("perf_cores") else "")
                   + f"), {m.get('ram_gib', '?')} GiB RAM, macOS {esc(m.get('macos', '?'))}")
    else:
        machine = f"{esc(m.get('cpu') or '?')} ({m.get('cores', '?')} cores, {m.get('ram_gib', '?')} GiB RAM), {esc(m.get('os', ''))}"
        lim = []
        if m.get("cgroup_cpus"):
            lim.append(f"{m['cgroup_cpus']:g} CPUs")
        if m.get("cgroup_memory_gib"):
            lim.append(f"{m['cgroup_memory_gib']:g} GiB")
        if m.get("container"):
            machine += f"; container <code>{esc(m['container'])}</code>"
        if lim:
            machine += " limited to " + " and ".join(lim)
    load = m.get("load_avg_at_start")
    load_end = m.get("load_avg_at_end")
    pins = ""
    if v.get("cran_snapshot"):
        pins += f"; CRAN snapshot {esc(v['cran_snapshot'])}"
    if v.get("java"):
        pins += f"; {esc(v['java'])}"
    parts = len(meta.get("resumed") or []) + 1
    span = f" (in {parts} parts, each engine whole within one)" if parts > 1 else ""
    return f'''<ul class="meta">
<li><strong>Run</strong>: {esc(meta["generated"])} to {esc(meta.get("finished", "?"))}{span}; peabiru commit
<code>{esc(v.get("peabiru_commit"))}</code>{" (core modified)" if v.get("peabiru_core_dirty") else ""}, {esc(v.get("rustc"))}, {esc(v.get("R"))}{pins}</li>
<li><strong>Machine</strong>: {machine}{f"; 1-minute load average {load[0]} when the run started" if load else ""}{f" and {load_end[0]} when it ended" if load_end else ""}</li>
{"".join(eng)}
</ul>'''


def diff_cells(e, cmp_):
    """Cells: pairs reached by both / only peabiru / only the engine, median and
    90th percentile of the absolute difference."""
    if cmp_:
        z = cmp_["pairs_both"] and cmp_["median_abs_diff_min"] == 0 and cmp_["p90_abs_diff_min"] == 0 \
            and not cmp_["only_peabiru"] and not cmp_["only_engine"]
        cells = [f'{cmp_["pairs_both"]:,}', f'{cmp_["only_peabiru"]:,}', f'{cmp_["only_engine"]:,}',
                 fmt_min(cmp_["median_abs_diff_min"]), fmt_min(cmp_["p90_abs_diff_min"])]
        out = "".join(f"<td>{c}</td>" for c in cells)
        if z:
            out = out[:-5] + ' <span class="tag ok">identical</span></td>'
        return out
    if e == "peabiru-cli":
        return '<td colspan="5" class="muted">reference</td>'
    if e in ("peabiru-r", "peabiru-py", "r5r"):
        return ('<td colspan="5" class="muted">same results (not compared in this run; see the '
                'parity suites)</td>')
    return '<td colspan="5" class="muted">–</td>'


def tables(meta, idx, recs_all, engines):
    out = []
    threads_all = sorted({k[3] for k in idx})
    tmax = threads_all[-1] if threads_all else None
    skipped = {(x["engine"], x["city"], x["task"], x["threads"]): x.get("note")
               for x in recs_all if x.get("status") == "skipped"}
    for c, info in meta["cities"].items():
        out.append(f'<h3>{esc(info["name"])} <span class="muted">({info["points"]:,} points, all to all; '
                   f'{", ".join(esc(g) for g in info["gtfs"])})</span></h3>')
        for t in TASK_ORDER:
            rows = [e for e in engines if any((e, c, t, n) in idx for n in threads_all)]
            if not rows:
                continue
            def sort_key(e):
                r = next(idx[(e, c, t, n)] for n in reversed(threads_all) if (e, c, t, n) in idx)
                # runs on a subset of the origins after the complete ones
                subset = bool(r.get("origins_used")) and r["origins_used"] < r.get("origins_total", 0)
                return (subset, r["seconds"])
            rows.sort(key=sort_key)
            cmp_task = t in COMPARED_TASKS
            head = '<th scope="col">Package</th><th scope="col">Language</th>'
            for n in threads_all:
                head += f'<th scope="col">Time, {n} thread{"s" if n > 1 else ""}</th>'
            head += f'<th scope="col">Peak memory ({tmax} thr.)</th>' if t != "build" else ""
            if cmp_task:
                head += ('<th scope="col">Pairs reached by both</th><th scope="col">Only peabiru</th>'
                         '<th scope="col">Only this package</th><th scope="col">Median |Δ|, min</th>'
                         '<th scope="col">90th pct |Δ|, min</th>')
            body, notes = [], []
            for e in rows:
                cells = [f'<th scope="row">{esc(label(e))}</th><td>{esc(ENGINE_LANG.get(e, ""))}</td>']
                first = None
                for n in threads_all:
                    r = idx.get((e, c, t, n))
                    if r:
                        first = first or r
                        mark = ""
                        if r.get("measured_threads"):
                            mark = ' <span class="muted" title="single-threaded: the 1-thread measurement">(1 thread)</span>'
                        cells.append(f'<td>{fmt_s(r["seconds"])} s{mark}</td>')
                    else:
                        why = skipped.get((e, c, t, n)) or "not run"
                        cells.append(f'<td class="muted" title="{esc(why)}">–</td>')
                if t != "build":
                    r = idx.get((e, c, t, tmax)) or {}
                    cells.append(f'<td>{fmt_mib(r.get("peak_rss_mib"))} MiB</td>' if r.get("peak_rss_mib")
                                 else '<td class="muted">–</td>')
                if cmp_task:
                    cmp_ = next((idx[(e, c, t, n)].get("comparison") for n in threads_all
                                 if (e, c, t, n) in idx and idx[(e, c, t, n)].get("comparison")), None)
                    cells.append(diff_cells(e, cmp_))
                sub = first and subset(first)
                if sub:
                    cells[0] = cells[0].replace("</th>", f' <span class="muted">({first["origins_used"]:,} of '
                                                         f'{first["origins_total"]:,} origins)</span></th>')
                body.append(f'<tr class="fam-{FAMILY.get(e, "other")}">{"".join(cells)}</tr>')
                ns = list(first.get("notes") or []) if first else []
                if sub:
                    ns.insert(0, f'run on the first {first["origins_used"]:,} of {first["origins_total"]:,} '
                                 "origins (all destinations); the time is that of the subset")
                if ns:
                    notes.append(f"<li><strong>{esc(label(e))}</strong>: {esc('; '.join(ns))}.</li>")
            caption = esc(TASK_LONG[t])
            if cmp_task:
                caption += (' <span class="muted">· differences against peabiru\'s matrix over the same '
                            'origins, in minutes</span>')
            out.append(f'<div class="table-wrap"><table class="bench"><caption>{caption}</caption>'
                       f'<thead><tr>{head}</tr></thead><tbody>{"".join(body)}</tbody></table></div>')
            if notes:
                out.append(f'<details class="bench-notes"><summary>Notes on this task</summary>'
                           f'<ul>{"".join(notes)}</ul></details>')
    return "\n".join(out)


def why_block(engines, idx):
    items = []
    for e in engines:
        if e in WHY and e != "peabiru-cli":
            items.append(f"<li><strong>{esc(label(e))}</strong>. {WHY[e]}</li>")
    if not items:
        return ""
    return "<ul class=\"why\">" + "".join(items) + "</ul>"


def charts_block(meta, idx, site, prefix):
    threads_all = sorted({k[3] for k in idx}, reverse=True)
    figs = []
    for c, info in meta["cities"].items():
        for n in threads_all:
            f = f"assets/charts/bench-{c}-t{n}.svg"
            if (site / f).exists():
                figs.append(f'<figure><img src="{prefix}{f}" alt="{esc(info["name"])}: time and peak '
                            f'memory by task and engine, {n} threads" loading="lazy"></figure>')
    return "\n".join(figs)


def replace_blocks(path, blocks):
    if not path.exists():
        return
    s = path.read_text()
    for name, content in blocks.items():
        pat = re.compile(rf"(<!-- BEGIN bench:{name} [^>]*-->)(.*?)(<!-- END bench:{name} -->)", re.S)
        s = pat.sub(lambda m: m.group(1) + "\n" + content + "\n" + m.group(3), s)
    path.write_text(s)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("data", nargs="?", help="directory with benchmarks.json, or the file (default site/data)")
    ap.add_argument("--data", dest="data_opt", help=argparse.SUPPRESS)
    ap.add_argument("--preview", help="write the pages and charts into a copy of the site in this "
                                      "directory instead of site/")
    a = ap.parse_args()
    data = a.data or a.data_opt or str(ROOT / "site" / "data")
    meta, recs, recs_all = load(data)
    site = ROOT / "site"
    if a.preview:
        dst = Path(a.preview)
        dst.mkdir(parents=True, exist_ok=True)
        for f in ("index.html", "benchmarks.html", "style.css"):
            shutil.copy2(site / f, dst / f)
        shutil.copytree(site / "assets", dst / "assets", dirs_exist_ok=True)
        src = Path(data) if Path(data).is_dir() else Path(data).parent
        (dst / "data").mkdir(exist_ok=True)
        for f in ("benchmarks.json", "benchmarks.csv"):
            if (src / f).exists():
                shutil.copy2(src / f, dst / "data" / f)
        site = dst
    idx = index(recs)
    engines = engines_present(recs)
    outdir = site / "assets" / "charts"
    outdir.mkdir(parents=True, exist_ok=True)
    for old in outdir.glob("bench-*.svg"):
        old.unlink()
    for c in meta["cities"]:
        for n in sorted({k[3] for k in idx}):
            chart(meta, idx, c, n, engines, outdir / f"bench-{c}-t{n}.svg")
    replace_blocks(site / "index.html", {"headline": headline(meta, idx, engines)})
    replace_blocks(site / "benchmarks.html", {
        "headline": headline(meta, idx, engines, link=False),
        "meta": meta_block(meta),
        "charts": charts_block(meta, idx, site, ""),
        "tables": tables(meta, idx, recs_all, engines),
        "why": why_block(engines, idx),
    })


if __name__ == "__main__":
    main()
