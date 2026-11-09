# r5r street-only baseline

Baseline that peabiru's street router is compared against: r5r 2.5.0
(R5 7.6) computing all-to-all `travel_time_matrix()` on the hexgrid points of
the two sample cities shipped with r5r, for `CAR`, `WALK` and `BICYCLE`.

## Running

```sh
bench/r5r/run_r5r.sh            # both cities (poa, spo); about 4 minutes
bench/r5r/run_r5r.sh spo        # one city
```

Requirements: R with r5r 2.5.0, rJava and data.table, and a JDK 21. R5 7.6
refuses Java 22+, so the wrapper exports `JAVA_HOME` itself (default:
Homebrew's `openjdk@21`; override with `JAVA_HOME_21=/path/to/jdk21`). rJava
picks up the JVM from `JAVA_HOME` at runtime; nothing had to be reinstalled.
The JVM heap is `-Xmx4G` (override with `R5R_XMX=8G`). On first use r5r
downloads the R5 jar (65 MB) into `~/Library/Caches/org.R-project.R/R/r5r/`;
the script does this before any timed section.

What the scripts do:

- `run_r5r.sh` starts one `Rscript` process per city under `/usr/bin/time -l`
  and appends the peak RSS (R + embedded JVM) and wall time to
  `bench/results/r5r/memory.csv`. It resets the result CSVs on every run.
- `bench_r5r.R <city>` copies **only** `<city>_osm.pbf` from
  `system.file("extdata", <city>, package = "r5r")` into a fresh
  `bench/data/r5r/<city>/` (no GTFS, no elevation raster, so the network is
  street-only and `elevation = "NONE"`) and times `build_network()` there.
  It then times `travel_time_matrix()` origins = destinations = the hexgrid
  (POA 1227 points, SPO 323) for each mode with `n_threads = 1` and with
  `n_threads` = all cores: one warm-up call, then 3 timed calls, median kept.

Routing parameters (r5r defaults, written out explicitly):
`departure_datetime = 2019-05-13 14:00` (America/Sao_Paulo),
`walk_speed = 3.6`, `bike_speed = 12`, `max_lts = 2`,
`max_trip_duration = 120`, `seed = 42`, `progress = FALSE`; everything else
default (`time_window = 10`, `percentiles = 50`, no congestion / LTS
overrides).

Outputs in `bench/results/r5r/` (gitignored):

| File | Content |
|---|---|
| `<city>_<MODE>.csv` | `from_id,to_id,travel_time_p50` (minutes) from the all-cores run, as returned by r5r |
| `timings.csv` | `city,mode,n_threads,phase,seconds,n_pairs,n_non_na`; `phase` is `build` or `route`, route seconds are the median of 3 |
| `timings_raw.csv` | each of the 3 timed repetitions |
| `memory.csv` | `city,max_rss_bytes,max_rss_mib,wall_seconds` (whole process: build + all 18 routing calls) |
| `versions.txt`, `run_<city>.log` | versions, machine, and the full log |

Note that r5r **omits** unreachable pairs (more than 120 min, or not
connected) from its output rather than returning them as `NA`; `n_non_na`
counts the pairs returned with a time, out of `n_pairs` = n_points².

## Environment (run of 2026-11-09)

| | |
|---|---|
| Machine | `hw.model` Mac14,9 (MacBook Pro, Apple M2 Pro), 10 cores (6 performance + 4 efficiency), 16 GiB RAM, macOS 26.7.2 |
| R | 4.6.0 (2026-04-24); r5r 2.5.0 (built under R 4.6.1), rJava 1.0.18, data.table 1.18.4 |
| R5 | `r5-v7.6-all.jar` (R5 7.6.0) |
| Java | OpenJDK 21.0.13 (Homebrew `openjdk@21`), `-Xmx4G` |
| "All cores" | `n_threads = 10` (`parallel::detectCores()`) |

## Results

Seconds; route = median of 3 after a warm-up. Build includes loading the R5
classes into the JVM and writing `network.dat`.

| City | Points | Pairs | Build (s) | Mode | Route, 1 thread (s) | Route, 10 threads (s) | Speed-up | Pairs with a time | Share |
|---|---:|---:|---:|---|---:|---:|---:|---:|---:|
| POA | 1227 | 1,505,529 | 1.66 | CAR | 16.39 | 3.69 | 4.4x | 1,483,605 | 98.5% |
| | | | | WALK | 10.03 | 1.81 | 5.5x | 789,976 | 52.5% |
| | | | | BICYCLE | 18.80 | 3.39 | 5.5x | 1,452,296 | 96.5% |
| SPO | 323 | 104,329 | 1.11 | CAR | 1.08 | 0.20 | 5.3x | 104,329 | 100% |
| | | | | WALK | 1.75 | 0.36 | 4.9x | 103,018 | 98.7% |
| | | | | BICYCLE | 1.88 | 0.38 | 4.9x | 104,329 | 100% |

Peak RSS of the whole process (R + JVM): POA 1945 MiB (wall 224 s),
SPO 1683 MiB (wall 25 s).

Sanity checks on the matrices:

- Mean travel time on the pairs reachable in all three modes:
  POA car 10.9 < bike 36.0 < walk 75.2 min; SPO car 10.2 < bike 35.9 <
  walk 61.0 min. Car <= bike on 99.4% of the pairs, bike <= walk on 99.2–99.7%.
- Diagonal (origin = destination): median 0–2 min, but up to 49 min in POA
  (12 min in SPO, car). r5r's diagonal is not zero: it is the walk from the
  point to its snapped street and back, and a few POA hexagons are far from
  any routable street (snapping radius 1.6 km).
- POA has a 9-hexagon component disconnected from the rest of the street
  network: those 9 points reach only each other in every mode
  (1218² + 9² = 1,483,605 car pairs). WALK misses half of the POA pairs
  simply because 120 min at 3.6 km/h is only 7.2 km.
- Walk/bike time ratio on long trips is ~2.1 (POA) and ~1.7 (SPO), well
  below the speed ratio 12 / 3.6 = 3.3. The likely causes (not verified
  here) are bike routes detouring around streets above LTS 2 and the
  walking access/egress legs, which do not scale with mode speed.
