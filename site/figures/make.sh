#!/usr/bin/env bash
# Regenerate the figures of the documentation sites (site/assets/figures/):
#
#   bash site/figures/make.sh              # every SVG figure, from figures.R
#   bash site/figures/make.sh iso_modes    # only the ones named
#   bash site/figures/make.sh --demo       # also the screenshot of the demo
#   bash site/figures/make.sh --pernambuco # only the Recife and Caruaru ones
#   bash site/figures/make.sh --capitals   # only the ones of the capitals
#
# --pernambuco runs site/figures/pernambuco.R instead of figures.R: it
# prepares the data of Recife and Caruaru (pernambuco-data.R: about 640 MB of
# downloads, cached, and osmium), draws their figures, writes the extracts of
# the browser demo (site/demo/data/pe/), and knits the precomputed R article
# r/vignettes/articles/recife-caruaru.Rmd from its .Rmd.orig. Names after it
# select figures of pernambuco.R (pe_iso_recife, pe_share, ...).
#
# --capitals runs site/figures/capitals.R: São Paulo, Rio de Janeiro and Belo
# Horizonte from their official GTFS feeds and Ipea's grid (capitals-data.R:
# about 1 GB of downloads, cached, and osmium; half an hour of routing, kept
# in the cache), and the fares of Porto Alegre; then it knits the precomputed
# R article r/vignettes/articles/capitals.Rmd from its .Rmd.orig. Names after
# it select figures of capitals.R (cap_income, cap_fare_budget, ...).
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
pe=0
capitals=0
names=()
for a in "$@"; do
  case "$a" in
    --demo) demo=1 ;;
    --pernambuco) pe=1 ;;
    --capitals) capitals=1 ;;
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

if [ "$capitals" = 1 ]; then
  Rscript "$here/capitals.R" ${names[@]+"${names[@]}"}
  if [ ${#names[@]} -eq 0 ]; then
    # the article reuses the downloads of capitals-data.R (PEABIRU_CAPITALS_CACHE)
    (cd "$root/r/vignettes/articles" &&
      PEABIRU_CAPITALS_CACHE="$(Rscript -e 'source("'"$here"'/capitals-data.R"); cat(cap_cache())')" \
      Rscript -e 'knitr::knit("capitals.Rmd.orig", "capitals.Rmd", quiet = TRUE)')
  fi
  du -ch "$root"/site/assets/figures/* | tail -1
  exit 0
fi

if [ "$pe" = 1 ]; then
  Rscript "$here/pernambuco.R" ${names[@]+"${names[@]}"}
  if [ ${#names[@]} -eq 0 ]; then
    # the article: knitted here, rendered as plain Markdown by pkgdown; it
    # reuses the downloads of pernambuco-data.R (PEABIRU_PE_CACHE)
    (cd "$root/r/vignettes/articles" &&
      PEABIRU_PE_CACHE="$(Rscript -e 'source("'"$here"'/pernambuco-data.R"); cat(pe_cache())')" \
      Rscript -e 'knitr::knit("recife-caruaru.Rmd.orig", "recife-caruaru.Rmd", quiet = TRUE)')
  fi
else
  Rscript "$here/figures.R" ${names[@]+"${names[@]}"}
fi

if [ "$demo" = 1 ]; then
  png="$root/site/assets/figures/demo-poa.png"
  python3 "$here/demo_screenshot.py" "$root/_site" "$png"
  cwebp -quiet -q 78 "$png" -o "$root/site/assets/figures/demo-poa.webp"
  rm "$png"
fi
du -ch "$root"/site/assets/figures/* | tail -1
