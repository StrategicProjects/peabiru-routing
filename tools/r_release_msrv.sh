#!/usr/bin/env bash
# Build the Rust code of the R package's CRAN tarball with the minimum
# supported Rust, as the package's Makevars build it: offline, from the
# vendored crates alone, with a CARGO_HOME of its own and two jobs.
#
# The CI's msrv job checks the core and the command-line tool; this checks
# what CRAN compiles: the R bindings and the exact vendored versions of
# every crate (extendr and its dependencies among them).
#
# Usage:
#   bash tools/r_release_msrv.sh PKG_DIR WORK_DIR [TOOLCHAIN]
#
# PKG_DIR is the release copy of the package that tools/build_r_release.sh
# leaves in OUT_DIR/peabiru (with src/rust/vendor.tar.xz). WORK_DIR must not
# exist. TOOLCHAIN defaults to the rustc of SystemRequirements in PKG_DIR's
# DESCRIPTION and must be installed (rustup toolchain install 1.81).
set -euo pipefail

[ $# -ge 2 ] && [ $# -le 3 ] || {
  sed -n '/^# Usage:/,/^# DESCRIPTION/p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}
pkg=$(cd "$1" && pwd)
work=$2
if [ -e "$work" ]; then
  echo "$work already exists; give a new directory" >&2
  exit 2
fi
toolchain=${3:-$(sed -n 's/^SystemRequirements:.*rustc >= \([0-9.]*\).*/\1/p' "$pkg/DESCRIPTION")}
[ -n "$toolchain" ] || { echo "no rustc >= x.y in SystemRequirements" >&2; exit 1; }
[ -f "$pkg/src/rust/vendor.tar.xz" ] || {
  echo "$pkg/src/rust/vendor.tar.xz not found: not a release copy" >&2; exit 1; }

mkdir -p "$work/.cargo"
work=$(cd "$work" && pwd)
cp -R "$pkg/src/rust" "$work/rust"
(cd "$work" && tar xf rust/vendor.tar.xz)
cp "$work/rust/vendor-config.toml" "$work/.cargo/config.toml"

cd "$work"
cargo "+$toolchain" --version
rustc "+$toolchain" --version
# rustup, which resolves +toolchain, keeps its own home when CARGO_HOME moves
RUSTUP_HOME=$(rustup show home) CARGO_HOME="$work/.cargo" \
  cargo "+$toolchain" build -j 2 --offline --lib --release \
  --manifest-path rust/Cargo.toml --target-dir "$work/target"
echo "== the vendored sources build offline with Rust $toolchain"
