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
packaged core: `cargo package -p peabiru-cli --no-verify`, then extract both
`.crate` files from `target/package/`, add to the CLI's `Cargo.toml` an empty
`[workspace]` and `[patch.crates-io] peabiru-core = { path =
"../peabiru-core-<version>" }`, and `cargo build --release` there.

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
