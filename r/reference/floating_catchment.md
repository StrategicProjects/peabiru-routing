# Supply against demand around each place: the two-step floating catchment area

Step 1: the supply of each facility (places, staff, beds) is divided by
the demand within its catchment. Step 2: each demand point adds up the
ratios of the facilities within its catchment. The result, per demand
point, is supply per unit of demand, which accounts both for how near
the facilities are and for how many others compete for them (Luo and
Wang, 2003).

## Usage

``` r
floating_catchment(
  network,
  demand,
  supply,
  cutoff = NULL,
  decay_function = "step",
  decay_value = NULL,
  demand_colname = NULL,
  supply_colname = NULL,
  mode = "WALK",
  mode_egress = "WALK",
  departure_datetime = NULL,
  percentile = 50L,
  max_trip_duration = NULL,
  travel_times = NULL,
  ...
)
```

## Arguments

- network:

  The network of [`build_network()`](build_network.md); may be `NULL`
  with `travel_times`.

- demand:

  The demand points: a `data.frame` with `id`, `lon` and `lat`, or a
  `POINT sf` with an `id` column, and, with `demand_colname`, their
  weights.

- supply:

  The facilities: a `data.frame` or `POINT sf` with `id`, `lon`, `lat`
  and, with `supply_colname`, their capacities.

- cutoff:

  The catchment in minutes; `NULL` with `"fixed_exponential"`.

- decay_function:

  A string: `"step"`, `"exponential"`, `"fixed_exponential"`, `"linear"`
  or `"logistic"` (see [`accessibility()`](accessibility.md)).

- decay_value:

  The parameter of `"fixed_exponential"`, `"linear"` and `"logistic"`,
  as in [`accessibility()`](accessibility.md); `NULL` for the others.

- demand_colname, supply_colname:

  The names of the numeric columns with the weight of each demand point
  and the capacity of each facility; `NULL` for 1 each.

- mode:

  A character vector, the transport modes (see the section "Transport
  modes"). Defaults to `"WALK"`.

- mode_egress:

  A string, the mode from the last stop to the destination: `"WALK"`
  (the default), `"BICYCLE"` or `"CAR"`. Ignored without public
  transport.

- departure_datetime:

  A POSIXct, the departure date and time (see the section "Date and
  time"). Required with public transport.

- percentile:

  The travel-time percentile used (public transport over a time window),
  a whole number from 1 to 99.

- max_trip_duration:

  The longest trip considered, in minutes. `NULL`: `cutoff` (rounded up)
  for `"step"`, 120 for the other functions.

- travel_times:

  A table of travel times to use instead of routing: `from_id` (demand
  point), `to_id` (site) and `travel_time_pXX` for the `percentile` (or
  `travel_time`), in minutes, as
  [`travel_time_matrix()`](travel_time_matrix.md) returns it. A pair
  that is absent or has a missing time is unreachable; rows of other
  points are ignored. With it, `network` and the routing arguments are
  not used and the points need only their `id`.

- ...:

  Any other argument of [`travel_time_matrix()`](travel_time_matrix.md),
  by its full name: `time_window`, `walk_speed`, `max_walk_time`, `seed`
  and so on (not `percentiles` or `output_dir`).

## Value

A list of two tables:

- `demand`: one row per demand point: `id` (with `lon` and `lat` when
  given), `demand` (its weight) and `accessibility` (supply per unit of
  demand).

- `supply`: one row per facility: `id`, `supply`, `catchment_demand`
  (the weighted demand in its catchment) and `ratio` (supply divided by
  that demand; 0 when the catchment holds no demand).

  Tables are tibbles by default;
  `options(peabiru.output = "data.frame")` gives base `data.frame`s and
  `options(peabiru.output = "data.table")` gives `data.table`s (see
  [peabiru-package](peabiru-package.md)).

## Details

With `decay_function = "step"` a pair of demand point and facility
counts in full up to `cutoff` minutes (travel time `<= cutoff`) and not
at all beyond: the two-step floating catchment area in its original
form. Any other decay function gives the enhanced form, in which nearer
pairs weigh more, in both steps (Luo and Qi, 2009, with a continuous
weight in place of their travel-time zones); the functions and their
values are those of [`accessibility()`](accessibility.md), and the trips
beyond the cutoff still count with their weight, up to
`max_trip_duration`.

The sum over the demand points of demand times `accessibility` equals
the supply of the facilities that have any demand in their catchment.
The travel times are computed with
[`travel_time_matrix()`](travel_time_matrix.md), from each demand point
to each facility, or taken from `travel_times`.

## References

Luo W, Wang F (2003). "Measures of spatial accessibility to health care
in a GIS environment: synthesis and a case study in the Chicago region."
*Environment and Planning B: Planning and Design*, **30**(6), 865-884.
[doi:10.1068/b29120](https://doi.org/10.1068/b29120) .

Luo W, Qi Y (2009). "An enhanced two-step floating catchment area
(E2SFCA) method for measuring spatial accessibility to primary care
physicians." *Health & Place*, **15**(4), 1100-1107.
[doi:10.1016/j.healthplace.2009.06.002](https://doi.org/10.1016/j.healthplace.2009.06.002)
.

## See also

[`locate_facilities()`](locate_facilities.md) to choose where to open
facilities.

Other location functions: [`locate_facilities()`](locate_facilities.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
points <- read.csv(file.path(data_path, "spo_points.csv"))

# schools per resident within a 20-minute walk
schools <- points[points$schools > 0, ]
fca <- floating_catchment(
  network, points, schools, cutoff = 20,
  demand_colname = "population", supply_colname = "schools",
  mode = "WALK", n_threads = 2
)
head(fca$demand)
#> # A tibble: 6 × 5
#>   id                lon   lat demand accessibility
#>   <chr>           <dbl> <dbl>  <dbl>         <dbl>
#> 1 89a8100c00fffff -46.6 -23.6   3648     0.0000454
#> 2 89a8100c007ffff -46.6 -23.6   3936     0.0000454
#> 3 89a8100c077ffff -46.6 -23.6   6514     0.000140 
#> 4 89a8100c03bffff -46.6 -23.6   3670     0.0000454
#> 5 89a8100c033ffff -46.6 -23.6    893     0.000207 
#> 6 89a8100c0afffff -46.6 -23.6   2185     0.000205 
fca$supply
#> # A tibble: 6 × 6
#>   id                lon   lat supply catchment_demand     ratio
#>   <chr>           <dbl> <dbl>  <dbl>            <dbl>     <dbl>
#> 1 89a8100c077ffff -46.6 -23.6      1            22033 0.0000454
#> 2 89a8100c1dbffff -46.6 -23.6      2            17501 0.000114 
#> 3 89a8100c39bffff -46.6 -23.5      1            10593 0.0000944
#> 4 89a8100c1cbffff -46.6 -23.6      1            21263 0.0000470
#> 5 89a8100c153ffff -46.6 -23.5      1            22664 0.0000441
#> 6 89a8100c14fffff -46.6 -23.5      1            11967 0.0000836
```
