# Parity with r5r

<img src="../assets/diagrams/parity.svg" alt="The same inputs go through r5r 2.5.0 (R5 7.6 on Java) and peabiru; outputs are compared for exact equality: matrices pair by pair, accessibility bit for bit, expanded and arrival matrices column by column and in row order, data frames with identical() in R." width="980" height="470">

peabiru's target is not "close to r5r" but **equal** to it. The scripts in
`bench/` of the repository check that:

- `bench/peabiru/run.sh`, `run_accessibility.sh` and `run_expanded.sh` run
  the command-line tool on r5r's Porto Alegre and São Paulo samples and
  compare its CSV output with r5r's (`bench/compare/parity.py`,
  `accessibility.py`, `expanded.py`), failing on any difference;
- `bench/compare/r_parity.R` calls the same functions in the R packages
  peabiru and r5r and compares the results with `identical()`;
- `bench/compare/edges.py` compares the street networks edge by edge
  (`peabiru dump-edges` against `bench/r5r/edges_r5r.R`), which is where to
  start when a new difference appears.

The reference outputs come from r5r 2.5.0 on a JDK 21 (R5 7.6 does not run on
newer Java), with `seed` set wherever the feed is frequency-based.

Equality down to the last bit depends on details that look incidental:
reading ways and feeds in R5's order, keeping the first of two equal states as
R5's priority queue does, mixing `float` and `double` as the Java code does,
reproducing R5's random number stream, and using Apache Commons Math's
`FastMath.exp` instead of the platform's `exp`.

Results can differ between machines when a network has more than one GTFS
feed. R5 loads the feeds in the order in which the operating system lists the
directory, and peabiru does the same; that order depends on the file system.
On r5r's Porto Alegre sample (two feeds), r5r on macOS and r5r on Linux give
the same travel-time matrices and accessibility, but 227 of the 605,460 rows
of the expanded matrix differ: another stop or route, and a total time up to
0.2 minutes apart. peabiru matches r5r row for row on each platform, so
reference outputs are only valid for the machine that produced them. The references in `bench/results/r5r` are
whatever r5r wrote on the machine that ran `bench/r5r/`; generate them on the
platform where the comparison runs.

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
