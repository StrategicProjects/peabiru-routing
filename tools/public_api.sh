#!/usr/bin/env bash
# The public API of peabiru-core, item by item, against the snapshot in
# tools/public-api/peabiru-core.txt, so that every change to the API (an
# item added, removed or changed, a trait implementation gained or lost,
# auto traits such as Send and Sync included) is deliberate: the CI fails
# until the snapshot is updated in the same commit.
#
#   bash tools/public_api.sh            compare (exit 1 and a diff if they differ)
#   bash tools/public_api.sh --update   rewrite the snapshot
#
# Needs cargo-public-api and the nightly toolchain it reads rustdoc's JSON
# with, at the versions below (the CI installs them; set
# PEABIRU_PUBLIC_API_TOOLCHAIN to use another nightly and CARGO_PUBLIC_API
# for a binary outside the PATH):
#   rustup toolchain install nightly-2026-11-08 --profile minimal
#   cargo install cargo-public-api --version 0.52.0 --locked
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/.." && pwd)"
snapshot="$here/public-api/peabiru-core.txt"
toolchain="${PEABIRU_PUBLIC_API_TOOLCHAIN:-nightly-2026-11-08}"
tool="${CARGO_PUBLIC_API:-cargo-public-api}"

current="$(mktemp)"
trap 'rm -f "$current"' EXIT
# -s: leave out the blanket implementations (From<T> for T, ...), which
# every type has; auto traits stay in
(cd "$root" && RUSTUP_TOOLCHAIN="$toolchain" "$tool" -p peabiru-core -s) > "$current"

if [ "${1:-}" = "--update" ]; then
  mkdir -p "$(dirname "$snapshot")"
  cp "$current" "$snapshot"
  echo "wrote $snapshot ($(wc -l < "$snapshot") items)"
  exit 0
fi
if ! diff -u "$snapshot" "$current"; then
  echo "The public API of peabiru-core differs from $snapshot (above: - snapshot, + now)." >&2
  echo "If the change is deliberate, run: bash tools/public_api.sh --update" >&2
  exit 1
fi
echo "public API of peabiru-core matches the snapshot ($(wc -l < "$snapshot") items)"
