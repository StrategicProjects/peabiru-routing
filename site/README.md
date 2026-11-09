# Documentation sites

Everything published about peabiru is built from this repository into
`_site/` (gitignored):

| Path | Source | Tool |
|---|---|---|
| `_site/` | `site/index.html`, `site/benchmarks.html`, `site/style.css`, `site/assets/`, `site/data/` | static files |
| `_site/r/` | `r/` (`_pkgdown.yml`, `vignettes/`, `pkgdown/`) | pkgdown |
| `_site/rust/` | `site/rust/` (mdBook) and `crates/peabiru-core/examples/` | mdBook |
| `_site/rust/api/` | `crates/peabiru-core` | `cargo doc --no-deps` |
| `_site/python/` | `python/mkdocs.yml`, `python/docs/` (when they exist) | MkDocs |

## Build and preview

```sh
site/build.sh                      # everything
site/build.sh --skip-r             # faster: no pkgdown
python3 -m http.server -d _site 8000   # then open http://localhost:8000/
```

Requirements: Python 3, R with pkgdown (>= 2.2), knitr and rmarkdown, the
peabiru R package installed (set `PEABIRU_R_LIB` to a library that has it;
otherwise the script installs it from `r/` into `.claude/scratch/site-rlib`),
mdBook, and a Rust toolchain. The Python site needs `mkdocs` on the `PATH`, in
`python/.venv`, or `uvx`; without `python/mkdocs.yml` a placeholder page is
written instead.

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

For the Python docs the agreed contract is: `site/build.sh` copies
`logo-mark.svg` and `favicon.svg` to `python/docs/assets/` and `brand.css` to
`python/docs/stylesheets/brand.css` before building them.

## Benchmarks

Numbers are never typed into the pages. The pipeline is:

1. `python3 bench/site/run.py` runs every available engine (peabiru CLI,
   peabiru R, peabiru Python, r5r, r5py) on the same tasks and writes
   `site/data/benchmarks.json` and `.csv` (see its docstring for options and
   environment variables).
2. `python3 bench/site/charts.py` (also run by `site/build.sh`) draws
   `site/assets/charts/bench-<city>-t<threads>.svg` and regenerates the blocks
   between `<!-- BEGIN bench:… -->` and `<!-- END bench:… -->` in
   `site/index.html` and `site/benchmarks.html`.
3. The R article `r/vignettes/articles/benchmarks.Rmd` reads the CSV when it
   is rendered.
