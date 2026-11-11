#!/usr/bin/env bash
# Build the R package's source tarball for CRAN.
#
# In the repository, r/src/rust/Cargo.toml takes peabiru-core by path from
# crates/peabiru-core, so development, the CI and devtools::test("r") always
# run against the core in the same commit. That path does not exist in a
# tarball, and CRAN builds offline. This script makes a release copy of r/
# in which:
#
#   - the core is the version published on crates.io, pinned exactly
#     (peabiru-core = '=x.y.z', x.y.z = the version of crates/peabiru-core);
#   - Cargo.lock is the development lockfile with only the core's entry
#     changed (every other crate keeps its locked version);
#   - all crates are vendored with rextendr::vendor_crates() into
#     src/rust/vendor.tar.xz and src/rust/vendor-config.toml, which the
#     Makevars use for an offline build;
#
# and then runs R CMD build on that copy. The repository is not modified.
#
# It refuses to go on when the published core differs from the local one
# (sources or dependencies), when the lockfile changes in anything else
# than the core's source, or when inst/AUTHORS does not list exactly the
# vendored crates (python3 tools/third_party_notices.py writes it).
#
# Usage:
#   bash tools/build_r_release.sh OUT_DIR [--allow-dirty] [--no-build-vignettes]
#
# OUT_DIR must not exist. It receives peabiru/ (the release copy of r/) and
# peabiru_<version>.tar.gz. Run R CMD check --as-cran on the tarball.
#
# Needs: git, cargo (network access to crates.io, for the core and for any
# crate not yet in the cargo cache), python3 >= 3.11, R with rextendr.
set -euo pipefail

usage() {
  sed -n '/^# Usage:/,/^# OUT_DIR/p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

out=""
allow_dirty=0
build_args=()
for a in "$@"; do
  case "$a" in
    --allow-dirty) allow_dirty=1 ;;
    --no-build-vignettes) build_args+=(--no-build-vignettes) ;;
    -h | --help) usage ;;
    -*) echo "unknown option: $a" >&2; usage ;;
    *) [ -z "$out" ] || usage; out="$a" ;;
  esac
done
[ -n "$out" ] || usage

root=$(cd "$(dirname "$0")/.." && pwd)
if [ -e "$out" ]; then
  echo "$out already exists; give a new directory" >&2
  exit 2
fi

dirty=$(git -C "$root" status --porcelain -- r crates/peabiru-core Cargo.toml Cargo.lock)
if [ -n "$dirty" ] && [ "$allow_dirty" = 0 ]; then
  echo "uncommitted changes in r/ or the core (use --allow-dirty to go on):" >&2
  echo "$dirty" >&2
  exit 1
fi

version=$(cargo metadata --format-version 1 --no-deps --locked \
  --manifest-path "$root/crates/peabiru-core/Cargo.toml" |
  python3 -c 'import json, sys
print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "peabiru-core"))')
echo "== peabiru-core $version"

mkdir -p "$out"
out=$(cd "$out" && pwd)
pkg="$out/peabiru"
mkdir "$pkg"

# the files git tracks under r/, as they are in the working tree
(cd "$root/r" && git ls-files -z . | tar --null -T - -cf -) | tar -xf - -C "$pkg"
rust="$pkg/src/rust"

# swap the path dependency for the published version
python3 - "$rust/Cargo.toml" "$version" <<'EOF'
import re, sys
path, version = sys.argv[1], sys.argv[2]
text = open(path, encoding="utf-8").read()
pattern = re.compile(r"^peabiru-core = \{ path = '[^']*' \}$", re.M)
if len(pattern.findall(text)) != 1:
    sys.exit(f"{path}: expected one line peabiru-core = {{ path = '...' }}")
text = pattern.sub(f"peabiru-core = '={version}'", text)
open(path, "w", encoding="utf-8").write(text)
EOF
grep -n "^peabiru-core" "$rust/Cargo.toml"

# update the lockfile for the new source only: cargo keeps every other
# locked version when nothing else in the manifest changed
cp "$rust/Cargo.lock" "$out/Cargo.lock.dev"
cargo fetch --manifest-path "$rust/Cargo.toml"
python3 - "$out/Cargo.lock.dev" "$rust/Cargo.lock" <<'EOF'
import sys, tomllib
def load(p):
    with open(p, "rb") as f:
        return {(x["name"], x["version"]): x for x in tomllib.load(f)["package"]}
dev, rel = load(sys.argv[1]), load(sys.argv[2])
if set(dev) != set(rel):
    sys.exit(f"Cargo.lock: the crates changed: only in development "
             f"{sorted(set(dev) - set(rel))}, only in the release {sorted(set(rel) - set(dev))}")
for key in dev:
    a, b = dict(dev[key]), dict(rel[key])
    if key[0] == "peabiru-core":
        if not b.get("source", "").startswith("registry+"):
            sys.exit("Cargo.lock: peabiru-core does not come from crates.io")
        a.pop("source", None); a.pop("checksum", None)
        b.pop("source", None); b.pop("checksum", None)
    if a != b:
        sys.exit(f"Cargo.lock: {key} changed: {a} -> {b}")
print("== Cargo.lock: only peabiru-core changed (now from crates.io)")
EOF
rm "$out/Cargo.lock.dev"

# vendor every crate of the lockfile (keeps the unpacked vendor/ to check it)
Rscript -e 'rextendr::vendor_crates(commandArgs(TRUE)[1], quiet = TRUE, clean = FALSE)' "$pkg"

# the published core must be the local one: same sources, same dependencies
diff -r "$root/crates/peabiru-core/src" "$rust/vendor/peabiru-core/src" || {
  echo "the sources of peabiru-core $version on crates.io differ from crates/peabiru-core:" >&2
  echo "publish a new version of the core first" >&2
  exit 1
}
deps() {
  cargo metadata --format-version 1 --locked --manifest-path "$1/Cargo.toml" |
    python3 -c 'import json, sys
m = json.load(sys.stdin)
core = [p for p in m["packages"] if p["name"] == "peabiru-core"]
assert len(core) == 1, "one peabiru-core expected"
keep = ("name", "req", "kind", "optional", "uses_default_features", "features", "target", "rename")
print(json.dumps(sorted([{k: d[k] for k in keep} for d in core[0]["dependencies"]], key=str), indent=1))
print(json.dumps(core[0]["features"], sort_keys=True))'
}
diff <(deps "$root/r/src/rust") <(deps "$rust") || {
  echo "the dependencies of peabiru-core $version on crates.io differ from crates/peabiru-core" >&2
  exit 1
}
echo "== the published peabiru-core $version has the local sources and dependencies"

# inst/AUTHORS lists exactly the vendored crates (besides the core)
python3 - "$rust/vendor" "$pkg/inst/AUTHORS" <<'EOF'
import os, re, sys, tomllib
vendor, authors = sys.argv[1], sys.argv[2]
vendored = set()
for d in os.listdir(vendor):
    with open(os.path.join(vendor, d, "Cargo.toml"), "rb") as f:
        p = tomllib.load(f)["package"]
    vendored.add((p["name"], p["version"]))
listed = set(re.findall(r"^(\S+) (\d\S*)$", open(authors, encoding="utf-8").read(), re.M))
extra = vendored - listed - {(n, v) for n, v in vendored if n.startswith("peabiru")}
if extra or listed - vendored:
    sys.exit(f"inst/AUTHORS: vendored but not listed {sorted(extra)}, "
             f"listed but not vendored {sorted(listed - vendored)}; "
             "run python3 tools/third_party_notices.py")
print(f"== inst/AUTHORS lists the {len(vendored)} vendored crates")
EOF
rm -rf "$rust/vendor"

echo "== vendor.tar.xz: $(du -k "$rust/vendor.tar.xz" | cut -f1) KiB"
cd "$out"
R CMD build "${build_args[@]+"${build_args[@]}"}" "$pkg"
ls -l "$out"/peabiru_*.tar.gz
