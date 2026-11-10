#!/bin/sh
# Build the WebAssembly module and its JavaScript glue into js/pkg/ (the
# files package.json ships next to lib/). Needs Rust with the
# wasm32-unknown-unknown target and wasm-pack (which fetches the matching
# wasm-bindgen and wasm-opt). Pass --dev for a quick, unoptimised build.
set -eu
cd "$(dirname "$0")"
profile=--release
[ "${1:-}" = --dev ] && profile=--dev
wasm-pack build "$profile" --target web --out-dir pkg --out-name peabiru --no-pack
# wasm-pack writes a .gitignore that hides every file of pkg/ from npm too
rm -f pkg/.gitignore
