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

# r5r public-transport baseline

Same machine, R, r5r and Java as above (`bench/results/r5r/transit_versions.txt`),
but with transit: `mode = c("WALK", "TRANSIT")` all-to-all on the hexgrid.

## Running

```sh
bench/r5r/run_transit.sh        # both cities (poa, spo); about 2.5 minutes
bench/r5r/run_transit.sh spo    # one city
```

`run_transit.sh` works like `run_r5r.sh`: one `Rscript` per city under
`/usr/bin/time -l`, peak RSS to `transit_memory.csv`, and it resets the
`transit_*.csv` files each time. Heap is `-Xmx6G` (`R5R_XMX` overrides).
`bench_transit.R <city>` copies `<city>_osm.pbf` and the GTFS zips
(POA: `poa_eptc.zip` and `poa_trensurb.zip`; SPO: `spo.zip`) into a fresh
`bench/data/r5r/<city>-transit/`. It does **not** copy `poa_elevation.tif`,
so `elevation = "NONE"`. It then times `build_network()` there.

Parameters: `departure_datetime = 2019-05-13 14:00` (America/Sao_Paulo,
Monday), `seed = 42`, and otherwise r5r defaults written out:
`time_window = 10`, `max_walk_time = Inf`, `max_trip_duration = 120`,
`walk_speed = 3.6`, `max_rides = 3`, `draws_per_minute = 5`. The date is valid
for both feeds. POA's EPTC calendar runs 2019-04-15..2019-07-15 (weekday
services, with no `calendar_dates` exception on 05-13), and Trensurb's runs
2019-03-01..2019-12-31. SPO is frequency-based (`frequencies.txt`), and its
calendar runs 2008-01-01..2020-05-01 on every day. SPO's
`calendar.txt`/`agency.txt` hold every row twice; R5 reports 7
`DuplicateKeyError`s in `gtfs_errors.csv`, which do no harm because the
duplicates are identical.

| Setting | Call | Output |
|---|---|---|
| `TRANSIT` | `travel_time_matrix`, `percentiles = c(5,25,50,75,95)` | `<city>_TRANSIT.csv` |
| `tw1` | same, `time_window = 1` | `<city>_TRANSIT_tw1.csv` |
| `mwt15` | same, `max_walk_time = 15` | `<city>_TRANSIT_mwt15.csv` |
| `expanded` | `expanded_travel_time_matrix`, `breakdown = TRUE`, first 50 hexgrid ids (file order) x all | `<city>_TRANSIT_expanded.csv` |
| `WALK` | walk-only `travel_time_matrix` on the same transit network (sanity reference only) | `<city>_WALK_transitnet.csv` |

Each `travel_time_matrix` setting runs as follows. First a warm-up call with
10 threads. Then 3 timed calls with 10 threads, of which the median is kept.
Then 1 timed call with 1 thread. `TRANSIT` also gets one call with `seed = 43`.
The expanded matrix runs twice with 10 threads. All of these outputs are
compared pair by pair (`transit_determinism.csv`; a pair counts as changed if
any percentile differs or if it is missing from one of the two outputs).
Other files: `transit_timings.csv` / `transit_timings_raw.csv`
(`city,setting,n_threads,phase,seconds,n_pairs,n_non_na`), `transit_sanity.csv`,
and `run_transit_<city>.log`.

## Results (run of 2026-11-09)

Seconds. Route time is the median of 3 after a warm-up (1 thread: a single
run). "Reach p50" is the share of the n² pairs that have a `travel_time_p50`.
Unreachable pairs are omitted, as in the street baseline.

| City | Build (s) | Setting | 1 thread (s) | 10 threads (s) | Rows | Reach p50 | Reach walk-only |
|---|---:|---|---:|---:|---:|---:|---:|
| POA (1227 pts, scheduled) | 4.59 | TRANSIT | 23.08 | 4.20 | 1,475,387 | 97.9% | 52.4% |
| | | tw1 | 15.84 | 3.55 | 1,472,938 | 97.8% | |
| | | mwt15 | 5.45 | 1.10 | 1,233,130 | 81.8% | |
| | | expanded 50 x 1227 | | 1.69 | 605,460 | | |
| SPO (323 pts, frequency) | 2.35 | TRANSIT | 7.86 | 1.38 | 104,326 | 99.99% | 98.7% |
| | | tw1 | 2.48 | 0.47 | 104,325 | 99.99% | |
| | | mwt15 | 2.58 | 0.45 | 60,152 | 57.7% | |
| | | expanded 50 x 323 | | 1.86 | 807,400 | | |

Peak RSS of the whole process (R + JVM, `-Xmx6G`): POA 2865 MiB (wall
115 s), SPO 3210 MiB (wall 32 s).

Determinism (`transit_determinism.csv`):

- With the same seed, every setting and both cities give **identical**
  outputs: across the 3 repetitions, between 10 threads and 1 thread, and
  between the two runs of the expanded matrix (row for row, after sorting).
- With `seed = 43` instead of 42, **POA (scheduled) is unchanged**: 0 of
  1,475,387 pairs differ, because no randomness is involved.
  **SPO (frequency-based) changes 63.8%** of its pairs in at least one
  percentile (66,590 of 104,327). The p50 changes on 27.2% of the pairs
  (28,332); the mean absolute p50 difference is 0.37 min and the maximum is
  8 min. The seed only drives the random frequency offsets (Monte Carlo
  draws), so a frequency-feed comparison with peabiru has to either reproduce
  R5's RNG stream or be done in distribution.

Sanity checks (`transit_sanity.csv`):

- Percentiles are ordered p5 <= p25 <= p50 <= p75 <= p95 on 100% of the pairs
  that have all five, in every setting.
- With `max_walk_time = Inf`, transit p50 <= walk-only p50 on 100% of the pairs
  reachable both ways, and strictly faster on 92.2% (POA) and 75.0% (SPO).
  Mean over those pairs: POA 40.6 vs 75.2 min walking; SPO 44.0 vs 61.0 min.
  No pair is reachable on foot but not with transit.
- `mwt15` caps the direct walk too. Transit p50 > walk on 29,622 POA and
  14,606 SPO pairs, and in all but 210 / 59 of them the walk-only time is
  above 15 min. 87,181 / 43,049 pairs that can be walked within 120 min
  become unreachable.
- POA `tw1` gives p5 = p25 = ... = p95 on every pair: with a scheduled feed
  and a single departure minute there is a single travel time. SPO `tw1` still
  spreads (mean p5 42.5 vs p95 46.0 min) because of the 5 frequency draws.
- Pairs with a time at some percentiles but not at all of them: POA TRANSIT
  1,475,387 rows but 1,473,547 with a p50. The high percentiles cross the
  120 min cutoff on some departures and become `NA`.
- The TRANSIT diagonal is mostly 0 (unlike the street-only matrices), but it
  still reaches 49 min in POA, at the same badly snapped hexagons as before.
- Expanded matrix: POA has 1 row per pair x departure minute (10 minutes,
  `draw_number` always 1). SPO has 50 rows per pair (10 minutes x 5 draws).
  `total_time` is fractional minutes. 1,883 (POA) / 294 (SPO) rows are
  unreachable (`NA`). Taking the median of `total_time` over the rows of a
  pair (type-1 quantile) and flooring it reproduces the `TRANSIT` p50 on only
  ~94–95% of the pairs (mean abs. difference 0.5 min). It is therefore an
  oracle for *paths and per-minute times*, **not** an exact recipe for R5's
  percentile and rounding rule. That rule still has to be read in R5's
  `TravelTimeReducer` before a percentile-exact comparison.
