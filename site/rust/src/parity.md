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
