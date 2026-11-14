# Check transit service availability by date

Counts the public transport services in the GTFS feeds of a network that
are active on given dates, and their proportion of all services. Use it
to pick a departure date with adequate service: public transport routing
only finds trips on dates when services run (per the GTFS calendars and
date exceptions).

## Usage

``` r
check_transit_availability(
  network,
  dates = NULL,
  start_date = NULL,
  end_date = NULL
)
```

## Arguments

- network:

  A network built with [`build_network()`](build_network.md).

- dates:

  A vector of specific dates to be checked. Can be character strings in
  "YYYY-MM-DD" format, or objects of class `Date`. This argument cannot
  be used with `start_date` or `end_date`. Defaults to `NULL`; when all
  date arguments are `NULL`, operating dates are discovered from the
  network.

- start_date:

  The start date for a continuous date range. Must be a single character
  string in "YYYY-MM-DD" format or a `Date` object. Must be used with
  `end_date`.

- end_date:

  The end date for a continuous date range. Must be a single character
  string in "YYYY-MM-DD" format or a `Date` object. Must be used with
  `start_date`.

## Value

A table with four columns: `date` (`Date`), `total_services`,
`active_services` and `pct_active` (the proportion of active services).
Automatic discovery returns dates in ascending order, or a zero-row
table with these columns if no retained transit trips operate.

## Details

When no date arguments are supplied, the function discovers all dates
with an active service referenced by a retained transit trip, including
frequency-based trips. This accounts for weekday calendars and date
exceptions, including exception-only services. A date with active
service does not guarantee a trip at every departure time or between
every pair of points.

You can also specify the dates to check in two ways: a vector of
specific dates in `dates`, or a continuous range with `start_date` and
`end_date`. You cannot combine these methods. In all modes,
`total_services` counts every imported service, not only those used by
trips; counts are of services, not trips. If calendar bounds are
invalid, automatic discovery fails naming the service; explicit date
queries remain available.

## See also

Other network functions: [`build_network()`](build_network.md),
[`find_snap()`](find_snap.md),
[`get_gtfs_errors()`](get_gtfs_errors.md),
[`street_network_bbox()`](street_network_bbox.md),
[`street_network_to_sf()`](street_network_to_sf.md),
[`transit_network_to_sf()`](transit_network_to_sf.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
head(check_transit_availability(network))
#> # A tibble: 6 × 4
#>   date       total_services active_services pct_active
#>   <date>              <int>           <int>      <dbl>
#> 1 2008-01-01              6               3      0.5  
#> 2 2008-01-02              6               3      0.5  
#> 3 2008-01-03              6               3      0.5  
#> 4 2008-01-04              6               3      0.5  
#> 5 2008-01-05              6               4      0.667
#> 6 2008-01-06              6               3      0.5  
check_transit_availability(network, dates = c("2019-05-13", "2019-05-19"))
#> # A tibble: 2 × 4
#>   date       total_services active_services pct_active
#>   <date>              <int>           <int>      <dbl>
#> 1 2019-05-13              6               3        0.5
#> 2 2019-05-19              6               3        0.5
```
