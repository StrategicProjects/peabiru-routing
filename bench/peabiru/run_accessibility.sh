#!/usr/bin/env bash
# peabiru accessibility on r5r's sample data, one run per row of
# bench/r5r/accessibility_settings.csv (the settings bench_accessibility.R
# gave r5r), then parity against bench/results/r5r/<city>_ACCESS_<setting>.csv.
#
# Usage: bench/peabiru/run_accessibility.sh [city ...]   (default: poa spo)
# Env:   PEABIRU_ROOT  repository with bench/data and bench/results
#                      (default: this one)
#        PEABIRU_BIN   peabiru binary (default: build it with cargo)
#        PEABIRU_OUT   output directory (default: $PEABIRU_ROOT/bench/results/peabiru)
# Exits 1 if any peabiru command fails or any table differs from r5r's.
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
X=$(Rscript -e 'cat(system.file("extdata", package="r5r"))')
out="${PEABIRU_OUT:-$root/bench/results/peabiru}"
mkdir -p "$out"
cities=("$@")
[ ${#cities[@]} -eq 0 ] && cities=(poa spo)
semi() { echo "${1//;/,}"; }
status=0
for c in "${cities[@]}"; do
  while IFS=, read -r setting network mode decay value cutoffs pcts opps tw mwt mtd; do
    if [ "$network" = transit ]; then net="$root/bench/data/r5r/$c-transit"; else net="$X/$c/${c}_osm.pbf"; fi
    a=(--mode "$(semi "$mode")" --decay "$decay" --percentiles "$(semi "$pcts")"
       --opportunities "$(semi "$opps")" --tw "$tw" --max-walk "$mwt" --max-trip "$mtd"
       --date 2019-05-13 --time 14:00:00 --seed 42 --max-rides 3 --draws 5
       --walk-speed 3.6 --bike-speed 12 --max-lts 2)
    [ -n "$value" ] && a+=(--decay-value "$value")
    [ -n "$cutoffs" ] && a+=(--cutoffs "$(semi "$cutoffs")")
    f="${c}_ACCESS_${setting}.csv"
    rm -f "$out/$f"
    printf '== %s %s: ' "$c" "$setting"
    "$PEABIRU_BIN" accessibility "$net" "$X/$c/${c}_hexgrid.csv" "$out/$f" "${a[@]}" < /dev/null
    python3 -I "$repo/bench/compare/accessibility.py" "$root/bench/results/r5r/$f" "$out/$f" || status=1
  done < <(tail -n +2 "$repo/bench/r5r/accessibility_settings.csv")
done
exit $status
