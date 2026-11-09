# Benchmarks

Same scenarios run on peabiru, r5r 2.5.0 (R5 7.6) and r5py, reporting

- wall time, split into network build and routing;
- peak memory;
- parity: share of origin-destination pairs within 1 minute of r5r.

Scenarios: the Porto Alegre and São Paulo samples shipped with r5r, then a
metropolitan-scale grid (tens of thousands of hexagons), where the
differences actually show. Data and results are not versioned
(`bench/data/`, `bench/results/`).
