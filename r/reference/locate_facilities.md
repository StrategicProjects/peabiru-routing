# Choose where to open new facilities

Chooses, among the candidate sites, where to open facilities so that the
demand points are served best, by one of four classic location models.
The travel times from every demand point to every site are computed with
[`travel_time_matrix()`](travel_time_matrix.md) (or taken from
`travel_times`), so any mode, departure time and routing option of the
matrix applies.

## Usage

``` r
locate_facilities(
  network,
  demand,
  candidates,
  existing = NULL,
  model = "max_cover",
  n_facilities = 1L,
  cutoff = NULL,
  demand_colname = NULL,
  mode = "WALK",
  mode_egress = "WALK",
  departure_datetime = NULL,
  percentile = 50L,
  max_trip_duration = NULL,
  travel_times = NULL,
  swap = TRUE,
  min_gain = 0,
  unreachable_time = NULL,
  n_threads = Inf,
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

- candidates:

  The sites where a facility could open: a `data.frame` or `POINT sf`
  with `id`, `lon`, `lat`.

- existing:

  The facilities already open (`id`, `lon`, `lat`), or `NULL`. Their ids
  may not repeat those of the candidates.

- model:

  A string: `"max_cover"`, `"set_cover"`, `"p_median"` or `"p_center"`.

- n_facilities:

  How many new facilities to open. Fewer come back when no further site
  improves the objective. Ignored by `"set_cover"`.

- cutoff:

  The service standard of the covering models, in minutes: a point is
  covered by a site at most this far (travel time `<= cutoff`). Required
  by `"max_cover"` and `"set_cover"`, `NULL` for the other models.

- demand_colname:

  The name of the numeric column of `demand` with the weight of each
  point (residents, pupils...), finite and not negative; `NULL` for a
  weight of 1 each.

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
  for the covering models, 120 for the others.

- travel_times:

  A table of travel times to use instead of routing: `from_id` (demand
  point), `to_id` (site) and `travel_time_pXX` for the `percentile` (or
  `travel_time`), in minutes, as
  [`travel_time_matrix()`](travel_time_matrix.md) returns it. A pair
  that is absent or has a missing time is unreachable; rows of other
  points are ignored. With it, `network` and the routing arguments are
  not used and the points need only their `id`.

- swap:

  A logical: improve the greedy solution by vertex substitution (for
  `"set_cover"`, drop the sites the others make redundant).

- min_gain:

  `"max_cover"` stops opening sites when the best one left would cover
  less than this much demand.

- unreachable_time:

  For `"p_median"` and `"p_center"`, the minutes charged to a demand
  point that reaches no open facility. `NULL`: twice the longest travel
  time of the table.

- n_threads:

  The number of threads (`Inf`: all cores).

- ...:

  Any other argument of [`travel_time_matrix()`](travel_time_matrix.md),
  by its full name: `time_window`, `walk_speed`, `max_walk_time`, `seed`
  and so on (not `percentiles` or `output_dir`).

## Value

A list of three tables and a summary:

- `sites`: the new sites, ordered by what each adds: `id` (with `lon`
  and `lat` when the candidates have them), `order`, `marginal` (demand
  newly covered, or by how much the objective falls) and `cumulative`
  (demand covered so far, existing facilities included, or the objective
  so far).

- `candidates`: one row per candidate: `id`, `potential` (what it would
  add as the only new facility; for the covering models, the demand not
  covered today that it would cover alone) and `chosen`.

- `demand`: one row per demand point: `id`, `demand` (its weight),
  `facility_id` and `travel_time` (the nearest open facility, existing
  or new, and the minutes to it; `NA` when it reaches none) and
  `covered` (within the cutoff of a facility; for `"p_median"` and
  `"p_center"`, whether it reaches one).

- `summary`: a list with `model`, `n_sites`, `objective_before`
  (existing facilities only), `objective_greedy`, `objective` (demand
  covered, number of sites, weighted minutes or largest minutes, by
  model), `covered_before` and `covered` (covering models, else `NA`),
  `total_demand`, `unservable_demand` (demand no site can serve) and
  `n_swaps`.

  Tables are tibbles by default;
  `options(peabiru.output = "data.frame")` gives base `data.frame`s and
  `options(peabiru.output = "data.table")` gives `data.table`s (see
  [peabiru-package](peabiru-package.md)).

## Models

- `"max_cover"`, maximal covering (Church and ReVelle, 1974): with
  `n_facilities` new facilities, cover the most demand within `cutoff`
  minutes.

- `"set_cover"`, location set covering (Toregas et al., 1971): the
  fewest new facilities that leave every demand point within `cutoff`
  minutes of one (every point some site can cover; the demand of the
  others is reported in `summary$unservable_demand`).

- `"p_median"` (Hakimi, 1964): `n_facilities` new facilities with the
  smallest demand-weighted sum of travel times to the nearest facility.

- `"p_center"` (Hakimi, 1964): `n_facilities` new facilities with the
  smallest travel time for the demand point farthest from its nearest
  facility.

The `existing` facilities are already open and count first: only the
demand they leave uncovered, or the travel time they leave, drives the
choice of the new sites. They are never closed or moved.

## How the models are solved

By heuristics, with no integer programming solver: a greedy construction
(each site the one that improves the objective most, given the ones
before it) followed, with `swap`, by vertex substitution (Teitz and
Bart, 1968): an open new site is swapped for a closed candidate while
that improves the objective, the best swap of all at each step. For
`"set_cover"` the greedy choice is the site covering the most points not
yet covered, and the improvement drops the sites the others make
redundant; for `"p_center"` greedy and swaps compare the largest travel
time and, on ties, the weighted sum. The solution is good and
reproducible, not proven optimal. For maximal covering the greedy
solution covers at least 63% (1 - 1/e) of the most that could be covered
of the demand not yet covered (Nemhauser, Wolsey and Fisher, 1978). Ties
go to the candidate listed first, and the result does not depend on
`n_threads`.

In `"set_cover"` and `"p_center"` a demand point with weight 0 is not
demand, and the others count the same whatever their weight. In
`"p_median"` and `"p_center"` a demand point that reaches no open
facility (within `max_trip_duration`) is charged `unreachable_time`
minutes, and one that no site at all reaches is left out and reported.

## References

Church R, ReVelle C (1974). "The maximal covering location problem."
*Papers of the Regional Science Association*, **32**, 101-118.
[doi:10.1007/BF01942293](https://doi.org/10.1007/BF01942293) .

Hakimi SL (1964). "Optimum locations of switching centers and the
absolute centers and medians of a graph." *Operations Research*,
**12**(3), 450-459.
[doi:10.1287/opre.12.3.450](https://doi.org/10.1287/opre.12.3.450) .

Nemhauser GL, Wolsey LA, Fisher ML (1978). "An analysis of
approximations for maximizing submodular set functions-I." *Mathematical
Programming*, **14**, 265-294.
[doi:10.1007/BF01588971](https://doi.org/10.1007/BF01588971) .

Teitz MB, Bart P (1968). "Heuristic methods for estimating the
generalized vertex median of a weighted graph." *Operations Research*,
**16**(5), 955-961.
[doi:10.1287/opre.16.5.955](https://doi.org/10.1287/opre.16.5.955) .

Toregas C, Swain R, ReVelle C, Bergman L (1971). "The location of
emergency service facilities." *Operations Research*, **19**(6),
1363-1373.
[doi:10.1287/opre.19.6.1363](https://doi.org/10.1287/opre.19.6.1363) .

## See also

[`floating_catchment()`](floating_catchment.md) for supply against
demand around each place.

Other location functions:
[`floating_catchment()`](floating_catchment.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
points <- read.csv(file.path(data_path, "spo_points.csv"))

# two new schools, so that the most residents are within a 12-minute
# walk of a school
schools <- points[points$schools > 0, ]
others <- points[points$schools == 0, ]
found <- locate_facilities(
  network, points, others, existing = schools,
  model = "max_cover", n_facilities = 2, cutoff = 12,
  demand_colname = "population", mode = "WALK", n_threads = 2
)
found$sites
#> # A tibble: 2 × 6
#>   id                lon   lat order marginal cumulative
#>   <chr>           <dbl> <dbl> <int>    <dbl>      <dbl>
#> 1 89a8100c03bffff -46.6 -23.6     1     5658      43534
#> 2 89a8100c383ffff -46.6 -23.5     2     2683      46217
found$summary$covered / found$summary$total_demand
#> [1] 0.9987898
```
