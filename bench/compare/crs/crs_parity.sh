#!/bin/bash
# The coordinate transforms of elevation rasters, peabiru against GeoTools as
# R5 7.6 (r5r 2.5.0's jar) calls it: CRS.findMathTransform(WGS84, EPSG:code)
# on random points (Brazil, the world, Porto Alegre's streets, the equator,
# special values), compared double for double.
#
# Usage: bash bench/compare/crs/crs_parity.sh [n points] [strict|native] [codes...]
#
#   strict  the JVM's sine, cosine, tangent and logarithm intrinsics turned
#           off (-XX:DisableIntrinsic), so Math.sin etc. are StrictMath's:
#           peabiru must be identical on every point (the default)
#   native  the JVM as r5r runs it: on arm64 and x86_64 Math.sin and
#           Math.cos differ from StrictMath in the last bit for a few per
#           cent of arguments, so the points differ by nanometres
#
# Environment: JAVA_HOME (JDK 21), R5_JAR (default: r5r's cached jar),
# PEABIRU_BIN (default target/release/peabiru), WORK (default a temp dir).
# Codes default to every supported one. Points outside a zone and the
# antimeridian are included on purpose: GeoTools' result is reproduced
# there too, whatever it is worth.
#
# Harness.java opens a raster as R5's RasterDataSourceSampler does and
# reads it at points (`run`), describes it (`info`), or makes points that
# fall within a hair of the boundaries between its pixels (`boundary`), for
# comparing with `peabiru sample-raster`.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
N=${1:-100000}
MODE=${2:-strict}
shift 2 2>/dev/null || shift $#
JAVA_HOME=${JAVA_HOME:-/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home}
JAR=${R5_JAR:-$(ls "$HOME"/Library/Caches/org.R-project.R/R/r5r/r5_jar_v7.6.0/r5-v7.6-all.jar \
  "$HOME"/.cache/R/r5r/r5_jar_v7.6.0/r5-v7.6-all.jar 2>/dev/null | head -1)}
BIN=${PEABIRU_BIN:-$here/../../../target/release/peabiru}
WORK=${WORK:-$(mktemp -d)}
CODES="$*"
if [ -z "$CODES" ]; then
  CODES="4326 4674 4618 4225 3857 5880 6210 6211 5396 $(seq -s ' ' 32601 32660) \
$(seq -s ' ' 32701 32760) $(seq -s ' ' 31965 31985) $(seq -s ' ' 29168 29172) \
$(seq -s ' ' 29187 29195) $(seq -s ' ' 22521 22525)"
fi
FLAGS=""
if [ "$MODE" = strict ]; then
  FLAGS="-XX:+UnlockDiagnosticVMOptions -XX:DisableIntrinsic=_dsin,_dcos,_dtan,_dlog,_dlog10,_dexp,_dpow"
fi
mkdir -p "$WORK/classes"
"$JAVA_HOME/bin/javac" -nowarn -cp "$JAR" -d "$WORK/classes" "$here/Steps.java" "$here/Harness.java"
python3 "$here/points.py" "$N" "$WORK/points.txt"
STEPS=0 "$JAVA_HOME/bin/java" $FLAGS -cp "$JAR:$WORK/classes" Steps "$WORK/points.txt" \
  "$WORK/geotools.txt" $CODES 2>/dev/null
fail=0
for c in $CODES; do
  "$BIN" crs-transform "$c" "$WORK/points.txt" "$WORK/peabiru-$c.txt"
  line=$(python3 "$here/compare.py" "$WORK/points.txt" "$WORK/geotools.txt" "$c" "$WORK/peabiru-$c.txt")
  echo "$line"
  case "$line" in *" differ 0 "*) ;; *) fail=1 ;; esac
done
if [ "$MODE" = strict ] && [ $fail = 1 ]; then
  echo "differences with StrictMath" >&2
  exit 1
fi
