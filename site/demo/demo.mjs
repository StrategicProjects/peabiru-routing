// The isochrone demo of the documentation site: build a network in the
// browser and draw isochrones from the point clicked, on a map that can be
// dragged and zoomed (MapLibre GL JS, loaded by the page as `maplibregl`).
// site/build.sh copies the npm package (js/lib and js/pkg) next to this file,
// in peabiru/, the São Paulo sample in data/spo/, and the streets of Recife
// and Caruaru (site/demo/data/pe/, written by site/figures/make.sh
// --pernambuco) in data/pe/.

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
  // streets only: no open GTFS feed was found for either city
  recife: {
    files: ["data/pe/recife.osm.pbf"],
    origin: [-34.8709722, -8.063374], // Marco Zero
    zoom: 11,
    cutoffs: ["10,20,30", "5,10,15", "15,30,45"],
  },
  caruaru: {
    files: ["data/pe/caruaru.osm.pbf"],
    origin: [-35.9713866, -8.2897512], // Feira de Caruaru
    zoom: 11,
    cutoffs: ["10,20,30", "5,10,15", "15,30,45"],
  },
};
const hasTransit = (city) => CITIES[city].files.some((f) => f.endsWith(".zip"));

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
  return lines; // a GeoJSON FeatureCollection of LineStrings, with `street_class`
}
// ANCHOR_END: streets

// ---------------------------------------------------------------- the map

const statusEl = document.querySelector("#status");
const legendEl = document.querySelector("#legend");
const form = document.querySelector("#controls");
const citySel = document.querySelector("#city");
const cutoffSel = document.querySelector("#cutoffs");
const timeInput = document.querySelector("#time");
const basemapInput = document.querySelector("#basemap");

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
const dark = () => matchMedia("(prefers-color-scheme: dark)").matches;
// near = strongest, as in the figures of the documentation
const RAMP = {
  light: ["#7a3318", "#a84b25", "#cf7445", "#e8a57b", "#f0c19d", "#f5d3b6"],
  dark: ["#f7c9a6", "#ee9c6e", "#d46f3d", "#a2502a", "#87432a", "#6a3820"],
};
const rampColor = (i, n) => {
  const ramp = RAMP[dark() ? "dark" : "light"];
  return ramp[Math.round((i * (ramp.length - 2)) / Math.max(n - 1, 1))];
};
const EMPTY = { type: "FeatureCollection", features: [] };

let state = { city: null, network: null, streets: EMPTY, bounds: null, origin: null, isochrones: null };

// The background: OpenFreeMap's light or dark style, or nothing at all (the
// streets of the network are then the whole map).
function mapStyle() {
  if (basemapInput.checked) return `https://tiles.openfreemap.org/styles/${dark() ? "dark" : "positron"}`;
  return {
    version: 8,
    sources: {},
    layers: [{ id: "paper", type: "background", paint: { "background-color": css("--pb-card") || "#fffdf9" } }],
  };
}

const map = new maplibregl.Map({
  container: "map",
  style: mapStyle(),
  center: CITIES[citySel.value].origin,
  zoom: 11,
  attributionControl: { compact: true },
  dragRotate: false,
  pitchWithRotate: false,
});
map.touchZoomRotate.disableRotation();
map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "top-right");
map.addControl(new maplibregl.ScaleControl({ unit: "metric" }), "bottom-left");
map.getCanvas().style.cursor = "crosshair";

const pin = document.createElement("div");
pin.className = "origin-pin";
const marker = new maplibregl.Marker({ element: pin });

// The layers of this page, put back after every change of style: the
// isochrones and the streets of the network, over the land, the water and the
// buildings of the background map and under its roads and its labels.
function decorate() {
  const based = basemapInput.checked;
  const layers = map.getStyle().layers;
  const labels = (layers.find((l) => /^(tunnel|highway|road|aeroway)/.test(l.id)) ??
    layers.find((l) => l.type === "symbol"))?.id;
  if (!map.getSource("streets")) map.addSource("streets", { type: "geojson", data: state.streets });
  if (!map.getSource("iso")) map.addSource("iso", { type: "geojson", data: isoData() });
  if (!map.getLayer("iso")) {
    map.addLayer({
      id: "iso",
      type: "fill",
      source: "iso",
      paint: { "fill-color": ["get", "fill"], "fill-opacity": based ? 0.62 : 0.78, "fill-antialias": false },
    }, labels);
  }
  if (!map.getLayer("streets")) {
    const wide = ["match", ["get", "street_class"], "MOTORWAY", 2.2, "PRIMARY", 1.8, "SECONDARY", 1.3, "TERTIARY", 1, 0.5];
    map.addLayer({
      id: "streets",
      type: "line",
      source: "streets",
      layout: { "line-cap": "round", "line-join": "round" },
      paint: {
        // alone, the streets are the map; over a background map they are a
        // faint trace of the network that is routed
        "line-color": based ? (dark() ? "#e98a5c" : "#8f3f1d") : (dark() ? "#3b4749" : "#d9cfbf"),
        "line-opacity": based ? 0.16 : 1,
        "line-width": based ? ["interpolate", ["linear"], ["zoom"], 10, 0.4, 15, 1.2]
          : ["interpolate", ["linear"], ["zoom"], 10, ["*", wide, 0.8], 15, ["*", wide, 2.4]],
      },
    }, based ? labels : "iso"); // alone, the streets go under the isochrones
  }
}

function isoData() {
  if (!state.isochrones) return EMPTY;
  const n = state.isochrones.length;
  // largest first (as returned), so that the shorter times are drawn on top
  return {
    type: "FeatureCollection",
    features: state.isochrones.map((f, i) => ({ ...f, properties: { ...f.properties, fill: rampColor(n - 1 - i, n) } })),
  };
}

function draw() {
  map.getSource("streets")?.setData(state.streets);
  map.getSource("iso")?.setData(isoData());
  if (state.origin) marker.setLngLat(state.origin).addTo(map); else marker.remove();
}

function boundsOf(lines) {
  const b = new maplibregl.LngLatBounds();
  for (const f of lines.features) for (const p of f.geometry.coordinates) b.extend(p);
  return b;
}
const fit = (animate = true) => state.bounds && map.fitBounds(state.bounds, { padding: 28, animate });

function restyle() {
  map.once("style.load", () => { decorate(); draw(); }); // before setStyle: an inline style loads at once
  map.setStyle(mapStyle(), { diff: false });
}
map.on("load", () => { decorate(); draw(); });

function legend() {
  if (!state.isochrones) { legendEl.textContent = ""; return; }
  const n = state.isochrones.length;
  legendEl.replaceChildren(...[...state.isochrones].reverse().map((f, i) => {
    const span = document.createElement("span");
    const sw = document.createElement("i");
    sw.style.background = rampColor(i, n);
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

// Public transport and the departure time only where the city has GTFS feeds.
function transitControls(city) {
  const transitInput = form.querySelector("input[value='WALK,TRANSIT']");
  const on = hasTransit(city);
  transitInput.closest("label").hidden = !on;
  timeInput.closest("label").hidden = !on;
  if (!on && transitInput.checked) form.querySelector("input[value='WALK']").checked = true;
}

async function loadCity(city) {
  const c = CITIES[city];
  transitControls(city);
  cutoffSel.replaceChildren(...c.cutoffs.map((v) => new Option(v.replaceAll(",", ", "), v)));
  form.querySelectorAll("select, input").forEach((el) => { el.disabled = true; });
  state.network?.free();
  state = { city, network: null, streets: EMPTY, bounds: null, origin: c.origin, isochrones: null };
  legend();
  draw();
  map.jumpTo({ center: c.origin, zoom: 12 });
  try {
    statusEl.textContent = city === "poa" ? "Downloading the Porto Alegre files (2 MB) and building the network…"
      : "Building the network…";
    const t0 = performance.now();
    state.network = await networkFrom(c.files);
    const info = state.network.info();
    state.streets = await streetsOf(state.network, c.origin);
    state.bounds = boundsOf(state.streets);
    fit(false);
    statusEl.textContent = `Network built in ${Math.round(performance.now() - t0)} ms: ` +
      `${info.edges.toLocaleString("en")} street edges` +
      (info.stops ? `, ${info.stops.toLocaleString("en")} stops.` : " (streets only).");
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

map.on("click", (e) => {
  if (!state.network) return;
  state.origin = [e.lngLat.lng, e.lngLat.lat];
  update();
});
document.querySelector("#fit").addEventListener("click", () => fit());
citySel.addEventListener("change", () => loadCity(citySel.value));
basemapInput.addEventListener("change", restyle);
form.addEventListener("change", (e) => {
  if (e.target !== citySel && e.target !== basemapInput) update();
});
form.addEventListener("submit", (e) => e.preventDefault());
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => { legend(); restyle(); });

loadCity(citySel.value).catch((e) => {
  statusEl.textContent = `Could not start: ${e.message || e}`;
  statusEl.classList.add("error");
});
