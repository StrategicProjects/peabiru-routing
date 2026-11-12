#!/usr/bin/env bash
# Regenerate the figures of the documentation sites (site/assets/figures/):
#
#   bash site/figures/make.sh              # every SVG figure, from figures.R
#   bash site/figures/make.sh iso_modes    # only the ones named
#   bash site/figures/make.sh --demo       # also the screenshot of the demo
#   bash site/figures/make.sh --pernambuco # only the Recife and Caruaru ones
#   bash site/figures/make.sh --terrain    # only the terrain ones
#   bash site/figures/make.sh --scenarios  # only the scenario and Helsinki ones
#
# --terrain runs site/figures/terrain.R: elevation in Pernambuco (the Recife
# metropolitan region as one network, five more municipalities) and in two
# cities of hills (Salvador, Olinda). It uses the data of pernambuco-data.R
# (same cache), the census tracts of Bahia and the Copernicus DEM (GLO-90 and
# GLO-30, read from their tiles on AWS), and knits r/vignettes/articles/terrain.Rmd from its
# .Rmd.orig. Names after it select figures of terrain.R (tr_pe_shares, ...).
#
# --pernambuco runs site/figures/pernambuco.R instead of figures.R: it
# prepares the data of Recife and Caruaru (pernambuco-data.R: about 640 MB of
# downloads, cached, and osmium), draws their figures, writes the extracts of
# the browser demo (site/demo/data/pe/), and knits the precomputed R article
# r/vignettes/articles/recife-caruaru.Rmd from its .Rmd.orig. Names after it
# select figures of pernambuco.R (pe_iso_recife, pe_share, ...).
#
# --scenarios runs site/figures/scenarios.R instead: it prepares the data of
# the scenarios on Porto Alegre and of Helsinki (scenarios-data.R: about
# 880 MB of downloads, cached, and osmium), draws their figures, and knits the
# precomputed R articles scenarios-porto-alegre.Rmd and helsinki.Rmd from
# their .Rmd.orig (about ten minutes: the fare routing is slow). Names after
# it select figures of scenarios.R (scen_cycle, hel_jobs, ...).
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
terrain=0
sc=0
names=()
for a in "$@"; do
  case "$a" in
    --demo) demo=1 ;;
    --pernambuco) pe=1 ;;
    --terrain) terrain=1 ;;
    --scenarios) sc=1 ;;
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

if [ "$terrain" = 1 ]; then
  Rscript "$here/terrain.R" ${names[@]+"${names[@]}"}
  if [ ${#names[@]} -eq 0 ]; then
    # the article, knitted here like recife-caruaru.Rmd; it sources
    # pernambuco-data.R from this checkout and reuses its cache
    (cd "$root/r/vignettes/articles" &&
      PEABIRU_PE_CACHE="$(Rscript -e 'source("'"$here"'/pernambuco-data.R"); cat(pe_cache())')" \
      PEABIRU_PE_SCRIPT="$here/pernambuco-data.R" \
      Rscript -e 'knitr::knit("terrain.Rmd.orig", "terrain.Rmd", quiet = TRUE)')
  fi
  du -ch "$root"/site/assets/figures/terrain-* | tail -1
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
elif [ "$sc" = 1 ]; then
  Rscript "$here/scenarios.R" ${names[@]+"${names[@]}"}
  if [ ${#names[@]} -eq 0 ]; then
    # the articles reuse the downloads of scenarios-data.R
    (cd "$root/r/vignettes/articles" &&
      PEABIRU_SCENARIOS_CACHE="$(Rscript -e 'source("'"$here"'/scenarios-data.R"); cat(sc_cache())')" \
      Rscript -e 'for (a in c("scenarios-porto-alegre", "helsinki")) knitr::knit(paste0(a, ".Rmd.orig"), paste0(a, ".Rmd"), quiet = TRUE)')
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
