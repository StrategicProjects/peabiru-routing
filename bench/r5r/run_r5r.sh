#!/usr/bin/env bash
# Run the r5r street-only baseline, one R process per city, under
# /usr/bin/time -l so the peak RSS of R + the embedded JVM is recorded.
#
# Usage: bench/r5r/run_r5r.sh [city ...]   (default: poa spo)
# Env:   JAVA_HOME_21  JDK 21 home exported as JAVA_HOME (R5 7.6 rejects newer Java);
#                      defaults to Homebrew's openjdk@21
#        R5R_XMX       JVM max heap (default 4G)
# Starts from clean result files (timings*.csv, memory.csv) on every run.
set -euo pipefail
# Plain decimal points in /usr/bin/time and awk output regardless of the user locale.
export LC_NUMERIC=C

export JAVA_HOME="${JAVA_HOME_21:-/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home}"
export PATH="$JAVA_HOME/bin:$PATH"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="$root/bench/results/r5r"
mkdir -p "$out"

cities=("$@")
[ ${#cities[@]} -eq 0 ] && cities=(poa spo)

rm -f "$out/timings.csv" "$out/timings_raw.csv" "$out/memory.csv"
echo "city,max_rss_bytes,max_rss_mib,wall_seconds" > "$out/memory.csv"

for city in "${cities[@]}"; do
  log="$out/run_${city}.log"
  /usr/bin/time -l Rscript "$here/bench_r5r.R" "$city" > "$log" 2>&1
  # macOS /usr/bin/time -l reports "maximum resident set size" in bytes.
  rss=$(awk '/maximum resident set size/ {print $1}' "$log")
  wall=$(awk '/ real / {print $1; exit}' "$log")
  mib=$(awk -v b="$rss" 'BEGIN {printf "%.1f", b / 1048576}')
  echo "$city,$rss,$mib,$wall" >> "$out/memory.csv"
  echo "$city: peak RSS ${mib} MiB, wall ${wall} s (log: $log)"
done
