// The isochrone demo of the documentation site: build a network in the
// browser, draw its streets, and draw isochrones from the point clicked.
// site/build.sh copies the npm package (js/lib and js/pkg) next to this file,
// in peabiru/, and the São Paulo sample in data/spo/.

// ANCHOR: load
import * as peabiru from "./peabiru/lib/index.js";

const CITIES = {
  spo: {
    files: ["data/spo/spo_osm.pbf", "data/spo/spo_gtfs.zip"],
    origin: [-46.6290, -23.5563], // a point of the sample, by Avenida Paulista
    zoom: 12,
    cutoffs: ["5,10,15", "10,20,30", "5,10,15,20,25,30"],
  },
  poa: {
    files: ["poa_osm.pbf", "poa_eptc.zip", "poa_trensurb.zip"].map(
      (f) => `https://raw.githubusercontent.com/ipea/r5r/v2.5.0/r-package/inst/extdata/poa/${f}`,
    ),
    origin: [-51.216111, -30.036746], // Farroupilha Park
    zoom: 11,
    cutoffs: ["10,20,30", "15,30,45", "10,20,30,40,50,60"],
  },
};

/** Build a network from the URLs of one .pbf and any GTFS .zip files. */
async function networkFrom(urls) {
  const bytes = async (url) => {
    const r = await fetch(url);
    if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
    return new Uint8Array(await r.arrayBuffer());
  };
  const name = (url) => url.split("/").pop();
  const pbf = urls.find((u) => u.endsWith(".pbf"));
  const gtfs = urls.filter((u) => u.endsWith(".zip"));
  return peabiru.buildNetwork({
    pbf: await bytes(pbf),
    gtfs: await Promise.all(gtfs.map(async (u) => ({ name: name(u), data: await bytes(u) }))),
  });
}
// ANCHOR_END: load

// ANCHOR: streets
/** The street network as GeoJSON lines: every segment a car reaches from a
 *  point within two hours, which on a city extract is every street. */
async function streetsOf(network, [lon, lat]) {
  const lines = await peabiru.isochrone(network, { id: ["o"], lon: [lon], lat: [lat] }, {
    mode: "CAR",
    cutoffs: [120],
    polygonOutput: false,
  });
  return lines.features.map((f) => ({ cls: f.properties.street_class, xy: f.geometry.coordinates }));
}
// ANCHOR_END: streets

// ---------------------------------------------------------------- drawing

const canvas = document.querySelector("#map");
const ctx = canvas.getContext("2d");
const statusEl = document.querySelector("#status");
const legendEl = document.querySelector("#legend");
const form = document.querySelector("#controls");
const citySel = document.querySelector("#city");
const cutoffSel = document.querySelector("#cutoffs");
const timeInput = document.querySelector("#time");

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
const dark = () => matchMedia("(prefers-color-scheme: dark)").matches;
// near = strongest, as in the figures of the documentation
const RAMP = {
  light: ["#7a3318", "#a84b25", "#cf7445", "#e8a57b", "#f0c19d", "#f5d3b6"],
  dark: ["#f7c9a6", "#ee9c6e", "#d46f3d", "#a2502a", "#87432a", "#6a3820"],
};

let state = { city: null, network: null, streets: [], origin: null, isochrones: null, view: null };

const merc = ([lon, lat]) => [
  (lon * Math.PI) / 180,
  Math.log(Math.tan(Math.PI / 4 + (lat * Math.PI) / 360)),
];

function fitView() {
  let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const s of state.streets) {
    for (const p of s.xy) {
      const [x, y] = merc(p);
      x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y);
    }
  }
  const w = canvas.clientWidth, h = canvas.clientHeight, pad = 12;
  const k = Math.min((w - 2 * pad) / (x1 - x0), (h - 2 * pad) / (y1 - y0));
  const ox = (w - k * (x1 - x0)) / 2, oy = (h - k * (y1 - y0)) / 2;
  state.view = {
    toPx: (p) => { const [x, y] = merc(p); return [ox + k * (x - x0), oy + k * (y1 - y)]; },
    toLonLat: (px, py) => {
      const x = (px - ox) / k + x0, y = y1 - (py - oy) / k;
      return [(x * 180) / Math.PI, (Math.atan(Math.sinh(y)) * 180) / Math.PI];
    },
  };
}

function draw() {
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth, h = canvas.clientHeight;
  if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = css("--pb-surface");
  ctx.fillRect(0, 0, w, h);
  if (!state.view) return;
  const { toPx } = state.view;

  // streets, thin to thick
  const width = { MOTORWAY: 2.2, PRIMARY: 1.8, SECONDARY: 1.3, TERTIARY: 1, OTHER: 0.5 };
  ctx.strokeStyle = dark() ? "#3b4749" : "#d9cfbf";
  ctx.lineCap = "round";
  for (const cls of ["OTHER", "TERTIARY", "SECONDARY", "PRIMARY", "MOTORWAY"]) {
    ctx.lineWidth = width[cls];
    ctx.beginPath();
    for (const s of state.streets) {
      if (s.cls !== cls) continue;
      s.xy.forEach((p, i) => { const [x, y] = toPx(p); i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); });
    }
    ctx.stroke();
  }

  // isochrones, largest first (as returned), with the shorter times on top
  if (state.isochrones) {
    const ramp = RAMP[dark() ? "dark" : "light"];
    const n = state.isochrones.length;
    state.isochrones.forEach((f, i) => {
      ctx.beginPath();
      for (const polygon of f.geometry.coordinates) {
        for (const ring of polygon) {
          ring.forEach((p, j) => { const [x, y] = toPx(p); j ? ctx.lineTo(x, y) : ctx.moveTo(x, y); });
          ctx.closePath();
        }
      }
      ctx.globalAlpha = 0.78;
      ctx.fillStyle = ramp[Math.round(((n - 1 - i) * (ramp.length - 2)) / Math.max(n - 1, 1))];
      ctx.fill("evenodd");
      ctx.globalAlpha = 1;
    });
  }
  if (state.origin) {
    const [x, y] = toPx(state.origin);
    ctx.beginPath();
    ctx.arc(x, y, 6, 0, 2 * Math.PI);
    ctx.fillStyle = css("--pb-ink");
    ctx.fill();
    ctx.lineWidth = 2.5;
    ctx.strokeStyle = css("--pb-surface");
    ctx.stroke();
  }
}

function legend() {
  if (!state.isochrones) { legendEl.textContent = ""; return; }
  const ramp = RAMP[dark() ? "dark" : "light"];
  const n = state.isochrones.length;
  legendEl.replaceChildren(...[...state.isochrones].reverse().map((f, i) => {
    const span = document.createElement("span");
    const sw = document.createElement("i");
    sw.style.background = ramp[Math.round((i * (ramp.length - 2)) / Math.max(n - 1, 1))];
    span.append(sw, `${f.properties.isochrone} min`);
    return span;
  }));
}

// ---------------------------------------------------------------- routing

const mode = () => form.elements.mode.value.split(",");

// ANCHOR: isochrone
async function update() {
  if (!state.network || !state.origin) return;
  const [lon, lat] = state.origin;
  const transit = mode().includes("TRANSIT");
  statusEl.textContent = "Routing…";
  statusEl.classList.remove("error");
  await new Promise(requestAnimationFrame); // let the status show before the work starts
  const t0 = performance.now();
  try {
    const iso = await peabiru.isochrone(state.network, { id: ["origin"], lon: [lon], lat: [lat] }, {
      mode: mode(),
      cutoffs: cutoffSel.value.split(",").map(Number),
      zoom: CITIES[state.city].zoom,
      departureDatetime: `2019-05-13 ${timeInput.value || "14:00"}:00`,
      timeWindow: 10,
      seed: 42, // frequency-based feeds (São Paulo) draw departures at random
    });
    state.isochrones = iso.features; // one MultiPolygon per cutoff, largest first
    const ms = Math.round(performance.now() - t0);
    statusEl.textContent = `${mode().join(" + ").toLowerCase()} from ${lat.toFixed(4)}, ${lon.toFixed(4)}` +
      `${transit ? ` at ${timeInput.value}` : ""}: computed in ${ms} ms. Click the map to move the origin.`;
  } catch (e) {
    state.isochrones = null;
    statusEl.textContent = String(e.message || e);
    statusEl.classList.add("error");
  }
  legend();
  draw();
}
// ANCHOR_END: isochrone

async function loadCity(city) {
  const c = CITIES[city];
  cutoffSel.replaceChildren(...c.cutoffs.map((v) => new Option(v.replaceAll(",", ", "), v)));
  form.querySelectorAll("select, input").forEach((el) => { el.disabled = true; });
  state.network?.free();
  state = { city, network: null, streets: [], origin: c.origin, isochrones: null, view: null };
  legend();
  draw();
  try {
    statusEl.textContent = city === "poa" ? "Downloading the Porto Alegre files (2 MB) and building the network…"
      : "Building the network…";
    const t0 = performance.now();
    state.network = await networkFrom(c.files);
    const info = state.network.info();
    state.streets = await streetsOf(state.network, c.origin);
    fitView();
    statusEl.textContent = `Network built in ${Math.round(performance.now() - t0)} ms: ` +
      `${info.edges.toLocaleString("en")} street edges, ${info.stops.toLocaleString("en")} stops.`;
  } catch (e) {
    statusEl.textContent = `Could not load the network: ${e.message || e}`;
    statusEl.classList.add("error");
    return;
  } finally {
    form.querySelectorAll("select, input").forEach((el) => { el.disabled = false; });
  }
  await update();
}

// ---------------------------------------------------------------- events

canvas.addEventListener("click", (e) => {
  if (!state.view || !state.network) return;
  const r = canvas.getBoundingClientRect();
  state.origin = state.view.toLonLat(e.clientX - r.left, e.clientY - r.top);
  update();
});
citySel.addEventListener("change", () => loadCity(citySel.value));
form.addEventListener("change", (e) => {
  if (e.target !== citySel) update();
});
form.addEventListener("submit", (e) => e.preventDefault());
let resizeTimer;
window.addEventListener("resize", () => {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => { if (state.streets.length) fitView(); draw(); }, 100);
});
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => { legend(); draw(); });

loadCity(citySel.value).catch((e) => {
  statusEl.textContent = `Could not start: ${e.message || e}`;
  statusEl.classList.add("error");
});
