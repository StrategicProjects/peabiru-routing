#!/usr/bin/env bash
# Parity of `peabiru transit` against r5r's travel_time_matrix() with
# mode = c("WALK", "TRANSIT") (bench/r5r/bench_transit.R): all-to-all on the
# hexgrid, percentiles 5/25/50/75/95, departure 2019-05-13 14:00, seed 42,
# in three settings: default, time_window = 1 and max_walk_time = 15.
#
# Usage: bench/peabiru/run_transit.sh [city ...]   (default: poa spo)
# Env:   PEABIRU_ROOT  repository with bench/data and bench/results
#                      (default: this one)
#        PEABIRU_BIN   peabiru binary (default: build it with cargo)
#        PEABIRU_OUT   output directory (default: $PEABIRU_ROOT/bench/results/peabiru)
#        R5R_EXTDATA   r5r's extdata directory (default: ask the installed r5r)
#        PEABIRU_THREADS  --threads for peabiru (default: 0, all cores)
# Writes <city>_TRANSIT{,_tw1,_mwt15}.csv and exits 1 if any peabiru command
# fails or any matrix differs from r5r's.
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
settings=(
  "|"
  "_tw1|--tw 1"
  "_mwt15|--max-walk 15"
)
status=0
for c in "${cities[@]}"; do
  for s in "${settings[@]}"; do
    suffix=${s%%|*}
    read -r -a opts <<< "${s#*|}"
    f="$OUT/${c}_TRANSIT$suffix.csv"
    rm -f "$f"
    printf '== %s TRANSIT%s: ' "$c" "$suffix"
    "$PEABIRU_BIN" transit "$root/bench/data/r5r/$c-transit" "$X/$c/${c}_hexgrid.csv" "$f" \
      --date 2019-05-13 --time 14:00:00 --percentiles 5,25,50,75,95 --seed 42 \
      --threads "${PEABIRU_THREADS:-0}" ${opts[@]+"${opts[@]}"}
    python3 -I "$repo/bench/compare/parity.py" "$root/bench/results/r5r/${c}_TRANSIT$suffix.csv" "$f" || status=1
  done
done
exit $status
