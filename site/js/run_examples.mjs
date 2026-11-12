#!/usr/bin/env node
// Run every example of the JavaScript guide (site/js/examples/) against the
// npm package in js/ (built with `sh js/build.sh`) and the sample network of
// the Python package (python/peabiru/data/spo/), and compare what each one
// prints with its expected output in site/js/examples/output/, which the
// guide shows next to the code.
//
//   node site/js/run_examples.mjs              run all, fail on any difference
//   node site/js/run_examples.mjs --update     rewrite the expected outputs
//   node site/js/run_examples.mjs threads      only the examples named
//
// Node examples (*.mjs) run with `node` in a scratch directory that holds
// the sample files, the small elevation rasters of js/tests/data and
// node_modules/peabiru, a link to js/, so that they read "spo_osm.pbf" and
// import "peabiru" as a user's script would (the examples are copied there:
// Node resolves imports from a script's real path). sample*.mjs are helpers
// the examples import, not examples; the itinerary example downloads r5r's
// Porto Alegre sample once into the temporary directory (sample-poa.mjs).
//
// Browser examples (browser-*.mjs) run in headless Chrome or Chromium, in
// the page browser.html of the same directory, served over HTTP: as plain
// ES modules (with the page's import map) and, when VITE names a Vite
// executable, once more bundled by `vite build`. CHROME names the browser
// (default: the usual install paths). Without a browser they are skipped,
// with a message, unless --require-browser is given.
//
// The examples in the doc comments of js/lib/index.d.ts, shown in the API
// reference, run after the setup in sample-api.mjs and must exit with
// status 0 (`node site/js/run_examples.mjs api` runs only those).
//
// The JavaScript examples of the home page (site/index.html) and of the
// README at the root follow a marker <!-- example: FILE:ANCHOR -->; the
// code block after it must be that part of the example (--update rewrites
// it).
//
// Exit status: 0 if every example ran and printed its expected output, and
// the pages show the examples as they are.

import { spawn, spawnSync } from "node:child_process";
import {
  copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync,
  symlinkSync, writeFileSync,
} from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = resolve(here, "../..");
const examples = join(here, "examples");
const expected = join(examples, "output");
const sample = join(root, "python/peabiru/data/spo");
const SAMPLE_FILES = ["spo_osm.pbf", "spo_gtfs.zip", "spo_points.csv"];
// the small elevation rasters of the tests (synthetic hills over the sample)
const rasters = join(root, "js/tests/data");
const RASTER_FILES = ["spo_hills.tif", "spo_hills_31983.tif", "spo_voids.tif"];
const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".csv": "text/csv; charset=utf-8",
  ".zip": "application/zip",
};

const args = process.argv.slice(2);
const update = args.includes("--update");
const requireBrowser = args.includes("--require-browser");
const only = args.filter((a) => !a.startsWith("--"));

if (!existsSync(join(root, "js/pkg/peabiru_bg.wasm"))) {
  console.error("js/pkg/peabiru_bg.wasm not found: build the package first with `sh js/build.sh`");
  process.exit(1);
}

const all = readdirSync(examples)
  // sample*.mjs are helpers the examples import
  .filter((f) => f.endsWith(".mjs") && !f.startsWith("sample"))
  .map((f) => f.slice(0, -4))
  .sort();
for (const name of only) {
  if (!all.includes(name) && name !== "api") {
    console.error(`no example ${name} (have: ${all.join(", ")}, and api for the examples of the API reference)`);
    process.exit(2);
  }
}
const chosen = only.length ? all.filter((n) => only.includes(n)) : all;
const nodeExamples = chosen.filter((n) => !n.startsWith("browser-"));
const browserExamples = chosen.filter((n) => n.startsWith("browser-"));

// The scratch directory stands for a user's project.
const work = mkdtempSync(join(tmpdir(), "peabiru-examples-"));
mkdirSync(join(work, "node_modules"));
symlinkSync(join(root, "js"), join(work, "node_modules/peabiru"), "dir");
for (const f of SAMPLE_FILES) symlinkSync(join(sample, f), join(work, f));
for (const f of RASTER_FILES) symlinkSync(join(rasters, f), join(work, f));
// a copy of the module at a path of its own, for the example of init(url)
mkdirSync(join(work, "static"));
copyFileSync(join(root, "js/pkg/peabiru_bg.wasm"), join(work, "static/peabiru_bg.wasm"));
for (const f of readdirSync(examples)) {
  if (f.endsWith(".mjs") || f.endsWith(".html")) copyFileSync(join(examples, f), join(work, f));
}

let failures = 0;
let ran = 0;
const skipped = [];

/** Compare an example's output with the expected one (or write it). */
function check(name, out, label = name) {
  ran++;
  const file = join(expected, `${name}.txt`);
  if (update) {
    writeFileSync(file, out);
    console.log(`wrote  ${label}`);
    return;
  }
  const want = existsSync(file) ? readFileSync(file, "utf8") : null;
  if (want === out) {
    console.log(`ok     ${label}`);
    return;
  }
  failures++;
  console.log(`FAIL   ${label}: output differs from examples/output/${name}.txt`);
  console.log("--- expected\n" + (want ?? "(no file)\n") + "--- got\n" + out);
}

function fail(label, message) {
  failures++;
  console.log(`FAIL   ${label}: ${message}`);
}

// ---------------------------------------------------------------- pages

/** The lines of a file between `ANCHOR: name` and `ANCHOR_END: name`, as mdBook includes them. */
function anchored(file, name) {
  const lines = readFileSync(join(root, file), "utf8").split("\n");
  const start = lines.findIndex((l) => l.includes(`ANCHOR: ${name}`));
  const end = lines.findIndex((l) => l.includes(`ANCHOR_END: ${name}`));
  if (start < 0 || end < start) throw new Error(`${file}: no anchor ${name}`);
  return lines.slice(start + 1, end).filter((l) => !/ANCHOR(_END)?:/.test(l)).join("\n");
}

const escapeHtml = (t) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

for (const page of ["site/index.html", "README.md"]) {
  const path = join(root, page);
  const text = readFileSync(path, "utf8");
  const html = page.endsWith(".html");
  const marker = /<!-- example: (\S+):(\S+)[^>]*-->\n/g;
  let out = "";
  let at = 0;
  let m;
  while ((m = marker.exec(text))) {
    const code = anchored(m[1], m[2]);
    const [open, close] = html ? ["<pre><code>", "</code></pre>"] : ["```js\n", "\n```"];
    const from = m.index + m[0].length;
    const end = text.indexOf(close, from);
    if (!text.startsWith(open, from) || end < 0) {
      fail(page, `no code block after the marker for ${m[1]}:${m[2]}`);
      continue;
    }
    const want = html
      ? escapeHtml(code).replace(/^(\s*)(\/\/.*)$/gm, '$1<span class="c">$2</span>')
      : code;
    const have = text.slice(from + open.length, end);
    out += text.slice(at, from + open.length) + want;
    at = end;
    const label = `${page} (${m[1].split("/").pop()}:${m[2]})`;
    if (have === want) console.log(`ok     ${label}`);
    else if (update) console.log(`wrote  ${label}`);
    else fail(label, "the code block differs from the example: run with --update");
  }
  out += text.slice(at);
  if (update && out !== text) writeFileSync(path, out);
}

// ---------------------------------------------------------------- Node

for (const name of nodeExamples) {
  const r = spawnSync(process.execPath, [`${name}.mjs`], { cwd: work, encoding: "utf8", timeout: 300_000 });
  if (r.status !== 0) {
    fail(name, `exit status ${r.status ?? r.signal}\n${r.stdout}${r.stderr}`);
    continue;
  }
  if (r.stderr) process.stderr.write(r.stderr.replace(/^/gm, `  ${name}: `));
  check(name, r.stdout);
}

// ---------------------------------------------------------------- API reference

// The examples in the doc comments of js/lib/index.d.ts (```js blocks),
// which the API reference shows: each runs after the setup of
// sample-api.mjs and must exit with status 0. Every exported value,
// function and class needs one.
if (!only.length || only.includes("api")) {
  const setup = anchored("site/js/examples/sample-api.mjs", "setup");
  const dts = readFileSync(join(root, "js/lib/index.d.ts"), "utf8");
  const documented = /\/\*\*((?:(?!\*\/)[\s\S])*)\*\/\s*\nexport (?:declare )?(const|function|class|type|interface) (\w+)/g;
  const seen = new Set();
  for (const [, doc, kind, name] of dts.matchAll(documented)) {
    const lines = doc.split("\n").map((l) => l.replace(/^\s*\* ?/, ""));
    const blocks = [];
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].trim() !== "```js") continue;
      const end = lines.findIndex((l, k) => k > i && l.trim() === "```");
      blocks.push(lines.slice(i + 1, end).join("\n"));
      i = end;
    }
    if (blocks.length) seen.add(name);
    blocks.forEach((code, k) => {
      const file = `api-${name}${k ? `-${k + 1}` : ""}.mjs`;
      writeFileSync(join(work, file), `${setup}\n\n${code}\n`);
      const r = spawnSync(process.execPath, [file], { cwd: work, encoding: "utf8", timeout: 300_000 });
      ran++;
      if (r.status !== 0) fail(`api: ${name}`, `exit status ${r.status ?? r.signal}\n${r.stdout}${r.stderr}`);
      else console.log(`ok     api: ${name}`);
    });
    if (["const", "function", "class"].includes(kind) && !seen.has(name)) fail(`api: ${name}`, "no example in its doc comment in js/lib/index.d.ts");
  }
}

// ---------------------------------------------------------------- browser

if (browserExamples.length) {
  const chrome = findChrome();
  if (!chrome) {
    const msg = `no Chrome or Chromium found (set CHROME): ${browserExamples.join(", ")} not run`;
    if (requireBrowser) fail("browser", msg);
    else {
      console.log(`skip   ${msg}`);
      skipped.push(...browserExamples);
    }
  } else {
    await runInBrowser(chrome, work, browserExamples, "");
    const vite = process.env.VITE;
    if (vite) {
      // the same page and examples, bundled; the sample files are fetched
      // at run time, so they go next to the bundle
      writeFileSync(join(work, "vite.config.mjs"),
        'export default { logLevel: "warn", build: { outDir: "dist", rollupOptions: { input: "browser.html" } } };\n');
      const r = spawnSync(vite, ["build"], { cwd: work, encoding: "utf8", timeout: 300_000 });
      if (r.status !== 0) fail("vite build", `exit status ${r.status ?? r.signal}\n${r.stdout}${r.stderr}`);
      else {
        for (const f of SAMPLE_FILES) symlinkSync(join(sample, f), join(work, "dist", f));
        mkdirSync(join(work, "dist/static"));
        copyFileSync(join(work, "static/peabiru_bg.wasm"), join(work, "dist/static/peabiru_bg.wasm"));
        await runInBrowser(chrome, join(work, "dist"), browserExamples, " (Vite)");
      }
    } else console.log("note   VITE not set: the browser examples were not bundled with Vite");
  }
}

rmSync(work, { recursive: true, force: true });
if (failures) {
  console.log(`\n${failures} failure(s)`);
  process.exit(1);
}
console.log(`\nall ${ran} run(s) of the examples gave their expected output` +
  (skipped.length ? ` (not run: ${skipped.join(", ")})` : ""));

function findChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  for (const c of [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
  ]) if (existsSync(c)) return c;
  for (const c of ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"]) {
    const r = spawnSync("sh", ["-c", `command -v ${c}`], { encoding: "utf8" });
    if (r.status === 0 && r.stdout.trim()) return r.stdout.trim();
  }
  return null;
}

/**
 * Serve `dir` over HTTP, open its browser.html in headless Chrome with the
 * examples to run, and take the page's reports (POST /__report): one per
 * example, with what it printed with console.log.
 */
async function runInBrowser(chrome, dir, names, suffix) {
  const reports = new Map();
  let wake = null;
  // the .wasm files fetched since the last report: each example is a page load
  let wasm = [];
  const server = createServer((req, res) => {
    if (req.method === "POST" && req.url === "/__report") {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const r = JSON.parse(body);
        r.wasm = wasm;
        wasm = [];
        reports.set(r.name, r);
        res.end("ok");
        wake?.();
      });
      return;
    }
    const path = decodeURIComponent(new URL(req.url, "http://x").pathname);
    if (path.endsWith(".wasm")) wasm.push(path);
    const file = resolve(dir, "." + path);
    if (!file.startsWith(dir + sep) || !existsSync(file) || !statSync(file).isFile()) {
      res.writeHead(404).end("not found");
      return;
    }
    res.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream" });
    res.end(readFileSync(file));
  });
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const url = `http://127.0.0.1:${server.address().port}/browser.html?run=${names.join(",")}`;
  const profile = mkdtempSync(join(tmpdir(), "peabiru-chrome-"));
  const proc = spawn(chrome, [
    "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
    "--disable-extensions", `--user-data-dir=${profile}`,
    // Linux CI runners (Ubuntu 24.04) do not let Chrome set up its sandbox
    ...(process.platform === "linux" ? ["--no-sandbox"] : []),
    url,
  ], { stdio: "ignore" });
  const deadline = Date.now() + 300_000;
  while (reports.size < names.length && Date.now() < deadline && proc.exitCode === null) {
    await new Promise((ok) => {
      wake = ok;
      setTimeout(ok, 1000);
    });
  }
  if (proc.exitCode === null) {
    const exited = new Promise((ok) => proc.once("exit", ok));
    proc.kill();
    await exited;
  }
  server.close();
  rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  for (const name of names) {
    const r = reports.get(name);
    if (!r) fail(name + suffix, "no report from the browser");
    else if (!r.ok) fail(name + suffix, r.error);
    else if (name === "browser-init" && r.wasm.join() !== "/static/peabiru_bg.wasm") {
      fail(name + suffix, `fetched ${r.wasm.join(", ") || "no .wasm file"} instead of /static/peabiru_bg.wasm only`);
    } else check(name, r.output, name + suffix);
  }
}
