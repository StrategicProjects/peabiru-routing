# Releasing (for maintainers)

The order of a release, and the checks of each step. Every upload below is
irreversible: crates.io versions cannot be replaced, and the GitHub release
gives the Zenodo DOI and starts the PyPI and npm workflows.

## 1. The Rust crates: core first, then the command-line tool

`peabiru-cli` depends on `peabiru-core` by version (and by path in the
repository), so crates.io has to have that core version before the CLI can
be verified and published.

```sh
# the version of both crates is [workspace.package] version in Cargo.toml,
# and [workspace.dependencies] peabiru-core's version must be the same
cargo package -p peabiru-core                 # builds the .crate and verifies it alone
cargo publish -p peabiru-core --dry-run
cargo publish -p peabiru-core

# only once the core version is on crates.io: before that the CLI cannot be
# verified (its package resolves peabiru-core from crates.io)
cargo package -p peabiru-cli
cargo publish -p peabiru-cli --dry-run
cargo publish -p peabiru-cli
```

Before the core is published, the CLI's package can be checked against the
packaged core with `cargo package -p peabiru-core -p peabiru-cli --locked`:
cargo packages both and verifies the CLI against the packaged core. (The
CLI alone, even with `--no-verify`, fails resolving `peabiru-core =
"^X.Y.Z"` from crates.io until that version is there.)

Checks before publishing: `python3 tools/third_party_notices.py --check`
(the licences of the crates in each binary), `bash tools/public_api.sh`
(the core's public API matches `tools/public-api/peabiru-core.txt`),
`cargo +1.81 check --locked -p peabiru-core -p peabiru-cli` (the minimum
supported Rust), and the CI green on `main`.

## 2. The packages

Then, as for 0.0.1: the GitHub release (tag `vX.Y.Z`, which runs
`python-release.yml` and `npm-release.yml`, each waiting for approval in its
environment), Zenodo (from the release), and the R package for CRAN with
`bash tools/build_r_release.sh <new dir>` (it pins the core to the
published version and vendors the crates, so it needs step 1 done).

If the npm upload fails with `404 Not Found - PUT
https://registry.npmjs.org/peabiru`, look at the trusted publisher on
npmjs.com (package peabiru > Settings > Trusted Publisher): a configuration
that went unused can show "Status: Expired". Delete it and create it again
with the same four values (they are in the header of
`.github/workflows/npm-release.yml`). A release whose npm job failed is
re-run with Actions > npm-release > Run workflow, with "publish" ticked.

## 3. The R package for CRAN

Before step 1, at any time, the whole path can be rehearsed with the core of
the working tree in the place of the published one:

```sh
bash tools/build_r_release.sh <new dir> --local-core
(cd <new dir> && R CMD check --as-cran peabiru_*-LOCAL-CORE-NOT-FOR-CRAN.tar.gz)
bash tools/r_release_msrv.sh <new dir>/peabiru <another new dir>
```

That tarball is never the one to submit (its name and the field
`Config/peabiru/local-core` of its DESCRIPTION say so): the core in it is
not a published crate. The CI job `r-cran` runs the first two lines on
every push.

The release, once the core of the same version is on crates.io:

```sh
# the same version in r/DESCRIPTION, r/src/rust/Cargo.toml and the heading
# of r/NEWS.md; r/cran-comments.md without its maintainer notes
bash tools/build_r_release.sh <new dir>
(cd <new dir> && R CMD check --as-cran peabiru_<version>.tar.gz)
bash tools/r_release_msrv.sh <new dir>/peabiru <another new dir>
Rscript -e 'urlchecker::url_check("r")' -e 'spelling::spell_check_package("r")'
```

Then win-builder (R-devel and R-release) and a Linux check (R-hub) with
that tarball, the sentences marked `[RE-VERIFY]` in `r/cran-comments.md`
checked against its logs, and the upload of that same file. In the
installation log (`00install.out`) look for the versions of cargo and rustc,
`=== Using offline vendor tarball ===` and `cargo build -j 2 --offline`.
