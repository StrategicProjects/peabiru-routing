#!/usr/bin/env bash
# Street-only benchmark of peabiru on r5r's sample data, mirroring bench/r5r:
# all-to-all matrices for CAR, WALK and BICYCLE, 1 and 10 threads, median of
# 3 runs, peak RSS, then parity against bench/results/r5r.
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/../.."
cargo build -q --release -p peabiru-cli
B="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/release/peabiru"
X=$(Rscript -e 'cat(system.file("extdata", package="r5r"))')
OUT=bench/results/peabiru
mkdir -p "$OUT"
echo "city,mode,n_threads,build_s,route_s,max_rss_mib" > "$OUT/timings.csv"
for c in poa spo; do
  for m in CAR WALK BICYCLE; do
    "$B" ttm "$X/$c/${c}_osm.pbf" "$X/$c/${c}_hexgrid.csv" "$m" "$OUT/${c}_$m.csv" 0 1 > /dev/null
    for t in 1 10; do
      o=$({ /usr/bin/time -l "$B" ttm "$X/$c/${c}_osm.pbf" "$X/$c/${c}_hexgrid.csv" "$m" /dev/null "$t" 3; } 2>&1)
      b=$(echo "$o" | grep -o "build [0-9.]*" | cut -d' ' -f2)
      r=$(echo "$o" | grep -o "route [0-9.]*" | cut -d' ' -f2)
      rss=$(echo "$o" | awk '/maximum resident/ {printf "%.1f", $1/1048576}')
      echo "$c,$m,$t,$b,$r,$rss" >> "$OUT/timings.csv"
    done
    echo "== parity $c $m"
    python3 -I bench/compare/parity.py "bench/results/r5r/${c}_$m.csv" "$OUT/${c}_$m.csv"
  done
done
cat "$OUT/timings.csv"
