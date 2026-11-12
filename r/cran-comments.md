<!--
MAINTAINER NOTES (delete this block before submitting)

This file was drafted on 2026-11-12 from a REHEARSAL tarball
(`bash tools/build_r_release.sh <dir> --local-core`, package version 0.0.1,
core from the working tree). Every statement marked [RE-VERIFY] below must be
checked again on the real release tarball
(`bash tools/build_r_release.sh <dir>`, after the core is on crates.io) and
the mark removed. Also before submitting:

- win-builder (R-devel and R-release), R-hub or another Linux check, and
  macOS builder have NOT been run. Run them and list them under "Test
  environments"; remove the sentence that says they were not run.
- Version in DESCRIPTION, the heading of NEWS.md and the numbers below
  (sizes, times, counts of tests) are those of the rehearsal.
- The tarball submitted must not be named *-LOCAL-CORE-NOT-FOR-CRAN* and its
  DESCRIPTION must not have the field Config/peabiru/local-core.
-->

## Submission

This is a new submission.

## Test environments

* local macOS Tahoe 26.7.2 (aarch64-apple-darwin23), R 4.6.0, cargo 1.100.0,
  rustc 1.100.0: `R CMD check --as-cran` [RE-VERIFY]
* the same machine, rustc 1.81.0 (the minimum stated in
  `SystemRequirements`): the package's Rust code built offline from the
  vendored sources [RE-VERIFY]

win-builder, R-hub and Linux have not been run yet. [RE-VERIFY: run them and
list them here]

## R CMD check results

0 errors | 0 warnings | 1 note [RE-VERIFY]

* checking CRAN incoming feasibility ... NOTE

      Maintainer: 'André Leite <leite@castlab.org>'
      New submission

  This is the first submission of the package.

On the local machine the check also reports "Skipping checking HTML
validation: 'tidy' doesn't look like recent enough HTML Tidy", which is about
the machine (macOS ships an old HTML Tidy), not about the package.

The installed size is 5.4 Mb (libs 4.5 Mb, the statically linked Rust
library); the source tarball is 1.5 MB, of which 1.1 MB are the vendored
Rust sources (`src/rust/vendor.tar.xz`). [RE-VERIFY]

## Rust

The package's compiled code is written in Rust and follows CRAN's policy for
packages using Rust (<https://cran.r-project.org/web/packages/using_rust.html>):

* **No downloads.** All Rust dependencies (15 crates [RE-VERIFY], the
  package's own routing engine 'peabiru-core' among them, at the version
  published on crates.io) are vendored in `src/rust/vendor.tar.xz`. The
  build runs `cargo build --offline` against them, with `CARGO_HOME` set to
  a directory inside the build tree that is removed afterwards; nothing is
  written to `~/.cargo`.
* **Two jobs at most.** The build runs `cargo build -j 2`.
* **Versions reported.** `configure` (`tools/msrv.R`) prints the versions of
  `cargo` and `rustc` to the installation log before compiling, and stops
  with an explanatory message when `cargo` or `rustc` is missing or `rustc`
  is older than the minimum.
* **Minimum Rust version.** `SystemRequirements: Cargo (Rust's package
  manager), rustc >= 1.81`. The vendored sources were built with rustc 1.81
  as well as with the current stable. [RE-VERIFY]
* **Authorship and licences.** `inst/AUTHORS` lists every vendored crate
  with its authors, licence and repository; the authors of the crates are
  credited in `Authors@R` (role `cph`, pointing to `inst/AUTHORS`). The
  licence texts of the crates linked into the package's library are in
  `inst/LICENSE-RUST-CRATES`; the notices of the works from which parts of
  the routing engine are derived are in `inst/LICENSE-THIRD-PARTY`. The
  vendored crates are under MIT, Apache-2.0, 0BSD, Zlib or Unicode-3.0
  licences (each crate's expression is in `inst/AUTHORS`). [RE-VERIFY]
* `cleanup` removes the generated `src/Makevars`, the unpacked vendored
  sources and the cargo build directory.

## Threads

The routing functions are multi-threaded, with the number of threads set by
their argument `n_threads`. Examples, tests and the vignette use at most two
threads: they pass `n_threads = 1` or `2`, and where the argument is left at
its default (all cores) the package uses two threads when
`_R_CHECK_LIMIT_CORES_` is set.

## Examples, tests and vignette

They use only the small sample network shipped in `inst/extdata` (a few
blocks of São Paulo, 170 kB), write only under `tempdir()` and do not use the
network. Examples take under one second each and the tests about ten seconds
on the local machine. [RE-VERIFY] There are no `\dontrun{}` or `\donttest{}`
examples; the examples that return 'sf' objects run only when 'sf' is
installed.

The tests that need 'sf' or 'data.table' are skipped where those packages
are not installed.

## Reverse dependencies

None: this is a new package.
