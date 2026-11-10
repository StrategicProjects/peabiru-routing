#!/usr/bin/env bash
# Build the benchmark image and run bench/site/run.py inside it, with CPU and
# memory limits, writing the results to a directory of the host. Nothing is
# installed on the host; everything this script creates has a name:
#
#   image      peabiru-bench:latest       (docker image rm peabiru-bench)
#   container  peabiru-bench-run          (removed when it exits: --rm)
#
# It never prunes anything.
#
# Usage: bench/docker/run.sh [options] [-- run.py options]
#   --out DIR       host directory for the results (default: ./bench-results)
#   --cpus N        CPUs for the container (default 6); also pinned to CPUs
#                   0..N-1 so that every engine sees N cores
#   --memory SIZE   memory limit (default 16g)
#   --threads LIST  thread counts for run.py (default 1,N)
#   --reps N        timed runs per task (default 3)
#   --no-build      use the existing peabiru-bench image
#   --no-cache      build without Docker's layer cache
#
# Everything after `--` goes to bench/site/run.py, for example
#   bench/docker/run.sh --out /tmp/b -- --cities spo --engines peabiru-cli,dodgr
#
# The results directory gets benchmarks.json and benchmarks.csv (what
# site/data holds), versions.txt (every package version in the image), the
# matrices compared (outputs/), and site/, a preview of the benchmarks page
# built from these results. To publish them: copy benchmarks.{json,csv} to
# site/data/ and run python3 bench/site/charts.py.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
image=peabiru-bench
container=peabiru-bench-run
out="$PWD/bench-results"
cpus=6
memory=16g
threads=""
reps=3
build=1
cache=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --cpus) cpus="$2"; shift 2 ;;
    --memory) memory="$2"; shift 2 ;;
    --threads) threads="$2"; shift 2 ;;
    --reps) reps="$2"; shift 2 ;;
    --no-build) build=0; shift ;;
    --no-cache) cache=(--no-cache); shift ;;
    --) shift; break ;;
    -h|--help) sed -n '2,30p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown option $1 (see --help)" >&2; exit 2 ;;
  esac
done
case "$cpus" in ''|*[!0-9]*) echo "--cpus must be a whole number" >&2; exit 2 ;; esac
threads="${threads:-1,$cpus}"
host_cpus="$(nproc 2>/dev/null || getconf _NPROCESSORS_ONLN)"
if [ "$cpus" -gt "$host_cpus" ]; then
  echo "--cpus $cpus is more than the $host_cpus CPUs of this host" >&2
  exit 2
fi
mkdir -p "$out"
out="$(cd "$out" && pwd)"
# the commit recorded in the results: PEABIRU_COMMIT when the sources are an
# export without .git, else the checkout's
if [ -n "${PEABIRU_COMMIT:-}" ]; then
  commit="$PEABIRU_COMMIT"
else
  commit="$(git -C "$root" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  if [ -n "$(git -C "$root" status --porcelain -- crates 2>/dev/null)" ]; then
    commit="$commit+dirty"
  fi
fi

if [ "$build" = 1 ]; then
  echo "== building $image (from $root, commit $commit)"
  docker build "${cache[@]}" -f "$here/Dockerfile" -t "$image:latest" \
    --label org.opencontainers.image.revision="$commit" "$root"
fi

echo "== running $container: $cpus CPUs (0-$((cpus - 1))), $memory, threads $threads, results in $out"
# a container of the same name left behind by an interrupted run
docker rm -f "$container" >/dev/null 2>&1 || true
docker run --rm --name "$container" \
  --cpus "$cpus" --cpuset-cpus "0-$((cpus - 1))" --memory "$memory" --memory-swap "$memory" \
  -e PEABIRU_COMMIT="$commit" \
  -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
  -v "$out:/out" \
  "$image:latest" \
  bash -c '
    set -euo pipefail
    status=0
    python3 bench/site/run.py --work /tmp/work --out /out --threads "$1" --reps "$2" "${@:3}" || status=$?
    cp /opt/bench/versions.txt /out/versions.txt
    rm -rf /out/outputs && cp -r /tmp/work/outputs /out/outputs 2>/dev/null || true
    if [ -f /out/benchmarks.json ]; then
      python3 bench/site/charts.py /out --preview /out/site || status=$?
    fi
    chown -R "$HOST_UID:$HOST_GID" /out
    exit $status
  ' run "$threads" "$reps" "$@"
echo "== done: $out/benchmarks.json, preview in $out/site/benchmarks.html"
