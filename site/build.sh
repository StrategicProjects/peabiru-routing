#!/usr/bin/env bash
# Assemble every documentation site into _site/ (gitignored):
#
#   _site/              landing page and benchmarks (site/*.html, site/assets, site/data)
#   _site/r/            pkgdown site of the R package (r/)
#   _site/rust/         mdBook guide (site/rust/)
#   _site/rust/api/     rustdoc of peabiru-core
#   _site/python/       MkDocs site of the Python package, when python/mkdocs.yml exists
#
# Usage: site/build.sh [--skip-r] [--skip-rust] [--skip-python]
# Env:   PEABIRU_R_LIB  library with the peabiru R package installed (pkgdown
#                       renders the articles with it; default: R's libraries,
#                       and if it is not there, a private library under
#                       .claude/scratch/site-rlib built from r/)
#
# Every link between the sub-sites is relative, so _site/ can be served from
# any path:  python3 -m http.server -d _site 8000
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
out="$root/_site"
skip_r=0 skip_rust=0 skip_python=0
for a in "$@"; do
  case "$a" in
    --skip-r) skip_r=1 ;;
    --skip-rust) skip_rust=1 ;;
    --skip-python) skip_python=1 ;;
    *) echo "unknown option $a" >&2; exit 2 ;;
  esac
done

step() { printf '\n== %s\n' "$*"; }

rm -rf "$out"
mkdir -p "$out"

# ---------------------------------------------------------------- landing
step "landing page and benchmarks"
if [ -f "$root/site/data/benchmarks.json" ]; then
  python3 "$root/bench/site/charts.py"     # charts + generated blocks of the pages
fi
cp "$here/index.html" "$here/benchmarks.html" "$here/style.css" "$out/"
mkdir -p "$out/assets" "$out/data"
cp -R "$here/assets/." "$out/assets/"
rm -rf "$out/assets/concepts"
[ -d "$here/data" ] && cp -R "$here/data/." "$out/data/"

# ---------------------------------------------------------------- R
if [ "$skip_r" = 0 ]; then
  step "R package site (pkgdown)"
  lib="${PEABIRU_R_LIB:-}"
  if [ -z "$lib" ] && ! Rscript -e 'quit(status = !requireNamespace("peabiru", quietly = TRUE))' >/dev/null 2>&1; then
    lib="$root/.claude/scratch/site-rlib"
    mkdir -p "$lib"
    echo "installing the R package into $lib"
    R CMD INSTALL --no-test-load --library="$lib" "$root/r" >/dev/null
  fi
  [ -n "$lib" ] && lib="$(cd "$lib" && pwd)"
  export R_LIBS="${lib:+$lib:}${R_LIBS:-}"
  (cd "$root/r" && PB_DEST="$out/r" Rscript -e '
    pkgdown::build_site(".", override = list(destination = Sys.getenv("PB_DEST")),
                        install = FALSE, new_process = FALSE, preview = FALSE)
  ')
  [ -f "$out/r/index.html" ] || { echo "pkgdown did not produce $out/r/index.html" >&2; exit 1; }
fi

# ---------------------------------------------------------------- Rust
if [ "$skip_rust" = 0 ]; then
  step "Rust guide (mdBook) and API (rustdoc)"
  cp "$here/assets/favicon.svg" "$here/rust/theme/favicon.svg"
  mdbook build "$here/rust" -d "$out/rust"
  (cd "$root" && cargo doc --no-deps -p peabiru-core -q)
  target="$(cd "$root" && cargo metadata --format-version 1 --no-deps |
    python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
  mkdir -p "$out/rust/api"
  cp -R "$target/doc/." "$out/rust/api/"
  # rustdoc's own landing: send /rust/api/ to the crate
  printf '<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=peabiru_core/index.html"><a href="peabiru_core/index.html">peabiru_core</a>\n' \
    > "$out/rust/api/index.html"
fi

# ---------------------------------------------------------------- Python
if [ "$skip_python" = 0 ]; then
  if [ -f "$root/python/mkdocs.yml" ]; then
    step "Python package site (MkDocs)"
    # The agreed contract with the Python docs: brand files at these paths.
    mkdir -p "$root/python/docs/assets" "$root/python/docs/stylesheets"
    cp "$here/assets/logo-mark.svg" "$here/assets/favicon.svg" "$root/python/docs/assets/"
    cp "$here/assets/brand.css" "$root/python/docs/stylesheets/brand.css"
    mkdocs_cmd=()
    if command -v mkdocs >/dev/null 2>&1; then
      mkdocs_cmd=(mkdocs)
    elif [ -x "$root/python/.venv/bin/mkdocs" ]; then
      mkdocs_cmd=("$root/python/.venv/bin/mkdocs")
    elif command -v uvx >/dev/null 2>&1; then
      mkdocs_cmd=(uvx --with mkdocs-material mkdocs)
    fi
    if [ ${#mkdocs_cmd[@]} -gt 0 ]; then
      (cd "$root/python" && "${mkdocs_cmd[@]}" build -q -d "$out/python")
    else
      echo "mkdocs not found: skipping the Python site" >&2
    fi
  else
    echo "python/mkdocs.yml not found: no Python site yet"
  fi
fi
if [ ! -f "$out/python/index.html" ]; then
  mkdir -p "$out/python"
  cat > "$out/python/index.html" <<'HTML'
<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>peabiru for Python</title><link rel="icon" href="../assets/favicon.svg" type="image/svg+xml">
<link rel="stylesheet" href="../assets/brand.css"><link rel="stylesheet" href="../style.css"></head>
<body><main class="wrap"><section class="hero"><h1>peabiru for Python</h1>
<p class="lead">The Python package is in development; its documentation will be published here.
Meanwhile, see the <a href="../r/">R package</a>, the <a href="../rust/">Rust guide</a> or the
<a href="../">home page</a>.</p></section></main></body></html>
HTML
fi

# ---------------------------------------------------------------- checks
step "checking local links"
python3 "$here/check_links.py" "$out"
echo
echo "built $out — preview with: python3 -m http.server -d _site 8000"
