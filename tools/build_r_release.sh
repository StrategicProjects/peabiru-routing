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
#   bash tools/build_r_release.sh OUT_DIR --local-core [--allow-dirty] [--no-build-vignettes]
#
# OUT_DIR must not exist. It receives peabiru/ (the release copy of r/) and
# peabiru_<version>.tar.gz. Run R CMD check --as-cran on the tarball.
#
# --local-core is a REHEARSAL, never a release: the core is not taken from
# crates.io but packaged from the working tree (cargo package -p
# peabiru-core --no-verify) and put in the vendored crates as if it had
# been published, so the tarball has the layout of the release one (pinned
# version, lockfile entry with a checksum, everything in vendor.tar.xz, an
# offline build) before that version of the core exists on crates.io. Every
# other check of this script still runs. The result is named
# peabiru_<version>-LOCAL-CORE-NOT-FOR-CRAN.tar.gz and its DESCRIPTION has
# the field Config/peabiru/local-core; it must never be submitted: the core
# in it is not a published crate, and its lockfile checksum is not the one
# crates.io will have.
#
# Needs: git, cargo (network access to crates.io, for the core and for any
# crate not yet in the cargo cache), python3 >= 3.11, R with rextendr.
set -euo pipefail

usage() {
  sed -n '/^# Usage:/,/^# crates.io will have/p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

out=""
allow_dirty=0
local_core=0
build_args=()
for a in "$@"; do
  case "$a" in
    --allow-dirty) allow_dirty=1 ;;
    --local-core) local_core=1 ;;
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
if [ "$local_core" = 1 ]; then
  cat >&2 <<'EOF'
#######################################################################
# --local-core: REHEARSAL BUILD. The core comes from the working tree, #
# not from crates.io. The tarball is NOT the release and must NEVER be #
# submitted to CRAN.                                                   #
#######################################################################
EOF
fi

mkdir -p "$out"
out=$(cd "$out" && pwd)
pkg="$out/peabiru"
mkdir "$pkg"

# the files git tracks under r/, as they are in the working tree
(cd "$root/r" && git ls-files -z . | tar --null -T - -cf -) | tar -xf - -C "$pkg"
rust="$pkg/src/rust"

# swap the path dependency for the published version (or, with
# --local-core and only while the other crates are vendored, for the path
# of the core packaged from the working tree)
set_core() {
  python3 - "$rust/Cargo.toml" "$1" <<'EOF'
import re, sys
path, new = sys.argv[1], sys.argv[2]
text = open(path, encoding="utf-8").read()
pattern = re.compile(r"^peabiru-core = (\{ path = '[^']*' \}|'=[^']*')$", re.M)
if len(pattern.findall(text)) != 1:
    sys.exit(f"{path}: expected one line peabiru-core = {{ path = '...' }}")
text = pattern.sub(lambda m: f"peabiru-core = {new}", text)
open(path, "w", encoding="utf-8").write(text)
EOF
}

cp "$rust/Cargo.lock" "$out/Cargo.lock.dev"
if [ "$local_core" = 1 ]; then
  # the .crate cargo would upload, from the working tree: its Cargo.toml no
  # longer inherits from the workspace (a plain copy of crates/peabiru-core
  # does). --no-verify: the R package's build compiles it later.
  package_args=(--no-verify --locked)
  [ "$allow_dirty" = 0 ] || package_args+=(--allow-dirty)
  cargo package -p peabiru-core "${package_args[@]}" \
    --manifest-path "$root/Cargo.toml" --target-dir "$out/core-package"
  crate="$out/core-package/package/peabiru-core-$version.crate"
  mkdir "$out/core-local"
  tar -xzf "$crate" -C "$out/core-local"
  core_local="$out/core-local/peabiru-core-$version"
  case "$core_local" in *"'"*) echo "a path with a quote: $core_local" >&2; exit 2 ;; esac
  set_core "{ path = '$core_local' }"
  # a path dependency, as in development: the lockfile must not change
  cargo fetch --locked --manifest-path "$rust/Cargo.toml"
  cmp "$out/Cargo.lock.dev" "$rust/Cargo.lock"
  echo "== Cargo.lock: unchanged while the other crates are vendored"
else
  set_core "'=$version'"
  grep -n "^peabiru-core" "$rust/Cargo.toml"

  # update the lockfile for the new source only: cargo keeps every other
  # locked version when nothing else in the manifest changed
  cargo fetch --manifest-path "$rust/Cargo.toml"
fi
check_lock() {
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
}
[ "$local_core" = 1 ] || check_lock

# vendor every crate of the lockfile (keeps the unpacked vendor/ to check it)
Rscript -e 'rextendr::vendor_crates(commandArgs(TRUE)[1], quiet = TRUE, clean = FALSE)' "$pkg"

if [ "$local_core" = 1 ]; then
  # Put the packaged core among the vendored crates as cargo vendor would
  # have, had it come from crates.io: the directory with the checksums of
  # its files and of the .crate, the manifest pinned to the version and the
  # lockfile entry with crates.io as the source and the .crate's checksum.
  # (cargo vendor itself would fetch the *published* core of this version
  # number, if there is one, which is not the one being rehearsed.)
  [ ! -e "$rust/vendor/peabiru-core" ] || {
    echo "cargo vendor already wrote vendor/peabiru-core" >&2; exit 1; }
  cp -R "$core_local" "$rust/vendor/peabiru-core"
  set_core "'=$version'"
  grep -n "^peabiru-core" "$rust/Cargo.toml"
  python3 - "$crate" "$rust/vendor/peabiru-core" "$rust/Cargo.lock" "$version" <<'EOF'
import hashlib, json, os, sys
crate, vendored, lock, version = sys.argv[1:]
def sha(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()
files = {}
for base, _, names in os.walk(vendored):
    for name in names:
        full = os.path.join(base, name)
        files[os.path.relpath(full, vendored).replace(os.sep, "/")] = sha(full)
package = sha(crate)
with open(os.path.join(vendored, ".cargo-checksum.json"), "w", encoding="utf-8") as f:
    json.dump({"files": dict(sorted(files.items())), "package": package}, f, separators=(",", ":"))
text = open(lock, encoding="utf-8").read()
entry = f'name = "peabiru-core"\nversion = "{version}"\n'
if text.count(entry) != 1 or text.count(entry + "source") != 0:
    sys.exit("Cargo.lock: expected one peabiru-core entry without a source")
text = text.replace(entry, entry
    + 'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
    + f'checksum = "{package}"\n')
open(lock, "w", encoding="utf-8", newline="\n").write(text)
print(f"== peabiru-core {version} of the working tree vendored as a registry crate ({len(files)} files)")
EOF
  check_lock
  # the archive again, with the core in it (rextendr's own command)
  rm "$rust/vendor.tar.xz"
  (cd "$rust" && tar -cJ --no-xattrs -f vendor.tar.xz vendor)
  # From here on cargo must see only the vendored crates, as in the
  # package's Makevars: a CARGO_HOME with vendor-config.toml, offline.
  mkdir -p "$out/offline/.cargo"
  cp "$rust/vendor-config.toml" "$out/offline/.cargo/config.toml"
  ln -s "$rust/vendor" "$out/offline/vendor"
  offline_home="$out/offline/.cargo"
  (cd "$out/offline" && CARGO_HOME="$offline_home" cargo metadata --format-version 1 \
    --locked --offline --manifest-path "$rust/Cargo.toml" >/dev/null)
  echo "== the manifest and the lockfile resolve offline from the vendored crates alone"
fi
rm "$out/Cargo.lock.dev"

# the published core must be the local one: same sources, same dependencies
diff -r "$root/crates/peabiru-core/src" "$rust/vendor/peabiru-core/src" || {
  echo "the sources of peabiru-core $version on crates.io differ from crates/peabiru-core:" >&2
  echo "publish a new version of the core first" >&2
  exit 1
}
deps() {
  if [ "$local_core" = 1 ] && [ "$1" = "$rust" ]; then
    (cd "$out/offline" && CARGO_HOME="$offline_home" cargo metadata --format-version 1 \
      --locked --offline --manifest-path "$1/Cargo.toml")
  else
    cargo metadata --format-version 1 --locked --manifest-path "$1/Cargo.toml"
  fi |
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
if [ "$local_core" = 1 ]; then
  echo "== the vendored peabiru-core $version (working tree) has the local sources and dependencies"
else
  echo "== the published peabiru-core $version has the local sources and dependencies"
fi

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
if [ "$local_core" = 1 ]; then
  rm -rf "$out/offline" "$out/core-package" "$out/core-local"
  # the mark inside the tarball (a Config/ field: R CMD check accepts it)
  printf 'Config/peabiru/local-core: REHEARSAL, NOT FOR CRAN: core %s from the working tree at %s%s\n' \
    "$version" "$(git -C "$root" rev-parse --short HEAD)" \
    "$([ -z "$dirty" ] || echo ' with uncommitted changes')" >>"$pkg/DESCRIPTION"
fi
R CMD build "${build_args[@]+"${build_args[@]}"}" "$pkg"
if [ "$local_core" = 1 ]; then
  for t in "$out"/peabiru_*.tar.gz; do
    mv "$t" "${t%.tar.gz}-LOCAL-CORE-NOT-FOR-CRAN.tar.gz"
  done
fi
ls -l "$out"/peabiru_*.tar.gz
if [ "$local_core" = 1 ]; then
  cat >&2 <<'EOF'
#######################################################################
# REHEARSAL tarball (core from the working tree). NOT FOR CRAN.        #
# The release: publish the core, then run this script without          #
# --local-core.                                                        #
#######################################################################
EOF
fi
