# syntax=docker/dockerfile:1
#
# Benchmark image for the documentation site: peabiru (CLI, R, Python), r5r,
# r5py, dodgr, cppRouting, networkx, igraph and pandana, with everything they
# need, so that bench/site/run.py runs without installing anything on the
# host. Build it from the root of the repository (bench/docker/run.sh does):
#
#   docker build -f bench/docker/Dockerfile -t peabiru-bench .
#
# The repository is copied in (see Dockerfile.dockerignore) and peabiru's CLI,
# R package and Python wheel are built from that copy. Versions are pinned:
# R and the system libraries by the rocker tag, CRAN packages by a dated Posit
# Package Manager snapshot, Python packages by bench/docker/requirements.txt,
# Rust by RUST_VERSION; the resolved versions are recorded in the image
# (/opt/bench/versions.txt) and, by run.py, in benchmarks.json.

ARG ROCKER_TAG=4.6.1
FROM rocker/geospatial:${ROCKER_TAG}

ARG CRAN_SNAPSHOT=2026-11-02
ARG RUST_VERSION=1.99.1
ENV CRAN_SNAPSHOT=${CRAN_SNAPSHOT} \
    DEBIAN_FRONTEND=noninteractive \
    LANG=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8

# ---------------------------------------------------------------- system
# JDK 21 for R5 7.6 (r5r) and r5py; osmium-tool converts the .pbf for dodgr;
# GNU time measures peak memory (run.py calls /usr/bin/time -v);
# python3-venv for the Python environment.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      openjdk-21-jdk-headless osmium-tool time python3-venv python3-dev \
      ca-certificates curl git build-essential pkg-config \
 && rm -rf /var/lib/apt/lists/*
RUN JAVA_HOME="/usr/lib/jvm/java-21-openjdk-$(dpkg --print-architecture)" \
 && test -x "$JAVA_HOME/bin/javac" \
 && ln -s "$JAVA_HOME" /opt/jdk21 \
 && R CMD javareconf JAVA_HOME="$JAVA_HOME"
ENV JAVA_HOME=/opt/jdk21 \
    JAVA_HOME_21=/opt/jdk21

# ---------------------------------------------------------------- R packages
# Binary packages for the image's Ubuntu release from a dated snapshot.
RUN . /etc/os-release \
 && echo "options(repos = c(CRAN = 'https://packagemanager.posit.co/cran/__linux__/${VERSION_CODENAME}/${CRAN_SNAPSHOT}'))" \
      >> "$(R RHOME)/etc/Rprofile.site" \
 && Rscript -e 'install.packages(c("rJava", "r5r", "data.table", "jsonlite", "dodgr", "cppRouting", "osmdata", "RcppParallel", "sf"))' \
 && Rscript -e 'for (p in c("rJava", "r5r", "dodgr", "cppRouting", "osmdata")) if (!requireNamespace(p, quietly = TRUE)) stop("R package ", p, " did not install")'
# R5's jar, downloaded once into the image (r5r keeps it in its user cache)
RUN Rscript -e 'options(java.parameters = "-Xmx1G"); library(r5r); cat(r5r::download_r5(quiet = TRUE), "\n")'

# ---------------------------------------------------------------- Rust
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain ${RUST_VERSION} \
 && rustc --version

# ---------------------------------------------------------------- Python
COPY bench/docker/requirements.txt /opt/bench/requirements.txt
RUN python3 -m venv /opt/bench/venv \
 && /opt/bench/venv/bin/pip install --no-cache-dir --upgrade pip \
 && /opt/bench/venv/bin/pip install --no-cache-dir -r /opt/bench/requirements.txt
ENV PATH=/opt/bench/venv/bin:$PATH

# ---------------------------------------------------------------- peabiru
# The repository, built from source: CLI, R package (into R's site library)
# and the Python wheel (into the venv).
COPY . /src
WORKDIR /src
ENV CARGO_TARGET_DIR=/opt/peabiru-target
RUN cargo build --release --locked -p peabiru-cli \
 && install -m 0755 /opt/peabiru-target/release/peabiru /usr/local/bin/peabiru \
 && peabiru --version
RUN R CMD INSTALL r \
 && Rscript -e 'cat("peabiru", as.character(packageVersion("peabiru")), peabiru::core_version(), "\n")'
RUN maturin build --release -m python/Cargo.toml -o /opt/bench/wheels \
 && pip install --no-cache-dir /opt/bench/wheels/peabiru-*.whl \
 && python -c "import peabiru; print('peabiru', peabiru.__version__)"
# what run.py does not get from the source tree itself: the commit is passed
# in by run.sh (PEABIRU_COMMIT), the sources come without .git
RUN rm -rf /opt/peabiru-target/release/build /opt/peabiru-target/release/deps \
           /opt/peabiru-target/release/incremental

# ---------------------------------------------------------------- versions
RUN pip freeze > /opt/bench/pip-freeze.txt \
 && { echo "rocker/geospatial:${ROCKER_TAG}"; echo "CRAN snapshot ${CRAN_SNAPSHOT}"; \
      rustc --version; java -version 2>&1 | head -1; python --version; osmium --version | head -1; \
      Rscript -e 'for (p in c("r5r", "dodgr", "cppRouting", "osmdata", "rJava", "peabiru")) cat(p, as.character(packageVersion(p)), "\n")'; \
      cat /opt/bench/pip-freeze.txt; } > /opt/bench/versions.txt \
 && cat /opt/bench/versions.txt

ENV PEABIRU_BIN=/usr/local/bin/peabiru \
    PEABIRU_PYTHON=/opt/bench/venv/bin/python \
    R5PY_PYTHON=/opt/bench/venv/bin/python \
    BENCH_PYTHON=/opt/bench/venv/bin/python \
    BENCH_REQUIREMENTS=/opt/bench/pip-freeze.txt \
    R5R_XMX=6G \
    R5PY_MAX_MEMORY=6G \
    PEABIRU_BENCH_CONTAINER=peabiru-bench

CMD ["python3", "bench/site/run.py", "--help"]
