# Write a fare structure object to disk

Writes a fare structure to a `.zip` file holding five `.csv` files
(`global_settings.csv`, `fares_per_type.csv`, `fares_per_transfer.csv`,
`fares_per_route.csv` and `debug_settings.csv`, in the format of
[`data.table::fwrite()`](https://rdrr.io/pkg/data.table/man/fwrite.html)),
which [`read_fare_structure()`](read_fare_structure.md) reads back.

## Usage

``` r
write_fare_structure(fare_structure, file_path)
```

## Arguments

- fare_structure:

  A fare structure (see
  [`setup_fare_structure()`](setup_fare_structure.md)).

- file_path:

  A path to a `.zip` file.

## Value

The path passed to `file_path`, invisibly.

## Details

The files are deflated and carry no modification times, so the same
structure always gives the same archive, byte for byte.

## See also

Other fare structure: [`pareto_frontier()`](pareto_frontier.md),
[`read_fare_structure()`](read_fare_structure.md),
[`setup_fare_structure()`](setup_fare_structure.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
fare_structure <- setup_fare_structure(network, base_fare = 4.4)
tmpfile <- tempfile("sample_fare_structure", fileext = ".zip")
write_fare_structure(fare_structure, tmpfile)
```
