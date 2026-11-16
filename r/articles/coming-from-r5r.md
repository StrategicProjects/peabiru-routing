# Compared with r5r

[r5r](https://github.com/ipea/r5r) (Pereira, Saraiva, Herszenhut, Braga
and Conway, 2021) is the R package that drives Conveyal’s R5 routing
engine on Java. peabiru is an independent package with an engine of its
own, written in Rust, that implements the same routing model, R5’s. This
is the one page of the documentation that compares the two: what they
share, how the results are checked against each other, every difference,
and how to move a script over. The same comparison, in the terms of each
language, is on the Python and JavaScript sites.

## What the two share

- **The routing model.** peabiru’s engine is a port of R5 7.6, the
  engine r5r 2.5.0 drives: the same street and public transport model,
  the same rules for linking points and stops to streets, the same Monte
  Carlo draws for frequency-based feeds, the same levels of traffic
  stress for cyclists, the same multi-criteria router (McRAPTOR) for
  fares and alternatives, the same rule-based fare calculator (R5’s
  `RuleBasedInRoutingFareCalculator`), the same checks of the GTFS feeds
  (those of gtfs-lib, R5’s GTFS library).
- **Function and argument names.** Of the 22 functions r5r 2.5.0
  exports, 17 exist in peabiru under the same name, with the same
  arguments in the same order and the same defaults; the other five
  manage Java and have no place here (see [Moving a script
  over](#moving-a-script-over)). The differences in the arguments are
  three: the network argument is called `network`, not `r5r_network`;
  the deprecated `r5r_core` and `sample_size` are gone;
  [`build_network()`](../reference/build_network.md) has `n_threads`. A
  check in the repository (`bench/compare/testthat/test-formals.R`)
  compares peabiru’s formals with a snapshot of r5r’s and fails on any
  other drift.
- **Argument checks** accept and reject the same values, in the same
  order, with the same bounds and conversions (a factor `mode`, a
  POSIXlt `arrival_datetime`) and messages of the same wording, except
  that peabiru’s never name r5r (see “Errors” under
  [Differences](#differences)). `bench/compare/error_parity.R` in the
  repository calls every function with invalid values of every argument
  in both packages and compares the errors.
- **Results**: the same rows, in the same order, with the same column
  names, types and values. For a frequency-based feed this needs the
  same `seed` (without one, both packages draw from the clock and every
  run differs). With `options(peabiru.output = "data.table")` the
  results also have the classes r5r gives them (see the class of the
  results under [Differences](#differences)).

Besides these, peabiru has functions of its own, which have no
counterpart in r5r: [trip_options()](trip-options.md), and
[locate_facilities() and floating_catchment()](facility-location.md).

A comparison of the two on the same inputs looks like this (not run when
this page is built; r5r needs a JDK 21 and the R5 jar):

\
[`options`](https://rdrr.io/r/base/options.html)`(``java.parameters ``=`` ``"-Xmx2G"``)`\
\
`# r5r writes its network.dat into the data directory: give it a copy`\
`r5r_dir`` ``<-`` `[`file.path`](https://rdrr.io/r/base/file.path.html)`(`[`tempdir`](https://rdrr.io/r/base/tempfile.html)`(``)``, ``"spo"``)`\
[`dir.create`](https://rdrr.io/r/base/files2.html)`(``r5r_dir``)`\
[`invisible`](https://rdrr.io/r/base/invisible.html)`(`[`file.copy`](https://rdrr.io/r/base/files.html)`(`[`list.files`](https://rdrr.io/r/base/list.files.html)`(``data_path``, pattern ``=`` ``"\\.(pbf|zip)$"``, full.names ``=`` ``TRUE``)``,`\
`                    ``r5r_dir``)``)`\
`r5r_network`` ``<-`` ``r5r``::`[`build_network`](https://ipea.github.io/r5r/reference/build_network.html)`(``r5r_dir``, verbose ``=`` ``FALSE``)`\
[`options`](https://rdrr.io/r/base/options.html)`(``peabiru.output ``=`` ``"data.table"``)`\
\
`args`` ``<-`` `[`list`](https://rdrr.io/r/base/list.html)`(``origins ``=`` ``points``, destinations ``=`` ``points``,`\
`             mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``, departure_datetime ``=`` ``departure``,`\
`             percentiles ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``25``, ``50``, ``75``)``, seed ``=`` ``42``, n_threads ``=`` ``2``,`\
`             progress ``=`` ``FALSE``)`\
`from_r5r`` ``<-`` `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``r5r``::`[`travel_time_matrix`](https://ipea.github.io/r5r/reference/travel_time_matrix.html)`, `[`c`](https://rdrr.io/r/base/c.html)`(`[`list`](https://rdrr.io/r/base/list.html)`(``r5r_network``)``, ``args``)``)`\
`from_peabiru`` ``<-`` `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``peabiru``::`[`travel_time_matrix`](../reference/travel_time_matrix.md)`, `[`c`](https://rdrr.io/r/base/c.html)`(`[`list`](https://rdrr.io/r/base/list.html)`(``network``)``, ``args``)``)`\
\
`# data.table's self-reference attribute is a pointer to the table itself`\
`no_selfref`` ``<-`` ``function``(``x``)`` ``` `attr<-` ```(``x``, ``".internal.selfref"``, ``NULL``)`\
[`identical`](https://rdrr.io/r/base/identical.html)`(``no_selfref``(``from_r5r``)``, ``no_selfref``(``from_peabiru``)``)`\
`#> [1] TRUE`\
[`options`](https://rdrr.io/r/base/options.html)`(``peabiru.output ``=`` ``NULL``)`

## How parity is checked

![Parity: the same inputs go through r5r 2.5.0 (R5 7.6 on Java) and
peabiru, and the outputs are compared for exact equality: matrices pair
by pair, accessibility bit for bit, expanded and arrival matrices column
by column and in order, and data frames with identical() in
R.](figures/parity.svg)

The [`identical()`](https://rdrr.io/r/base/identical.html) comparison
above is how parity is tested. The comparison is not part of the package
or of its tests: it lives in the repository (`bench/compare/`, run by
`tools/r5r_parity.R`), where hundreds of such comparisons on r5r’s Porto
Alegre and São Paulo samples (matrices, accessibility, expanded and
arrival matrices, isochrones, itineraries, fares, elevation, scenarios,
network exports, edge cases) run against r5r 2.5.0 on Linux, Windows and
macOS on every push to the main branch, and fail on any difference or on
any comparison that did not run.

“The same results” is meant literally: every travel time, every
accessibility value (as a double, to the last bit), every row of the
expanded and arrival matrices and of the itineraries, every vertex of
the isochrones, in the same order. This works because the engine is a
port of R5 7.6, the engine r5r drives, and follows its rules down to
details that only matter on ties: the order in which ways, feeds and
stops are read, which of two equal paths is kept, how lengths and speeds
are rounded, the random number generator behind the Monte Carlo draws (a
Mersenne Twister seeded from `seed` and the origin’s coordinates, as in
R5), and the exponential of the decay functions (Apache Commons Math’s
`FastMath.exp`, which R5 calls, instead of the C library’s). Limits are
R5’s too, such as the 5,000 destinations accepted by the expanded and
arrival matrices.

Two things depend on the platform, in r5r as in peabiru, which follows
the platform it runs on:

- **The order of the GTFS feeds.** R5 loads the feeds in the order in
  which the operating system lists the directory, and that order depends
  on the file system. On r5r’s Porto Alegre sample (two feeds), r5r on
  macOS and r5r on Linux give the same travel-time matrices and
  accessibility, but 227 of the 605,460 rows of the expanded matrix
  differ (another stop or route, a total time up to 0.2 minutes apart).
  peabiru matches r5r row for row on each platform.
- **The rings of isochrone polygons.** ‘isoband’, which r5r calls, walks
  a C++ hash table whose order decides where each ring starts and in
  which order the rings come; that order depends on the C++ library
  ‘isoband’ was built with (libc++ on macOS, libstdc++ on Linux and
  Windows). peabiru reproduces both.

## Speed and memory

The two packages give the same results on the tasks below, so what is
left to compare is what they cost. The charts show the time and the peak
memory of peabiru’s R package and of r5r 2.5.0 on the same inputs and
parameters, for the two sample cities, between all pairs of their grid
points: the median of the timed runs, with 4 threads, in a container
limited to 4 CPUs and 10 GiB on a Linux server (measured on 12 November
2026).

![Dot chart for Porto Alegre, 1,227 by 1,227 points and 4 threads, of
the time and peak memory of peabiru's R package and of r5r in seven
tasks: building the network, walking, cycling, driving and public
transport matrices, accessibility and the expanded matrix. peabiru takes
from 0.2 to 4.6 seconds and 160 to 494 MiB, r5r from 3.8 to 24 seconds
and 1.2 to 3.1 GiB.](figures/r5r-poa-t4.svg)

![The same dot chart for the Sao Paulo sample: time and peak memory of
peabiru's R package and of r5r in the same seven tasks, with 4
threads.](figures/r5r-spo-t4.svg)

Both charts are drawn from the data of the [benchmarks
page](../../benchmarks.md), which has the method, the machine, the runs
with one thread and the other packages measured.

## Differences

Everything below is deliberate. Nothing that r5r accepts is silently
ignored or computed differently: where peabiru does not give r5r’s
result, it says so, with an error or with the warning described in the
item.

### General

- **No Java.** There is no JVM to start, no `java.parameters` to set and
  no R5 jar to download, so peabiru has none of r5r’s functions for
  them: `setup_r5()`, `stop_r5()`, `download_r5()`, `r5r_cache()` and
  `r5r_sitrep()` do not exist (see [Moving a script
  over](#moving-a-script-over) for what to write instead).

&nbsp;

- **Results are tibbles by default**, not `data.table`s, and `sf`
  results are `sf` tibbles. The option `peabiru.output` chooses the
  class, at each call: `"tibble"` (the default; ‘tibble’ is the
  package’s one dependency), `"data.frame"` (base data frames, plain
  `sf`) or `"data.table"`, which gives the classes r5r returns:
  `data.table`s, `sf` `data.table`s where r5r’s `sf` is one (isochrone
  polygons, itineraries, street edges, transit routes) and plain `sf`
  elsewhere, and the index r5r’s
  [`find_snap()`](../reference/find_snap.md) leaves on `found`. It
  applies to every table, the tables of a fare structure included. The
  rows, the columns, their order and types, and the values do not depend
  on it.

  \
  [`options`](https://rdrr.io/r/base/options.html)`(``peabiru.output ``=`` ``"data.table"``)`` ``# r5r's classes`

&nbsp;

- **`sf` inputs in any coordinate reference system.** r5r stops with an
  error on a `POINT` `sf` (`origins`, `destinations`, `points`) or on
  scenario polygons or lines (`new_carspeeds`, `new_lts`) that are not
  in WGS 84. peabiru reprojects them with
  [`sf::st_transform()`](https://r-spatial.github.io/sf/reference/st_transform.html)
  and says so in a message (class `peabiru_reprojected`, one per
  argument; silence it with
  [`suppressMessages()`](https://rdrr.io/r/base/message.html)), naming
  the argument and the CRS. An `sf` without a CRS is an error that says
  how to set it, and geometries that are not points where points are
  needed are refused as r5r refuses them. Results are always in WGS 84
  (EPSG:4326).

\
`pts_utm`` ``<-`` ``sf``::`[`st_transform`](https://r-spatial.github.io/sf/reference/st_transform.html)`(``sf``::`[`st_as_sf`](https://r-spatial.github.io/sf/reference/st_as_sf.html)`(``points``, coords ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"lon"``, ``"lat"``)``, crs ``=`` ``4326``)``,`\
`                            ``31983``)`` ``# SIRGAS 2000 / UTM zone 23S`\
`ttm`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``network``, ``pts_utm``, ``pts_utm``, mode ``=`` ``"WALK"``, n_threads ``=`` ``2``)`\
`#> 'origins' reprojected from SIRGAS 2000 / UTM zone 23S (EPSG:31983) to WGS 84 (EPSG:4326).`\
`#> 'destinations' reprojected from SIRGAS 2000 / UTM zone 23S (EPSG:31983) to WGS 84 (EPSG:4326).`

- **The network lives in memory only.** Nothing is written to the data
  directory (no `network.dat`, no `gtfs_errors.csv`), and the network
  object cannot be saved with
  [`saveRDS()`](https://rdrr.io/r/base/readRDS.html): build it again in
  a new session, which is quick. `overwrite` and `temp_dir` are checked
  as r5r checks them and have no effect. A network built by
  [`r5r::build_network()`](https://ipea.github.io/r5r/reference/build_network.html)
  is not accepted: build it with
  [`peabiru::build_network()`](../reference/build_network.md). The
  network argument of every function is `network` (r5r’s is
  `r5r_network`).

&nbsp;

- **[`build_network()`](../reference/build_network.md) has
  `n_threads`**, the threads used to read the `.pbf` file.

&nbsp;

- **`verbose` and `progress` are checked but print no progress
  counter**; `verbose = TRUE` in
  [`build_network()`](../reference/build_network.md) reports the build
  in messages.

&nbsp;

- **Errors and warnings**: where the network’s argument or class is
  named, they are `network` and `peabiru_network`; r5r’s errors from
  ‘cli’ are ‘rlang’ conditions, and peabiru’s are plain errors with the
  same text. The error for exactly 200 Monte Carlo draws under a fare
  structure has class `peabiru_200_draws` (r5r’s is `r5r_200_draws`),
  and it and the error for too many draws leave out the link to r5r’s
  issue tracker; the error for the modes `CAR_PARK` and `BICYCLE_RENT`
  says they are “currently unsupported” (r5r’s adds “by r5r”). A
  scenario (`new_carspeeds`, `carspeed_scale`, `new_lts`) is routed
  without slope costs in both packages: r5r warns of it on every
  scenario, peabiru only on a network built with an elevation raster, in
  its own words.

&nbsp;

- **Deprecated arguments.** The arguments r5r 2.5.0 still accepts with a
  deprecation warning, `r5r_core` (the network’s old name, in every
  function) and `sample_size` (in
  [`isochrone()`](../reference/isochrone.md)), do not exist in peabiru:
  passing one is R’s “unused argument” error.

### Where peabiru’s result does not depend on the run or the machine

- **[`get_gtfs_errors()`](../reference/get_gtfs_errors.md) reads the
  feeds again** (r5r reads the `gtfs_errors.csv` its
  [`build_network()`](../reference/build_network.md) wrote) and lists
  the rows feed by feed in the order gtfs-lib finds them; r5r’s order is
  that of a Java `HashSet` and changes from run to run. The rows and
  columns are the same.

&nbsp;

- **Elevation in projected rasters.** In an elevation raster that is not
  in EPSG:4326, r5r reads one pixel or the other for a street point
  within nanometres (5e-8 m, or 2e-13 degrees) of a pixel boundary or of
  the raster’s edge, depending on the last bits of the JVM’s sines and
  cosines; peabiru reads the one Java’s `StrictMath` gives, on every
  machine, and warns once with the number of such points.

&nbsp;

- **Expanded and arrival matrices with a street mode alone.** r5r
  accepts them, but then reports the same trip for every minute of a
  departure time taken from the computer’s clock
  ([`Sys.time()`](https://rdrr.io/r/base/Sys.time.html)), not from
  `departure_datetime`. peabiru stops with an error; use
  [`travel_time_matrix()`](../reference/travel_time_matrix.md) for
  street modes.

Without a `seed`, frequency-based feeds give different results on every
run in both packages; with the same `seed` they give the same.

### Warnings peabiru adds

- **Voids and out-of-range elevations.** R5 keeps elevations in
  decimetres in 16-bit integers, so values beyond +/-3276.7 m wrap
  around: SRTM’s -32768 voids become 0 m, a no-data value of -9999
  becomes 3108.2 m, a `NaN` no-data value 0 m. peabiru stores them as
  r5r does, so the slope costs are r5r’s, and warns, counting them and
  the samples equal to the raster’s no-data value; r5r is silent.

&nbsp;

- **Isochrone polygons whose area the repair changes.** r5r passes the
  traced polygons to
  [`sf::st_make_valid()`](https://r-spatial.github.io/sf/reference/valid.html),
  which with spherical geometry on (sf’s default) can return an area
  that is not the traced one: of 438 isochrones of Porto Alegre and
  Helsinki, 17 came out between 0.1% and 9.5% larger or smaller (a
  30-minute car isochrone of Helsinki, 524 km² for 478). Where the s2
  repair changes the area of a polygon by more than one part in 10,000,
  peabiru repairs the polygons of that origin on the plane, which leaves
  the area as traced, and warns (class `peabiru_isochrone_repair`),
  naming them. Everywhere else the polygons are r5r’s, coordinate for
  coordinate, and `options(peabiru.isochrone_repair = "spherical")`
  keeps the s2 repair, and so gives r5r’s polygons, in every case. (In
  the [Helsinki example](helsinki.md), the 30-minute driving polygon is
  the one r5r returns at 523.8 km² and peabiru, by default, at 478.3.)

(The elevation warning above, about points on a pixel boundary, is
peabiru’s too.)

### Errors where r5r fails later or routes on wrong values

- **Values r5r lets through and then fails on.** peabiru stops with an
  error of its own that says what is wrong for: an empty `percentiles`
  (r5r fails in Java, or in R for isochrones); a missing destination id
  (a `NullPointerException` in Java); an `arrival_datetime` that is not
  a date-time (an internal R error); in
  [`isochrone()`](../reference/isochrone.md), a `polygon_output` of `NA`
  (r5r takes it as `FALSE` and fails), `origins` without rows (an
  internal R error) and a fractional `zoom` (which r5r passes to R5); in
  [`find_snap()`](../reference/find_snap.md), a `radius` of `NA` or of
  length 0 (r5r fails in Java); a `carspeed_scale` of `NA`, `NaN` or of
  a length other than 1 (an internal R error); in
  [`write_fare_structure()`](../reference/write_fare_structure.md),
  something that is not a fare structure, settings of the wrong length
  (r5r checks nothing there and fails in
  [`data.table::fwrite()`](https://rdrr.io/pkg/data.table/man/fwrite.html),
  or writes tables recycled to the wrong length) and a `NULL`
  `file_path` (r5r fails in
  [`zip::zip()`](https://r-lib.github.io/zip/reference/zip.html)); in
  [`read_fare_structure()`](../reference/read_fare_structure.md),
  several files (r5r fails in
  [`zip::unzip()`](https://r-lib.github.io/zip/reference/unzip.html)); a
  fare structure whose fare type is missing from `fares_per_type`, or
  that leaves out a route of the network (r5r fails in Java); in
  [`get_gtfs_errors()`](../reference/get_gtfs_errors.md), a folder that
  does not exist.

&nbsp;

- **Scenarios R5 would corrupt.** R5 keeps car speeds as 16-bit
  centimetres per second. A scenario whose speeds would wrap around or
  reach 0 or less (a negative factor or speed in `new_carspeeds`, a
  polygon `scale` of `Inf` or 0, a `carspeed_scale` far above 1) and a
  `carspeed_scale` beyond the 32-bit float range are errors; r5r routes
  on the wrapped speeds, hands R5 `Infinity`, or fails dividing by zero.
  Polygons that are not valid, and ties between polygons that cannot be
  settled exactly as R5 settles them, are errors too.

### What peabiru refuses

These are the inputs r5r accepts and peabiru does not; each stops with
an error that says so.

- **R5’s own fare calculators** (a `fare_structure` with a `type`). The
  fare structures of
  [`setup_fare_structure()`](../reference/setup_fare_structure.md),
  [`read_fare_structure()`](../reference/read_fare_structure.md) and
  [`write_fare_structure()`](../reference/write_fare_structure.md) are
  r5r’s, and so are the fares computed with them.

&nbsp;

- **Elevation rasters outside what is reproduced exactly**: anything but
  a one-band GeoTIFF, north up, uncompressed or compressed with LZW,
  Deflate or PackBits, in EPSG:4326 or a system whose GeoTools transform
  peabiru reproduces (SIRGAS 2000, SAD69, Córrego Alegre and their UTM
  zones used in Brazil, SIRGAS 2000 / Brazil Polyconic, WGS 84 / UTM,
  Pseudo-Mercator; the list is in
  [`?build_network`](../reference/build_network.md)), given by its EPSG
  code; a `.prj` file next to the raster; a raster named `.tiff` (r5r
  fails to read it); more than one raster in the directory (r5r reads
  the last one listed); a street point that `StrictMath` puts on a
  projected raster’s outer edge (r5r fails there too).

&nbsp;

- **More than one `.pbf` file** in the data directory.

&nbsp;

- **Ids that cannot name a file, with `output_dir`.** The files are
  named after the ids (`from_<id>.csv`), so an id holding a path
  separator (`/` or `\`) or NUL, and on Windows also `< > : " | ? *`, is
  an error that names the id, before anything is routed or written. r5r
  fails on such an id with Java’s `FileNotFoundException` (`a/b`,
  `../x`) or writes a file of another name (`from_a\b.csv` on Linux and
  macOS). [`trip_options()`](../reference/trip_options.md) follows the
  same rule;
  [`detailed_itineraries()`](../reference/detailed_itineraries.md) keeps
  r5r’s own check.

&nbsp;

- **[`pareto_frontier()`](../reference/pareto_frontier.md) with
  `output_dir`** (r5r writes the frontier to CSV files).

&nbsp;

- **Car egress through turn restrictions with two or more via ways.** In
  [`detailed_itineraries()`](../reference/detailed_itineraries.md) with
  `mode_egress = "CAR"`, R5 reverses the via edges of such restrictions
  in place during its egress search, so r5r’s itineraries for a pair
  whose car searches cross one can depend on the searches that ran
  before; peabiru returns what r5r gives on a freshly built network for
  that pair alone with one thread, and warns, naming the pairs that some
  other order would change (about 1 pair in 200 on Porto Alegre). The
  arrays are those of the network, which forward car searches read too:
  in r5r, a car matrix, accessibility or Pareto frontier computed after
  such a call could differ as well (none did on Porto Alegre: 0 of 344
  thousand cells after three calls); peabiru’s are those of a fresh
  network.

\
`fares`` ``<-`` `[`setup_fare_structure`](../reference/setup_fare_structure.md)`(``network``, base_fare ``=`` ``4.4``)`\
`fares``$``type`` ``<-`` ``"burlington"`\
[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``network``, ``points``, ``points``, mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`                   departure_datetime ``=`` ``departure``, fare_structure ``=`` ``fares``)`\
`#> ``Error``:`\
`#> ``!`` A fare structure with a 'type' (a named fare calculator instead of fare rules) is not supported by peabiru.`

### Files

- **[`write_fare_structure()`](../reference/write_fare_structure.md)**
  writes the same CSV files as r5r, deflated in the same order, but the
  archive’s bytes differ from r5r’s (which also holds the files’ dates);
  each package reads the other’s files. As in r5r 2.5.0, `debug_path`
  and `debug_info` are kept in the structure and no debug file is
  written.

## Moving a script over

A script written for r5r 2.5.0 runs on peabiru after these changes:

| In the r5r script | In peabiru |
|----|----|
| [`library(r5r)`](https://github.com/ipea/r5r), `options(java.parameters = ...)` | [`library(peabiru)`](https://strategicprojects.github.io/peabiru-routing/r/); there is no Java to configure |
| `r5r_network = net` (named) | `network = net`; a network passed by position, as the first argument, needs no change |
| `r5r_core = net` (deprecated in r5r) | `network = net` |
| `setup_r5(path)` (deprecated in r5r) | `build_network(path)` |
| `stop_r5(net)` | `rm(net)`; R’s garbage collector then frees the network ([`gc()`](https://rdrr.io/r/base/gc.html)) |
| `download_r5()`, `r5r_cache()` | nothing: peabiru downloads and caches nothing |
| `r5r_sitrep()` | `packageVersion("peabiru")`, [`core_version()`](../reference/core_version.md) and [`sessionInfo()`](https://rdrr.io/r/utils/sessionInfo.html) |
| `isochrone(..., sample_size = )` (deprecated in r5r) | drop the argument |
| code that relies on `data.table` results (`:=`, `[i, j, by]`) | `options(peabiru.output = "data.table")` once, at the top of the script |
| isochrone polygons that must equal r5r’s where the s2 repair changes an area | `options(peabiru.isochrone_repair = "spherical")` |
| `tryCatch(..., r5r_200_draws = )` | `tryCatch(..., peabiru_200_draws = )` |
| a `network.dat` kept between sessions | build the network again in each session |

Everything else (function names, the other arguments, their defaults,
the columns of the results) is the same. The inputs peabiru refuses are
listed under [What peabiru refuses](#what-peabiru-refuses), and each
stops with an error that says so.

## Checks made for the example articles

The longer examples on this site were checked against r5r 2.5.0 where a
result might be thought to be peabiru’s own doing:

- **Fares in Porto Alegre** ([scenarios
  example](scenarios-porto-alegre.md)). The multi-criteria router used
  with a fare structure and the one-criterion router used without one
  give slightly different travel times; that is R5’s behaviour: on 103
  of the origins of that example, r5r gives the same access as peabiru,
  origin by origin, with the structure and without it. The structure
  with free transfers is the slowest to route in both: on those 103
  origins it took r5r about 35 seconds against 6 to 10 with the sample
  structure, and peabiru 14 to 15 against 2 to 4, for the same results.
- **Bus speeds in Rio de Janeiro** ([capitals example](capitals.md)). On
  the feed slowed to 16 km/h, r5r gives the same access figures as
  peabiru.
- **The 70-minute walk to a direct line** (capitals example, time
  against fare). For the 14:05 departure, r5r’s
  [`travel_time_matrix()`](../reference/travel_time_matrix.md) gives 89
  minutes with `max_rides = 1` and its
  [`detailed_itineraries()`](../reference/detailed_itineraries.md) no
  trip, as peabiru’s do: R5’s matrix router and its trip planner search
  the walk to the first stop differently.
- **Boa Viagem by car** ([Recife and Caruaru](recife-caruaru.md)). The
  17 cells with no car route to the rest of the network have none in r5r
  either.
- **Frequency-based lines with fares.** With a fare structure, the
  travel times of trips on lines given by frequency fall back to walking
  in both packages, and both refuse such feeds in
  [`detailed_itineraries()`](../reference/detailed_itineraries.md).

## Reference

Pereira, R. H. M., Saraiva, M., Herszenhut, D., Braga, C. K. V. and
Conway, M. W. (2021). r5r: Rapid Realistic Routing on Multimodal
Transport Networks with R5 in R. *Findings*.
<https://doi.org/10.32866/001c.21262>
