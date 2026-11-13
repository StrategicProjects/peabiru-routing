# Compared with r5r

<img src="../assets/diagrams/parity.svg" alt="The same inputs go through r5r 2.5.0 (R5 7.6 on Java) and peabiru; outputs are compared for exact equality: matrices pair by pair, accessibility bit for bit, expanded and arrival matrices column by column and in row order, data frames with identical() in R." width="980" height="470">

[r5r](https://github.com/ipea/r5r) is the R interface to Conveyal's
[R5](https://github.com/conveyal/r5), the Java routing engine whose routing
model peabiru implements in Rust. This page says what the two share, what
differs for a user of the crate, and how the results are compared. It is the
only page of this guide that discusses r5r.

## What is shared

The routing model is R5's (version 7.6, the one r5r 2.5.0 runs): how the
street network is built from OpenStreetMap, how stops are linked to it, the
searches on the streets, RAPTOR over a departure window with Monte Carlo
draws for frequency-based routes, the multi-criteria router used with fares,
the decay functions of accessibility. On the test networks the results are
checked to be **identical** to r5r's, not close to them: matrices pair by
pair, accessibility bit for bit, expanded and arrival matrices column by
column and in row order, itineraries and isochrones by their coordinates.

Other pages of this guide state as plain facts about peabiru details that
are R5's: the multi-criteria router is a port of its
`McRaptorSuboptimalPathProfileRouter`, with its tie-breaking and iteration
orders, its `FareDominatingList` and its `RuleBasedInRoutingFareCalculator`
(fares read into Java `float`s, counts into `int`s); elevations are kept in
16-bit decimetres and car speeds in 16-bit centimetres per second as R5
keeps them; rasters are read at the nearest pixel as GeoTools reads a
coverage for R5; the expanded and arrival matrices take at most 5,000
destinations, R5's limit for paths; a scenario leaves the egress and the
transfers as built because R5 copies them from the network; the street
vertices are listed in the order of a Java `HashSet`.

Equality down to the last bit depends on details that look incidental:
reading ways and feeds in R5's order, keeping the first of two equal states as
R5's priority queue does, mixing `float` and `double` as the Java code does,
reproducing R5's random number stream, and using Apache Commons Math's
`FastMath.exp` instead of the platform's `exp`.

## What differs

- No Java: the network is built in memory in about a second for a city, and
  is plain data shared between threads; nothing is written next to the data.
- The crate's functions take resolved values (points as `(lat, lon)`, dates
  as `yyyymmdd`, times in seconds, parameter structs filled from
  `Default`), where r5r's functions take data frames and date-times.
  `accessibility_trip_limits`, `isochrone_trip_limit` and
  `arrival_departure` derive the limits the R, Python and JavaScript
  packages derive from their arguments.
- `Elevation::None` is the default of `BuildOptions`; r5r applies Tobler's
  function whenever its data directory has a raster.
- [`trip_options`](trip-options.md) has no counterpart in r5r.
- R5's own fare calculators (a JSON structure with a `type`) are not ported;
  fares follow the rule-based fare structure.
- `gtfs_errors` lists the errors in a fixed order (table, then line); r5r's
  `gtfs_errors.csv` has them in an order that changes from run to run, and
  peabiru does not write that file.
- Detailed itineraries by car across a turn restriction with two or more
  via ways depend, in R5, on the searches that ran before in the session;
  peabiru always returns those of a freshly built network and warns.

### Where results depend on the machine

Results can differ between machines when a network has more than one GTFS
feed. R5 loads the feeds in the order in which the operating system lists the
directory, and `build_from_dir` does the same; that order depends on the file
system. On the Porto Alegre sample (two feeds), r5r on macOS and r5r on Linux
give the same travel-time matrices and accessibility, but 227 of the 605,460
rows of the expanded matrix differ: another stop or route, and a total time
up to 0.2 minutes apart. peabiru matches r5r row for row on each platform, so
reference outputs are only valid for the machine that produced them.
`build_from_bytes` takes the feeds in the order you give them.

Polygon isochrones depend on the platform too. The R package isoband, which
r5r calls to trace them, collects the rings by walking a C++
`std::unordered_map`, so the order of the rings and the vertex each one
starts at are those of the C++ library isoband was built with: LLVM's libc++
on macOS, GCC's libstdc++ on Linux and on Windows (Rtools). peabiru
reproduces both tables (bucket counts, rehashing and all, checked against
sequences recorded from the real containers) and uses the one of the target
it is compiled for, so its polygons equal r5r's on the same platform. Across
platforms r5r's own polygons also differ in the last bits of some latitudes,
which come from the system's `sinh` and `atan`. The WebAssembly build always
uses the libc++ order.

Elevation is the one place where peabiru is deliberately more stable. With a
raster that is not in WGS 84 longitude and latitude, a sample within
nanometres of the boundary between two pixels falls on one side or the other
in r5r depending on the machine (Java's `Math` functions may differ in the
last bit between machines). peabiru always takes the pixel Java's
`StrictMath` gives and counts such samples in the network's elevation
warnings, so its slope costs are the same on every machine and equal r5r's
wherever r5r's own result does not depend on the machine (see
[Elevation](elevation.md#warnings)).

## How the comparison is made

The comparisons live in `bench/` of the repository, outside the packages,
and run on every push to the main branch (the workflow
`.github/workflows/r5r-parity.yml`, on Linux, Windows and macOS):

- `bench/peabiru/run.sh`, `run_accessibility.sh` and `run_expanded.sh` run
  the command-line tool on the Porto Alegre and São Paulo samples and
  compare its CSV output with r5r's (`bench/compare/parity.py`,
  `accessibility.py`, `expanded.py`), failing on any difference;
- `bench/compare/r_parity.R` calls the same functions in the R packages
  peabiru and r5r and compares the results with `identical()`;
  `bench/compare/cli_parity.R` does the same for the command-line tool;
- `bench/compare/edges.py` compares the street networks edge by edge
  (`peabiru dump-edges` against `bench/r5r/edges_r5r.R`), which is where to
  start when a new difference appears.

The reference outputs come from r5r 2.5.0 on a JDK 21 (R5 7.6 does not run on
newer Java), with `seed` set wherever the feed is frequency-based. The
references in `bench/results/r5r` are whatever r5r wrote on the machine that
ran `bench/r5r/`; generate them on the platform where the comparison runs.

## Reference

Pereira, R. H. M., Saraiva, M., Herszenhut, D., Braga, C. K. V., & Conway,
M. W. (2021). r5r: Rapid Realistic Routing on Multimodal Transport Networks
with R5 in R. *Findings*, 21262. <https://doi.org/10.32866/001c.21262>
