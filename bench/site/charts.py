#!/usr/bin/env python3
"""Charts and tables for the documentation sites, from site/data/benchmarks.json.

    python3 bench/site/charts.py [--data site/data/benchmarks.json]

Writes:
  site/assets/charts/bench-<city>-t<threads>.svg   time and peak memory per task
  and regenerates the blocks between
      <!-- BEGIN bench:NAME -->  and  <!-- END bench:NAME -->
  in site/index.html and site/benchmarks.html (NAME: headline, meta, engines,
  tables, charts). Every number on those pages comes from the results file
  through this script; nothing is typed by hand.
"""

import argparse
import html
import json
import math
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent

ENGINE_ORDER = ["peabiru-cli", "peabiru-r", "peabiru-py", "r5r", "r5py"]
ENGINE_LABEL = {
    "peabiru-cli": "peabiru CLI",
    "peabiru-r": "peabiru (R)",
    "peabiru-py": "peabiru (Python)",
    "r5r": "r5r",
    "r5py": "r5py",
}
# light, dark
ENGINE_COLOR = {
    "peabiru-cli": ("#b4532a", "#e98a5c"),
    "peabiru-r": ("#c98a1d", "#f0b54a"),
    "peabiru-py": ("#2f6b4f", "#6fbf95"),
    "r5r": ("#3d4f56", "#c6d0d2"),
    "r5py": ("#7f8fa6", "#8fa3bf"),
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


def load(path):
    d = json.load(open(path))
    recs = [r for r in d["records"] if r.get("status") == "ok"]
    return d["meta"], recs, d["records"]


def index(recs):
    return {(r["engine"], r["city"], r["task"], r["threads"]): r for r in recs}


def engines_present(recs):
    present = {r["engine"] for r in recs}
    return [e for e in ENGINE_ORDER if e in present]


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


def tick_label(v, unit):
    if unit == "s":
        return f"{v:g}"
    return f"{v:,.0f}" if v >= 1 else f"{v:g}"


def chart(meta, idx, city, threads, engines, out):
    tasks = [t for t in TASK_ORDER if any((e, city, t, threads) in idx for e in engines)]
    if not tasks:
        return False
    row_h, top, left = 30, 112, 190
    pw = 300  # panel width
    gap = 70
    W = left + 2 * pw + gap + 30
    H = top + row_h * len(tasks) + 56
    cityname = meta["cities"][city]["name"]
    npts = meta["cities"][city]["points"]

    def panel(x0, key, unit, title):
        vals = [idx[(e, city, t, threads)].get(key) for t in tasks for e in engines
                if (e, city, t, threads) in idx]
        vals = [v for v in vals if v]
        if not vals:
            return ""
        lo = 10 ** math.floor(math.log10(min(vals)))
        hi = 10 ** math.ceil(math.log10(max(vals)))
        if hi / lo < 100:
            hi = lo * 100

        def X(v):
            return x0 + (math.log10(v) - math.log10(lo)) / (math.log10(hi) - math.log10(lo)) * pw

        s = [f'<text class="t h" x="{x0}" y="{top - 22}">{html.escape(title)}</text>']
        y1 = top + row_h * len(tasks)
        for v in log_ticks(lo, hi):
            x = X(v)
            major = abs(math.log10(v) - round(math.log10(v))) < 1e-9
            s.append(f'<line class="{"grid" if major else "grid2"}" x1="{x:.1f}" y1="{top - 10}" x2="{x:.1f}" y2="{y1}"/>')
            if major or len(log_ticks(lo, hi)) < 8:
                s.append(f'<text class="t xs" x="{x:.1f}" y="{y1 + 16}" text-anchor="middle">{tick_label(v, unit)}</text>')
        s.append(f'<text class="t xs" x="{x0 + pw / 2}" y="{y1 + 34}" text-anchor="middle">{"seconds" if unit == "s" else "MiB"}, log scale</text>')
        for i, t in enumerate(tasks):
            yc = top + row_h * i + row_h / 2
            pts = [(e, idx[(e, city, t, threads)].get(key)) for e in engines if (e, city, t, threads) in idx]
            pts = [(e, v) for e, v in pts if v]
            if not pts:
                continue
            xs = [X(v) for _, v in pts]
            s.append(f'<line class="rng" x1="{min(xs):.1f}" y1="{yc}" x2="{max(xs):.1f}" y2="{yc}"/>')
            for e, v in pts:
                s.append(f'<circle class="e-{e}" cx="{X(v):.1f}" cy="{yc}" r="6"><title>{ENGINE_LABEL[e]}: {fmt_s(v) if unit == "s" else fmt_mib(v)} {"s" if unit == "s" else "MiB"}</title></circle>')
        return "\n  ".join(s)

    rows = []
    for i, t in enumerate(tasks):
        yc = top + row_h * i + row_h / 2
        if i % 2 == 0:
            rows.append(f'<rect class="band" x="12" y="{top + row_h * i}" width="{W - 24}" height="{row_h}"/>')
        rows.append(f'<text class="t s" x="24" y="{yc + 4.5}">{html.escape(TASK_SHORT[t])}</text>')
    legend = []
    lx = 24
    for e in engines:
        legend.append(f'<circle class="e-{e}" cx="{lx + 6}" cy="56" r="6"/>'
                      f'<text class="t s" x="{lx + 18}" y="60.5">{ENGINE_LABEL[e]}</text>')
        lx += 30 + 8 * len(ENGINE_LABEL[e])
    light = "\n    ".join(f".e-{e} {{ fill: {c[0]}; }}" for e, c in ENGINE_COLOR.items())
    dark = "\n      ".join(f".e-{e} {{ fill: {c[1]}; }}" for e, c in ENGINE_COLOR.items())
    title = f"{cityname}: time and peak memory by task, {threads} thread{'s' if threads > 1 else ''}"
    desc = (f"Dot plot, one row per task, for {cityname} ({npts} points, all to all) with "
            f"{threads} thread(s): left, median seconds of the timed runs; right, peak resident "
            "memory of the process; one dot per engine, log scales. Generated from "
            "site/data/benchmarks.json by bench/site/charts.py.")
    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" role="img" aria-labelledby="title desc">
  <title id="title">{html.escape(title)}</title>
  <desc id="desc">{html.escape(desc)}</desc>
  <style>
    .panel {{ fill: #fffdf9; stroke: #e1d8c8; }}
    .t {{ font-family: 'Helvetica Neue', Helvetica, Arial, sans-serif; font-size: 13px; fill: #1f2a2e; }}
    .s {{ font-size: 12.5px; }}
    .xs {{ font-size: 11px; fill: #4a5a5f; }}
    .h {{ font-weight: 700; }}
    .band {{ fill: #f6f1e8; }}
    .grid {{ stroke: #d9cfbe; stroke-width: 1; }}
    .grid2 {{ stroke: #ece4d6; stroke-width: 1; }}
    .rng {{ stroke: #c9bfae; stroke-width: 2; }}
    circle {{ stroke: #fffdf9; stroke-width: 1.5; }}
    {light}
    @media (prefers-color-scheme: dark) {{
      .panel {{ fill: #1d2427; stroke: #34403f; }}
      .t {{ fill: #ece6da; }}
      .xs {{ fill: #a9b4b6; }}
      .band {{ fill: #222a2d; }}
      .grid {{ stroke: #3e4a4c; }}
      .grid2 {{ stroke: #2c3538; }}
      .rng {{ stroke: #4a5859; }}
      circle {{ stroke: #1d2427; }}
      {dark}
    }}
  </style>
  <rect class="panel" x="0.5" y="0.5" width="{W - 1}" height="{H - 1}" rx="12"/>
  <text class="t h" x="24" y="30">{html.escape(cityname)} · {npts:,} × {npts:,} points · {threads} thread{"s" if threads > 1 else ""}</text>
  {"".join(legend)}
  {"".join(rows)}
  {panel(left, "seconds", "s", "Time (median)")}
  {panel(left + pw + gap, "peak_rss_mib", "mib", "Peak memory of the process")}
</svg>
'''
    out.write_text(svg)
    return True


# ------------------------------------------------------------------ html


def headline(meta, idx, engines, link=True):
    threads = max(k[3] for k in idx)
    c, t = "poa", "ttm_transit"
    pb = idx.get(("peabiru-r", c, t, threads)) or idx.get(("peabiru-cli", c, t, threads))
    ref = idx.get(("r5r", c, t, threads))
    if not (pb and ref):
        return "<p>No benchmark results yet: run <code>bench/site/run.py</code>.</p>"
    n = meta["cities"][c]["points"]
    who = "peabiru (R)" if pb["engine"] == "peabiru-r" else "peabiru CLI"
    speed = ref["seconds"] / pb["seconds"]
    mem = (ref["peak_rss_mib"] / pb["peak_rss_mib"]) if pb.get("peak_rss_mib") and ref.get("peak_rss_mib") else None
    more = '; <a href="benchmarks.html">methodology and all tasks</a>' if link else ""
    rows = f'''<div class="stat"><span class="stat-v">{speed:.1f}×</span><span class="stat-l">faster than r5r</span></div>'''
    if mem:
        rows += f'''<div class="stat"><span class="stat-v">{mem:.0f}×</span><span class="stat-l">less peak memory</span></div>'''
    return f'''<div class="stats">{rows}</div>
<p class="stat-note">{html.escape(meta["cities"][c]["name"])}, walk + public transport travel-time matrix,
{n:,} × {n:,} points, {threads} threads: {who} {fmt_s(pb["seconds"])} s and {fmt_mib(pb.get("peak_rss_mib"))} MiB,
r5r {fmt_s(ref["seconds"])} s and {fmt_mib(ref.get("peak_rss_mib"))} MiB (median of {pb["reps"]} runs; JVM warmed up;
memory is the whole process). Provisional numbers from {html.escape(meta["generated"][:10])}{more}.</p>'''


def meta_block(meta):
    m = meta["machine"]
    v = meta["versions"]
    eng = []
    for e in ENGINE_ORDER:
        s = meta["engines"].get(e)
        if not s:
            continue
        if s["status"] == "ok":
            eng.append(f"<li><strong>{ENGINE_LABEL[e]}</strong>: {html.escape(s.get('version') or '?')}</li>")
        else:
            eng.append(f"<li><strong>{ENGINE_LABEL[e]}</strong>: not run ({html.escape(s.get('reason') or '')})</li>")
    machine = (f"{html.escape(m.get('cpu', '?'))} ({m.get('cores', '?')} cores"
               + (f", {m['perf_cores']} performance" if m.get("perf_cores") else "")
               + f"), {m.get('ram_gib', '?')} GiB RAM, macOS {html.escape(m.get('macos', '?'))}")
    load = m.get("load_avg_at_start")
    load_end = m.get("load_avg_at_end")
    return f'''<ul class="meta">
<li><strong>Run</strong>: {html.escape(meta["generated"])} to {html.escape(meta.get("finished", "?"))}; peabiru commit
<code>{html.escape(str(v.get("peabiru_commit")))}</code>{" (core modified)" if v.get("peabiru_core_dirty") else ""}, {html.escape(str(v.get("rustc")))}, {html.escape(str(v.get("R")))}</li>
<li><strong>Machine</strong>: {machine}{f"; 1-minute load average {load[0]} when the run started" if load else ""}{f" and {load_end[0]} when it ended" if load_end else ""}</li>
{"".join(eng)}
</ul>'''


def tables(meta, idx, recs_all, engines):
    out = []
    threads_all = sorted({k[3] for k in idx})
    for c, info in meta["cities"].items():
        out.append(f'<h3>{html.escape(info["name"])} <span class="muted">({info["points"]:,} points, all to all; '
                   f'{", ".join(html.escape(g) for g in info["gtfs"])})</span></h3>')
        for n in threads_all:
            head = "".join(f'<th scope="col">{ENGINE_LABEL[e]}</th>' for e in engines)
            body = []
            for t in TASK_ORDER:
                cells = []
                anyv = False
                for e in engines:
                    r = idx.get((e, c, t, n))
                    if r:
                        anyv = True
                        mem = f'<span class="muted"> · {fmt_mib(r.get("peak_rss_mib"))} MiB</span>' if r.get("peak_rss_mib") else ""
                        cells.append(f'<td>{fmt_s(r["seconds"])} s{mem}</td>')
                    else:
                        skipped = next((x for x in recs_all if x["engine"] == e and x["city"] == c
                                        and x["task"] == t and x["threads"] == n), None)
                        why = html.escape(skipped["note"]) if skipped and skipped.get("note") else "not run"
                        cells.append(f'<td class="muted" title="{why}">–</td>')
                if anyv:
                    body.append(f'<tr><th scope="row">{html.escape(TASK_SHORT[t])}</th>{"".join(cells)}</tr>')
            out.append(f'<div class="table-wrap"><table><caption>{n} thread{"s" if n > 1 else ""}: median seconds · peak MiB</caption>'
                       f'<thead><tr><th scope="col">Task</th>{head}</tr></thead><tbody>{"".join(body)}</tbody></table></div>')
    return "\n".join(out)


def charts_block(meta, idx, prefix):
    threads_all = sorted({k[3] for k in idx}, reverse=True)
    figs = []
    for c, info in meta["cities"].items():
        for n in threads_all:
            f = f"assets/charts/bench-{c}-t{n}.svg"
            if (ROOT / "site" / f).exists():
                figs.append(f'<figure><img src="{prefix}{f}" alt="{html.escape(info["name"])}: time and peak memory by task and engine, {n} threads" loading="lazy"></figure>')
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
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", default=str(ROOT / "site" / "data" / "benchmarks.json"))
    a = ap.parse_args()
    meta, recs, recs_all = load(a.data)
    idx = index(recs)
    engines = engines_present(recs)
    outdir = ROOT / "site" / "assets" / "charts"
    outdir.mkdir(parents=True, exist_ok=True)
    for old in outdir.glob("bench-*.svg"):
        old.unlink()
    for c in meta["cities"]:
        for n in sorted({k[3] for k in idx}):
            chart(meta, idx, c, n, engines, outdir / f"bench-{c}-t{n}.svg")
    replace_blocks(ROOT / "site" / "index.html", {"headline": headline(meta, idx, engines)})
    replace_blocks(ROOT / "site" / "benchmarks.html", {
        "headline": headline(meta, idx, engines, link=False),
        "meta": meta_block(meta),
        "charts": charts_block(meta, idx, ""),
        "tables": tables(meta, idx, recs_all, engines),
    })


if __name__ == "__main__":
    main()
