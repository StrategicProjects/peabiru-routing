#!/usr/bin/env bash
# Street-only benchmark of peabiru on r5r's sample data, mirroring bench/r5r:
# all-to-all matrices for CAR, WALK and BICYCLE, 1 and 10 threads, median of
# 3 runs, peak RSS, then parity against bench/results/r5r.
#
# Usage: bench/peabiru/run.sh [city ...]   (default: poa spo)
#   poa, spo   r5r's sample networks and hexgrids
#   poa-city   the whole Porto Alegre extract (turn restrictions and
#              park-and-ride) on the 700 m grid, as bench/r5r/bench_custom.R
#              ran it: bench/data/poa-city/{poa_city.osm.pbf,points_700m.csv}
# Env:   PEABIRU_ROOT  repository with bench/data and bench/results
#                      (default: this one)
#        PEABIRU_BIN   peabiru binary (default: build it with cargo)
#        PEABIRU_OUT   output directory (default: $PEABIRU_ROOT/bench/results/peabiru)
#        R5R_EXTDATA   r5r's extdata directory (default: ask the installed r5r)
#        PEABIRU_TIMING=0  skip the timed runs (parity only)
# Exits 1 if any peabiru command fails or any matrix differs from r5r's.
set -euo pipefail
export LC_ALL=C
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
root="${PEABIRU_ROOT:-$repo}"
if [ -z "${PEABIRU_BIN:-}" ]; then
  (cd "$repo" && cargo build -q --release -p peabiru-cli)
  PEABIRU_BIN="$(cd "$repo" && cargo metadata --format-version 1 --no-deps |
    python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/release/peabiru"
fi
X="${R5R_EXTDATA:-$(Rscript -e 'cat(system.file("extdata", package="r5r"))')}"
OUT="${PEABIRU_OUT:-$root/bench/results/peabiru}"
mkdir -p "$OUT"
cities=("$@")
[ ${#cities[@]} -eq 0 ] && cities=(poa spo)
timing="${PEABIRU_TIMING:-1}"
[ "$timing" = 1 ] && echo "city,mode,n_threads,build_s,route_s,max_rss_mib" > "$OUT/timings.csv"
status=0
for c in "${cities[@]}"; do
  if [ "$c" = poa-city ]; then
    pbf="$root/bench/data/poa-city/poa_city.osm.pbf"
    pts="$root/bench/data/poa-city/points_700m.csv"
  else
    pbf="$X/$c/${c}_osm.pbf"
    pts="$X/$c/${c}_hexgrid.csv"
  fi
  for m in CAR WALK BICYCLE; do
    f="$OUT/${c}_$m.csv"
    rm -f "$f"
    "$PEABIRU_BIN" ttm "$pbf" "$pts" "$m" "$f" 0 1 > /dev/null
    if [ "$timing" = 1 ]; then
      for t in 1 10; do
        o=$({ /usr/bin/time -l "$PEABIRU_BIN" ttm "$pbf" "$pts" "$m" /dev/null "$t" 3; } 2>&1)
        b=$(echo "$o" | grep -o "build [0-9.]*" | cut -d' ' -f2)
        r=$(echo "$o" | grep -o "route [0-9.]*" | cut -d' ' -f2)
        rss=$(echo "$o" | awk '/maximum resident/ {printf "%.1f", $1/1048576}')
        echo "$c,$m,$t,$b,$r,$rss" >> "$OUT/timings.csv"
      done
    fi
    echo "== parity $c $m"
    python3 -I "$repo/bench/compare/parity.py" "$root/bench/results/r5r/${c}_$m.csv" "$f" || status=1
  done
done
[ "$timing" = 1 ] && cat "$OUT/timings.csv"
exit $status
