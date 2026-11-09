#!/usr/bin/env bash
# Parity of `peabiru expanded` against r5r's expanded_travel_time_matrix()
# and arrival_travel_time_matrix() baselines (bench/r5r/bench_transit.R and
# bench/r5r/bench_expanded.R), same parameters setting by setting.
#
# Usage: bench/peabiru/run_expanded.sh [city ...]   (default: poa spo)
# Env:   PEABIRU_BIN  binary to use (default: build with cargo and use the
#                     workspace target directory)
#        PEABIRU_ROOT checkout whose bench/data and bench/results to use
#                     (default: this one)
#        PEABIRU_OUT  output directory (default: $PEABIRU_ROOT/bench/results/peabiru)
#        R5R_EXTDATA  r5r's extdata directory (default: ask the installed r5r)
# Writes <out>/<city>_TRANSIT_{expanded,expanded_nobd,expanded_v2,arrival,
# arrival_v2,arrival_midnight}.csv and exits 1 if a peabiru command fails or
# any matrix differs.
set -euo pipefail
export LC_ALL=C
HERE="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$HERE"
if [ -z "${PEABIRU_BIN:-}" ]; then
  cargo build -q --release -p peabiru-cli
  PEABIRU_BIN="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/release/peabiru"
fi
ROOT="${PEABIRU_ROOT:-$HERE}"
X="${R5R_EXTDATA:-$(Rscript -e 'cat(system.file("extdata", package="r5r"))')}"
OUT="${PEABIRU_OUT:-$ROOT/bench/results/peabiru}"
REF="$ROOT/bench/results/r5r"
mkdir -p "$OUT"
cities=("$@")
[ ${#cities[@]} -eq 0 ] && cities=(poa spo)

settings=(
  "expanded|--date 2019-05-13 --time 14:00:00 --seed 42 --origins 50 --breakdown"
  "expanded_nobd|--date 2019-05-13 --time 14:00:00 --seed 42 --origins 50"
  "expanded_v2|--date 2019-05-13 --time 07:30:30 --tw 5 --draws 3 --max-trip 60 --max-walk 15 --max-rides 2 --seed 7 --origins 50 --breakdown"
  "arrival|--arrival --date 2019-05-13 --time 16:00:00 --seed 42 --origins 200 --breakdown"
  "arrival_v2|--arrival --date 2019-05-13 --time 08:00:00 --max-trip 60 --max-walk 15 --max-rides 2 --seed 7 --origins 200"
  "arrival_midnight|--arrival --date 2019-05-14 --time 00:30:00 --seed 42 --origins 50 --breakdown"
)
status=0
for c in "${cities[@]}"; do
  for s in "${settings[@]}"; do
    name=${s%%|*}
    read -r -a opts <<< "${s#*|}"
    out="$OUT/${c}_TRANSIT_${name}.csv"
    rm -f "$out"
    "$PEABIRU_BIN" expanded "$ROOT/bench/data/r5r/$c-transit" "$X/$c/${c}_hexgrid.csv" "$out" "${opts[@]}"
    echo "== parity $c $name"
    python3 -I "$HERE/bench/compare/expanded.py" "$REF/${c}_TRANSIT_${name}.csv" "$out" || status=1
  done
done
exit $status
