#!/usr/bin/env bash
# Regenerate the figures of the documentation sites (site/assets/figures/):
#
#   bash site/figures/make.sh              # every SVG figure, from figures.R
#   bash site/figures/make.sh iso_modes    # only the ones named
#   bash site/figures/make.sh --demo       # also the screenshot of the demo
#
# The SVG figures are computed with the peabiru R package on the sample data
# of Porto Alegre (r5r 2.5.0's, downloaded once into a cache directory) and
# São Paulo (the package's own), and drawn with ggplot2, sf and svglite.
# The R package is taken from PEABIRU_R_LIB, R's libraries, or else
# installed from r/ into .claude/scratch/site-rlib (as site/build.sh does).
#
# --demo writes demo-poa.webp, a screenshot of site/demo with the Porto Alegre
# network; it needs the npm package built (sh js/build.sh) and the site built
# (site/build.sh), Python with playwright (pip install playwright; playwright
# install chromium) and cwebp.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"

demo=0
names=()
for a in "$@"; do
  case "$a" in
    --demo) demo=1 ;;
    *) names+=("$a") ;;
  esac
done

lib="${PEABIRU_R_LIB:-}"
if [ -z "$lib" ] && ! Rscript -e 'quit(status = !requireNamespace("peabiru", quietly = TRUE))' >/dev/null 2>&1; then
  lib="$root/.claude/scratch/site-rlib"
  mkdir -p "$lib"
  echo "installing the R package into $lib"
  R CMD INSTALL --no-test-load --library="$lib" "$root/r" >/dev/null
fi
if [ -n "$lib" ]; then
  R_LIBS="$(cd "$lib" && pwd)${R_LIBS:+:$R_LIBS}"
  export R_LIBS
fi

Rscript "$here/figures.R" ${names[@]+"${names[@]}"}

if [ "$demo" = 1 ]; then
  png="$root/site/assets/figures/demo-poa.png"
  python3 "$here/demo_screenshot.py" "$root/_site" "$png"
  cwebp -quiet -q 78 "$png" -o "$root/site/assets/figures/demo-poa.webp"
  rm "$png"
fi
du -ch "$root"/site/assets/figures/* | tail -1
