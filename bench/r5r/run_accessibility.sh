#!/usr/bin/env bash
# Run the r5r accessibility baseline (bench_accessibility.R), one R process
# per city. Files that already exist in bench/results/r5r are kept.
#
# Usage: bench/r5r/run_accessibility.sh [city ...]   (default: poa spo)
# Env:   JAVA_HOME_21  JDK 21 home (default: Homebrew's openjdk@21)
#        R5R_XMX       JVM max heap (default 3G)
#        PEABIRU_ROOT  repository with bench/data and bench/results (default: this one)
set -euo pipefail
export LC_ALL=C
export JAVA_HOME="${JAVA_HOME_21:-/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home}"
export PATH="$JAVA_HOME/bin:$PATH"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PEABIRU_ROOT="${PEABIRU_ROOT:-$(cd "$here/../.." && pwd)}"
cities=("$@")
[ ${#cities[@]} -eq 0 ] && cities=(poa spo)
for city in "${cities[@]}"; do
  Rscript "$here/bench_accessibility.R" "$city"
done
