# Documentation sites

Everything published about peabiru is built from this repository into
`_site/` (gitignored):

| Path | Source | Tool |
|---|---|---|
| `_site/` | `site/index.html`, `site/benchmarks.html`, `site/style.css`, `site/assets/`, `site/data/` | static files |
| `_site/r/` | `r/` (`_pkgdown.yml`, `vignettes/`, `pkgdown/`) | pkgdown |
| `_site/rust/` | `site/rust/` (mdBook) and `crates/peabiru-core/examples/` | mdBook |
| `_site/rust/api/` | `crates/peabiru-core` | `cargo doc --no-deps` |
| `_site/python/` | `python/mkdocs.yml`, `python/docs/` | MkDocs (Material) |
| `_site/js/` | `site/js/` (mdBook), `site/js/examples/`, and `js/lib/index.d.ts` for the API reference | mdBook |

## Build and preview

```sh
site/build.sh                      # everything
site/build.sh --skip-r             # faster: no pkgdown (also --skip-rust, --skip-python, --skip-js)
python3 -m http.server -d _site 8000   # then open http://localhost:8000/
```

Requirements: Python 3, R with pkgdown (>= 2.2), knitr and rmarkdown, the
peabiru R package installed (set `PEABIRU_R_LIB` to a library that has it;
otherwise the script installs it from `r/` into `.claude/scratch/site-rlib`),
mdBook, and a Rust toolchain. The Python site needs `mkdocs` on the `PATH`, in
`python/.venv`, or `uvx`; without `python/mkdocs.yml` a placeholder page is
written instead.

## The JavaScript guide

`site/js/` is an mdBook, like the Rust guide, and shares its theme
(`site/rust/theme/peabiru.css` and `peabiru-nav.js`, the links to the other
sites in the menu bar of both guides; `site/build.sh` copies them, with the
favicon, into `site/js/theme/`). Two scripts keep it from going stale:

- `site/js/gen_api.py` writes the API reference, `site/js/src/api.md`, from
  `js/lib/index.d.ts` (its declarations and their doc comments). The build
  runs it; `--check` fails if the page is out of date, or if `js/lib/index.js`
  and `index.d.ts` export different values.
- `site/js/run_examples.mjs` runs every example of the guide
  (`site/js/examples/*.mjs`, included in the pages with mdBook's
  `{{#include file:anchor}}`) on the sample network of the Python package and
  compares what each prints with `site/js/examples/output/*.txt`, which the
  pages also include; `--update` rewrites those files. The Node examples run
  with `node`, the browser ones (`browser-*.mjs`) in headless Chrome (set
  `CHROME` if it is not found), as plain ES modules and, when `VITE` names a
  Vite executable, bundled by Vite. It also checks the JavaScript example of
  the home page and of the root README against `examples/landing.mjs`. It
  needs the package built (`sh js/build.sh`), so the build does not run it.

The script ends with `site/check_links.py`, which fails if any local link or
image of the built site is missing. Links between the sub-sites are relative,
so `_site/` can be served from any path; nothing is deployed.

## Shared assets

`site/assets/` is the single source of the brand and the diagrams:

- `logo.svg` (mark + wordmark), `logo-mark.svg` (square mark), `favicon.svg`,
  `brand.css` (palette as `--pb-*` custom properties, light and dark);
  `r/man/figures/logo.svg` and `logo.png` are the R hexagon sticker;
  `concepts/` keeps the logo concepts that were not chosen.
- `diagrams/*.svg`: hand-written SVG, each with a `<title>`, a `<desc>` and its
  own light/dark styles (`prefers-color-scheme`), so they read on any page.
  The R articles copy them next to themselves at render time
  (`r/vignettes/articles/setup.R`); the Rust guide links to `../assets/`.
- `charts/*.svg`: generated, see below.
- `figures/*`: the figures of the gallery and of the worked examples,
  computed by the packages; see below.

For the Python docs the agreed contract is: `site/build.sh` copies
`logo-mark.svg` and `favicon.svg` to `python/docs/assets/` and `brand.css` to
`python/docs/stylesheets/brand.css` before building them.

## Figures, worked examples and the demo

The gallery of the home page and the worked examples show real output.
`bash site/figures/make.sh` regenerates `site/assets/figures/*.svg` with
`site/figures/figures.R` (the peabiru R package with ggplot2, sf and
svglite, on the Porto Alegre sample of r5r 2.5.0, downloaded once into a
cache directory, and the São Paulo sample of the package); `--demo` also
retakes `demo-poa.webp`, a screenshot of the demo page (needs the site
built, playwright and cwebp). The SVGs carry their alt text (`<title>`,
`<desc>`) and switch to the dark palette with `prefers-color-scheme`, like
the diagrams. Run it after a release that changes results, then rebuild.

`bash site/figures/make.sh --pernambuco` does the same for Recife and
Caruaru, from open data rather than a sample: `site/figures/pernambuco-data.R`
downloads (once, into `PEABIRU_PE_CACHE`, by default
`tools::R_user_dir("peabiru", "cache")/pernambuco`, about 640 MB) Geofabrik's
dated extract of the Nordeste, IBGE's municipal boundaries and Censo 2022
census tracts of Pernambuco, INEP's schools (via geobr) and CNES's
establishments, cuts the streets with osmium (municipality plus 10 km for
Recife, 5 km for Caruaru), and builds a hexagonal grid with the population;
`site/figures/pernambuco.R` draws `isochrones-recife.svg`,
`isochrones-caruaru.svg`, `nearest-recife.svg`, `nearest-caruaru.svg` and
`access-recife-caruaru.svg`, and writes the extracts of the demo,
`site/demo/data/pe/{recife,caruaru}.osm.pbf` (Recife's municipality and
Caruaru's urban area, without buildings and metadata, 2.8 and 0.55 MB,
committed); then the R article
`recife-caruaru.Rmd` is knitted from its `.Rmd.orig`. The raw downloads stay
in the cache, outside the repository.

The examples:

- R: the pkgdown-only articles `r/vignettes/articles/isochrones.Rmd`,
  `accessibility-map.Rmd` and `trips.Rmd` (excluded from the package by
  `.Rbuildignore`; they use ggplot2 and sf when rendered, and nothing is
  added to `DESCRIPTION`), and `recife-caruaru.Rmd`, which is precomputed:
  its code runs when `recife-caruaru.Rmd.orig` is knitted by `make.sh
  --pernambuco`, and pkgdown renders the result (code and output as
  Markdown, figures from `site/assets/figures`) without running it.
- Python: `python/docs/example-*.md`, run by `python/tests/test_docs.py`
  like every page (`PEABIRU_UPDATE_DOCS=1` rewrites their outputs); they
  download the Porto Alegre sample and need geopandas and matplotlib, so
  they are skipped where those are missing. `example-recife-caruaru.md`
  also needs osmium and `PEABIRU_EXAMPLE_DATA` (a directory for its 640 MB
  of downloads; the cache of `pernambuco-data.R` can be reused), declared
  in the page by `<!-- docs-test: needs osmium, env PEABIRU_EXAMPLE_DATA -->`.
- JavaScript: `site/js/src/examples.md`, with
  `site/js/examples/isochrones.mjs` (checked by `run_examples.mjs`) and the
  demo `site/demo/` (page, module, styles), which `site/build.sh` publishes
  at `_site/demo/` with a copy of the npm package (`js/lib`, `js/pkg`) and
  the São Paulo sample, and the Recife and Caruaru extracts of
  `site/demo/data/pe/` (streets only); the page fetches the Porto Alegre
  files from r5r's repository when chosen.
- Rust: `site/rust/src/isochrones.md`, with
  `crates/peabiru-core/examples/isochrone.rs`.

The pages refer to the figures at `../assets/figures/` (raw HTML `<img>` in
the MkDocs pages, so that `mkdocs build --strict` does not look for them in
`python/docs/`); `site/check_links.py` checks them in `_site/`.

## Benchmarks

Numbers are never typed into the pages. The pipeline is:

1. `python3 bench/site/run.py` runs every available engine on the same tasks
   and writes `benchmarks.json` and `.csv` into `--out` (default
   `site/data/`; see its docstring for options and environment variables).
   The engines are peabiru (CLI, R, Python), r5r and r5py, which compute the
   same matrices, and dodgr, cppRouting, networkx, igraph and pandana, which
   route the street tasks with network models of their own. Every engine
   writes its matrices, and `run.py` compares them with peabiru's (pairs
   reached by both or only one, median and 90th percentile of the absolute
   difference).
2. `python3 bench/site/charts.py [DATA]` (also run by `site/build.sh`, on
   `site/data`) draws `site/assets/charts/bench-<city>-t<threads>.svg` and
   regenerates the blocks between `<!-- BEGIN bench:… -->` and
   `<!-- END bench:… -->` in `site/index.html` (the summary) and
   `site/benchmarks.html` (summary, run metadata, charts, tables, why the
   results differ). With `--preview DIR` it writes into a copy of the pages
   in `DIR` instead, so a test run never touches the published pages.
3. The R article `r/vignettes/articles/benchmarks.Rmd` reads the CSV when it
   is rendered.

The published numbers are measured in the container of `bench/docker/`
(`bench/docker/run.sh`, see its README), not on a development machine. The
qualitative table of other tools on the benchmarks page is written by hand;
update it from each tool's documentation.
