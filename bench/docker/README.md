# Benchmark container

The numbers of the benchmarks page (`site/benchmarks.html`) are measured in
this container, so that nothing is installed on the measuring machine and
anyone can reproduce them.

```sh
bench/docker/run.sh --out /tmp/peabiru-bench            # 6 CPUs, 16 GiB
bench/docker/run.sh --out /tmp/b --cpus 4 --memory 12g -- --cities spo
```

`run.sh` builds the image `peabiru-bench` from the repository (the build
context is the repository root, filtered by `Dockerfile.dockerignore`), runs
`bench/site/run.py` in a container named `peabiru-bench-run` with
`--cpus N --cpuset-cpus 0-(N-1) --memory SIZE`, thread counts `1,N`, and
writes to the output directory:

- `benchmarks.json`, `benchmarks.csv`: the results, as in `site/data/`;
- `versions.txt`: every version in the image (R and CRAN packages, Python
  packages, Rust, Java, osmium);
- `outputs/`: the matrices that were compared with peabiru's;
- `site/`: a preview of the landing and benchmarks pages built from these
  results (`charts.py --preview`), which leaves `site/` in the repository
  untouched.

To publish a run, copy `benchmarks.json` and `benchmarks.csv` to `site/data/`
and run `python3 bench/site/charts.py`.

## What is in the image

| Part | Source | Pinned by |
|---|---|---|
| R, GDAL, PROJ, GEOS | `rocker/geospatial` | `ROCKER_TAG` |
| r5r, rJava, dodgr, cppRouting, osmdata | Posit Package Manager binaries | `CRAN_SNAPSHOT` (a date) |
| R5 7.6 jar | `r5r::download_r5()` at build time | the r5r version |
| JDK 21, osmium-tool, GNU time | Ubuntu packages | the rocker tag's Ubuntu release |
| r5py, pyrosm, osmnx, networkx, igraph, pandana | PyPI | `requirements.txt` (resolved set in `versions.txt`) |
| Rust | rustup | `RUST_VERSION` |
| peabiru CLI, R package, Python wheel | this repository | the commit (passed in as `PEABIRU_COMMIT`) |

r5py downloads its own R5 jar the first time it runs, so the container needs
network access then. Change a pin with `--build-arg`, e.g.
`docker build --build-arg CRAN_SNAPSHOT=2026-12-01 -f bench/docker/Dockerfile -t peabiru-bench .`.

## Cleaning up

The container removes itself when it exits (`--rm`). The image stays for the
next run; remove it with `docker image rm peabiru-bench`. `run.sh` never
prunes images, containers, volumes or the build cache.
