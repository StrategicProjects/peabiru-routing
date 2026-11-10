#!/usr/bin/env node
// Parity of the npm package (WebAssembly) with the native command-line tool:
// for each city, the same matrices computed by `peabiru` (the CLI, one
// thread) and by js/lib/index.js must agree cell for cell. Street matrices
// (WALK, BICYCLE, CAR), a transit matrix with five percentiles,
// accessibility (transit and street), expanded and arrival matrices.
//
// Usage: node bench/compare/js_parity.mjs [city ...]   (default: poa spo)
//   poa, spo  r5r's sample networks and hexgrids (R5R_EXTDATA/<city>)
//   sample    the sample shipped with the Python package (python/peabiru/data/spo)
// Env:  PEABIRU_BIN  the native CLI (default: build it with cargo)
//       R5R_EXTDATA  r5r's extdata directory (default: ask the installed r5r)
//       PARITY_OUT   directory for the CLI's CSV files (default: a temp dir)
// Needs the WebAssembly package built (js/build.sh). Exits 1 if any result
// differs. The feeds are given to the WebAssembly build in the order the
// CLI read them on this machine (it prints them), as the order matters.
//
// The JavaScript functions apply r5r's last step, which the CLI leaves
// out: travel times above max_trip_duration become NA (and, in expanded
// and arrival matrices, the whole breakdown). The CLI's values are compared
// after the same step. Street matrices and street accessibility use the
// network of the .pbf alone on both sides (the CLI's ttm on the .pbf), the
// rest the network with the feeds' stops linked.
//
// The timings printed at the end are the routing times the CLI reports
// (one thread; for expanded and arrival matrices they include writing the
// CSV) and those of the JavaScript calls in this process.

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const peabiru = await import(join(repo, "js", "lib", "index.js"));
const NA = peabiru.NA_INT32;

function cliBinary() {
  if (process.env.PEABIRU_BIN) return process.env.PEABIRU_BIN;
  execFileSync("cargo", ["build", "-q", "--release", "-p", "peabiru-cli"], { cwd: repo, stdio: "inherit" });
  const meta = JSON.parse(execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], { cwd: repo }));
  return join(meta.target_directory, "release", "peabiru");
}
const BIN = cliBinary();
const X =
  process.env.R5R_EXTDATA ||
  execFileSync("Rscript", ["-e", 'cat(system.file("extdata", package="r5r"))']).toString().trim();
const OUT = process.env.PARITY_OUT || mkdtempSync(join(tmpdir(), "peabiru-js-parity-"));
mkdirSync(OUT, { recursive: true });

/** Run the CLI; returns [stdout, seconds of routing, seconds of building] as it reports them. */
function cli(args) {
  const out = execFileSync(BIN, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  const secs = (what) => {
    const m = new RegExp(`${what} ([0-9.]+)s`).exec(out);
    return m ? Number(m[1]) : NaN;
  };
  return [out, secs("route"), secs("build")];
}

/** Fields of one CSV line (fwrite's quoting). */
function fields(line) {
  if (!line.includes('"')) return line.split(",");
  const out = [];
  let cur = "";
  let q = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (q) {
      if (c === '"' && line[i + 1] === '"') {
        cur += '"';
        i++;
      } else if (c === '"') q = false;
      else cur += c;
    } else if (c === '"') q = true;
    else if (c === ",") {
      out.push(cur);
      cur = "";
    } else cur += c;
  }
  out.push(cur);
  return out;
}

function readCsv(path) {
  const lines = readFileSync(path, "utf8").split(/\r?\n/).filter((l) => l.length);
  const header = fields(lines[0]);
  return { header, rows: lines.slice(1).map(fields) };
}

function readPoints(path) {
  const { header, rows } = readCsv(path);
  const pts = {};
  header.forEach((h, k) => {
    pts[h] = h === "id" ? rows.map((r) => r[k]) : Float64Array.from(rows, (r) => Number(r[k]));
  });
  return pts;
}

function head(pts, n) {
  const out = {};
  for (const [k, v] of Object.entries(pts)) out[k] = v.slice(0, n);
  return out;
}

/** Compare a CLI CSV with a JS result, column by column. */
function compare(name, csvPath, js, spec) {
  const { header, rows } = readCsv(csvPath);
  const n = js[spec.rowsFrom].length;
  const problems = [];
  if (rows.length !== n) problems.push(`rows: CLI ${rows.length}, JS ${n}`);
  const col = (h) => header.indexOf(h);
  const over = spec.over ? rows.map((r) => spec.over(r, col)) : null;
  for (const [h, kind] of Object.entries(spec.columns)) {
    const k = col(h);
    if (k < 0) {
      problems.push(`CLI has no column ${h}`);
      continue;
    }
    const v = js[h];
    if (!v) {
      problems.push(`JS has no column ${h}`);
      continue;
    }
    let bad = 0;
    let first = null;
    for (let i = 0; i < Math.min(rows.length, n); i++) {
      const s = rows[i][k];
      let want;
      if (kind === "str") want = s === "" && over && over[i] ? null : s;
      else if (kind === "route") want = s === "" || (over && over[i]) ? null : s;
      else if (kind === "int") want = s === "" || (spec.intLimit !== undefined && Number(s) > spec.intLimit) ? NA : Number(s);
      else if (kind === "naint") want = s === "" || (over && over[i]) ? NA : Number(s);
      else want = s === "" || (over && over[i]) ? NaN : Number(s);
      const got = v[i];
      const same = kind === "dbl" ? Object.is(got, want) : got === want;
      if (!same) {
        bad++;
        if (!first) first = `row ${i + 1}: CLI ${JSON.stringify(s)}, JS ${JSON.stringify(got)}`;
      }
    }
    if (bad) problems.push(`${h}: ${bad} cells differ (first: ${first})`);
  }
  const cells = Object.keys(spec.columns).length * n;
  if (problems.length) {
    console.log(`DIFF  ${name}: ${problems.join("; ")}`);
    return false;
  }
  console.log(`same  ${name}: ${n} rows, ${cells} cells`);
  return true;
}

const timings = [];
function time(_label, f) {
  const t = performance.now();
  const r = f();
  return [r, (performance.now() - t) / 1000];
}

let ok = true;
const cities = process.argv.slice(2);
for (const city of cities.length ? cities : ["poa", "spo"]) {
  const dir = city === "sample" ? join(repo, "python", "peabiru", "data", "spo") : join(X, city);
  const pbfName = readdirSync(dir).filter((f) => f.endsWith(".pbf")).pop();
  const pbfPath = join(dir, pbfName);
  const ptsPath = city === "sample" ? join(dir, "spo_points.csv") : join(dir, `${city}_hexgrid.csv`);
  const pts = readPoints(ptsPath);

  // the order in which the CLI reads the feeds of this directory here
  const probe = join(OUT, `${city}_probe.csv`);
  const run = spawnSync(BIN, ["ttm", dir, ptsPath, "WALK", probe, "1"], { encoding: "utf8" });
  if (run.status !== 0) throw new Error(`${BIN} ttm ${dir}: ${run.stderr}`);
  const ids = JSON.parse(/feeds (\[.*?\])/.exec(run.stderr)[1]);
  const feedFiles = ids.map((id) => {
    const f = `${id}.zip`;
    if (!existsSync(join(dir, f))) throw new Error(`feed id ${id} is not a file name: cannot tell the feed order`);
    return f;
  });
  console.log(`== ${city}: feeds in the CLI's order ${JSON.stringify(feedFiles)}; ${pts.id.length} points`);

  const pbf = readFileSync(pbfPath);
  const gtfs = feedFiles.map((f) => ({ name: f, data: readFileSync(join(dir, f)) }));
  const [streets, tStreets] = await (async () => {
    const t = performance.now();
    const n = await peabiru.buildNetwork({ pbf });
    return [n, (performance.now() - t) / 1000];
  })();
  const [net, tNet] = await (async () => {
    const t = performance.now();
    const n = await peabiru.buildNetwork({ pbf, gtfs });
    return [n, (performance.now() - t) / 1000];
  })();
  const feeds = net.info().feeds;
  if (JSON.stringify(feeds) !== JSON.stringify(ids)) {
    console.log(`DIFF  ${city} feed order: CLI ${JSON.stringify(ids)}, JS ${JSON.stringify(feeds)}`);
    ok = false;
  }
  const buildStreets = [city, "build streets", NaN, tStreets];
  const buildAll = [city, "build with GTFS", NaN, tNet];
  timings.push(buildStreets, buildAll);

  // street matrices
  for (const mode of ["WALK", "BICYCLE", "CAR"]) {
    const f = join(OUT, `${city}_${mode}.csv`);
    const [, tc, tb] = cli(["ttm", pbfPath, ptsPath, mode, f, "1"]);
    if (mode === "WALK") buildStreets[2] = tb;
    const [js, tj] = time(mode, () => peabiru.travelTimeMatrix(streets, pts, pts, { mode }));
    timings.push([city, `street ${mode}`, tc, tj]);
    ok &= compare(`${city} ${mode}`, f, js, {
      rowsFrom: "from_id",
      columns: { from_id: "str", to_id: "str", travel_time_p50: "int" },
      intLimit: 120,
    });
  }

  const when = ["--date", "2019-05-13", "--time", "14:00:00"];
  // transit matrix
  {
    const f = join(OUT, `${city}_TRANSIT.csv`);
    const [, tc, tb] = cli(["transit", dir, ptsPath, f, ...when, "--percentiles", "5,25,50,75,95", "--seed", "42", "--threads", "1"]);
    buildAll[2] = tb;
    const [js, tj] = time("transit", () =>
      peabiru.travelTimeMatrix(net, pts, pts, {
        mode: ["WALK", "TRANSIT"],
        departureDatetime: "2019-05-13 14:00:00",
        percentiles: [5, 25, 50, 75, 95],
        seed: 42,
      }),
    );
    timings.push([city, "transit ttm p5..p95", tc, tj]);
    const columns = { from_id: "str", to_id: "str" };
    for (const p of ["05", "25", "50", "75", "95"]) columns[`travel_time_p${p}`] = "int";
    ok &= compare(`${city} TRANSIT p05..p95`, f, js, { rowsFrom: "from_id", columns, intLimit: 120 });
  }

  // accessibility
  const accCases = [
    {
      name: "transit step",
      net: dir,
      args: ["--mode", "WALK,TRANSIT", "--opportunities", "population,schools", "--cutoffs", "30,60", "--percentiles", "25,50", ...when, "--seed", "42"],
      js: () =>
        peabiru.accessibility(net, pts, pts, {
          mode: ["WALK", "TRANSIT"],
          opportunitiesColnames: ["population", "schools"],
          cutoffs: [30, 60],
          percentiles: [25, 50],
          departureDatetime: "2019-05-13 14:00:00",
          seed: 42,
        }),
    },
    {
      name: "transit logistic",
      net: dir,
      args: ["--mode", "WALK,TRANSIT", "--opportunities", "jobs", "--decay", "logistic", "--decay-value", "8", "--cutoffs", "20,40,60", ...when, "--seed", "7", "--tw", "5"],
      js: () =>
        peabiru.accessibility(net, pts, pts, {
          mode: ["WALK", "TRANSIT"],
          opportunitiesColnames: "jobs",
          decayFunction: "logistic",
          decayValue: 8,
          cutoffs: [20, 40, 60],
          departureDatetime: "2019-05-13 14:00:00",
          seed: 7,
          timeWindow: 5,
        }),
    },
    {
      name: "BICYCLE linear",
      net: pbfPath,
      args: ["--mode", "BICYCLE", "--opportunities", "healthcare,jobs", "--decay", "linear", "--decay-value", "10", "--cutoffs", "15,30"],
      js: () =>
        peabiru.accessibility(streets, pts, pts, {
          mode: "BICYCLE",
          opportunitiesColnames: ["healthcare", "jobs"],
          decayFunction: "linear",
          decayValue: 10,
          cutoffs: [15, 30],
        }),
    },
    {
      name: "WALK fixed_exponential",
      net: pbfPath,
      args: ["--mode", "WALK", "--opportunities", "jobs", "--decay", "fixed_exponential", "--decay-value", "0.05"],
      js: () =>
        peabiru.accessibility(streets, pts, pts, {
          mode: "WALK",
          opportunitiesColnames: "jobs",
          decayFunction: "fixed_exponential",
          decayValue: 0.05,
        }),
    },
  ];
  for (const c of accCases) {
    const f = join(OUT, `${city}_ACC_${c.name.replace(" ", "_")}.csv`);
    const [, tc] = cli(["accessibility", c.net, ptsPath, f, ...c.args, "--threads", "1"]);
    const [js, tj] = time(c.name, c.js);
    timings.push([city, `accessibility ${c.name}`, tc, tj]);
    const columns = { id: "str", opportunity: "str", percentile: "int", accessibility: "dbl" };
    if (js.cutoff) columns.cutoff = "int";
    ok &= compare(`${city} accessibility ${c.name}`, f, js, { rowsFrom: "id", columns });
  }

  // expanded and arrival
  const nOrigins = 50;
  const origins = head(pts, nOrigins);
  const expCases = [
    {
      name: "expanded",
      args: [...when, "--seed", "42", "--breakdown"],
      js: () =>
        peabiru.expandedTravelTimeMatrix(net, origins, pts, {
          mode: ["WALK", "TRANSIT"],
          departureDatetime: "2019-05-13 14:00:00",
          breakdown: true,
          seed: 42,
        }),
      breakdown: true,
    },
    {
      name: "expanded v2",
      args: ["--date", "2019-05-13", "--time", "07:30:30", "--tw", "5", "--draws", "3", "--max-trip", "60", "--max-walk", "15", "--max-rides", "2", "--seed", "7"],
      js: () =>
        peabiru.expandedTravelTimeMatrix(net, origins, pts, {
          mode: "TRANSIT",
          departureDatetime: "2019-05-13 07:30:30",
          timeWindow: 5,
          drawsPerMinute: 3,
          maxTripDuration: 60,
          maxWalkTime: 15,
          maxRides: 2,
          seed: 7,
        }),
      breakdown: false,
      mtd: 60,
    },
    {
      name: "arrival",
      args: ["--arrival", "--date", "2019-05-13", "--time", "16:00:00", "--seed", "42", "--breakdown"],
      js: () =>
        peabiru.arrivalTravelTimeMatrix(net, origins, pts, {
          mode: ["WALK", "TRANSIT"],
          arrivalDatetime: "2019-05-13 16:00:00",
          breakdown: true,
          seed: 42,
        }),
      breakdown: true,
    },
    {
      name: "arrival midnight",
      args: ["--arrival", "--date", "2019-05-14", "--time", "00:30:00", "--seed", "42", "--breakdown"],
      js: () =>
        peabiru.arrivalTravelTimeMatrix(net, origins, pts, {
          mode: ["WALK", "TRANSIT"],
          arrivalDatetime: "2019-05-14 00:30:00",
          breakdown: true,
          seed: 42,
        }),
      breakdown: true,
    },
  ];
  for (const c of expCases) {
    const f = join(OUT, `${city}_EXP_${c.name.replace(" ", "_")}.csv`);
    const [, tc] = cli(["expanded", dir, ptsPath, f, ...c.args, "--origins", String(nOrigins), "--threads", "1"]);
    const [js, tj] = time(c.name, c.js);
    timings.push([city, `${c.name} (${nOrigins} origins)`, tc, tj]);
    const mtd = c.mtd ?? 120;
    const columns = { from_id: "str", to_id: "str", departure_time: "str", draw_number: "int", routes: "route", total_time: "dbl" };
    if (c.breakdown) {
      for (const k of ["access_time", "wait_time", "ride_time", "transfer_time", "egress_time"]) columns[k] = "dbl";
      columns.n_rides = "naint";
    }
    ok &= compare(`${city} ${c.name}`, f, js, {
      rowsFrom: "from_id",
      columns,
      over: (r, col) => r[col("total_time")] !== "" && Number(r[col("total_time")]) > mtd,
    });
  }
  streets.free();
  net.free();
}

console.log("\nseconds (routing only; CLI with 1 thread as it reports it, JS in Node on the calling thread)");
console.log("city  step                                   CLI      JS   JS/CLI");
for (const [c, s, a, b] of timings) {
  const r = Number.isFinite(a) && a > 0 ? (b / a).toFixed(2) : "";
  console.log(`${c.padEnd(5)} ${s.padEnd(36)} ${Number.isFinite(a) ? a.toFixed(3).padStart(7) : "      -"} ${b.toFixed(3).padStart(7)} ${r.padStart(6)}`);
}
console.log(ok ? "\nall identical" : "\nDIFFERENCES FOUND");
process.exit(ok ? 0 : 1);
