# Read a fare structure object from a file

Reads a fare structure from a `.zip` file written by
[`write_fare_structure()`](write_fare_structure.md): the five `.csv`
files are read with the columns and types of the structure (see
[`setup_fare_structure()`](setup_fare_structure.md)), the way
[`data.table::fread()`](https://rdrr.io/pkg/data.table/man/fread.html)
reads them.

## Usage

``` r
read_fare_structure(file_path, encoding = "UTF-8")
```

## Arguments

- file_path:

  A path to a fare structure `.zip` file.

- encoding:

  A string, `"UTF-8"` (the default), `"unknown"` or `"Latin-1"`: how the
  text of the files is read.

## Value

A fare structure (see
[`setup_fare_structure()`](setup_fare_structure.md)).

Tables are tibbles by default; `options(peabiru.output = "data.frame")`
gives base `data.frame`s and `options(peabiru.output = "data.table")`
gives `data.table`s (see [peabiru-package](peabiru-package.md)).

## See also

Other fare structure: [`pareto_frontier()`](pareto_frontier.md),
[`setup_fare_structure()`](setup_fare_structure.md),
[`write_fare_structure()`](write_fare_structure.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
tmpfile <- tempfile("sample_fare_structure", fileext = ".zip")
write_fare_structure(setup_fare_structure(network, base_fare = 4.4), tmpfile)
fare_structure <- read_fare_structure(tmpfile)
fare_structure$transfer_time_allowance
#> [1] 120
```
