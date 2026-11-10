# Benchmarks

Same scenarios run on peabiru, r5r 2.5.0 (R5 7.6) and r5py, reporting

- wall time, split into network build and routing;
- peak memory;
- parity: share of origin-destination pairs within 1 minute of r5r.

Scenarios: the Porto Alegre and São Paulo samples shipped with r5r, then a
metropolitan-scale grid (tens of thousands of hexagons), where the
differences actually show. Data and results are not versioned
(`bench/data/`, `bench/results/`).

The reference outputs in `bench/results/r5r` are valid for the machine that
produced them. R5 loads GTFS feeds in the order in which the operating system
lists the directory (peabiru does the same), so on a network with more than
one feed, such as Porto Alegre, r5r's expanded and arrival matrices differ in
a few rows between macOS and Linux. Run `bench/r5r/` on the platform where
the parity scripts will run.
