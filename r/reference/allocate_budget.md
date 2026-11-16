# Share a budget of facilities among groups

Given, for each group (a municipality, a district), what its first,
second, third... new facility would add, decides how many facilities
each group gets out of a common budget, and in what order.

## Usage

``` r
allocate_budget(
  curves,
  budget,
  groups = NULL,
  criterion = "utilitarian",
  weights = NULL,
  tiers = NULL,
  eligible = NULL
)
```

## Arguments

- curves:

  A `data.frame` with one row per facility of each group: `group` and
  `marginal` (what that facility adds, not negative), the rows of a
  group in the order of its facilities (or ordered by the column
  `order`, when there is one).

- budget:

  How many facilities to share out. Fewer are given when the groups have
  no more facilities that add anything.

- groups:

  A `data.frame` with one row per group: `group`, `total` (its demand)
  and, optionally, `covered` (the demand covered before any new
  facility; 0 without the column). `NULL` when the shares are not
  wanted; required by `"maximin"`. A group may have no row in `curves`.

- criterion:

  `"utilitarian"` or `"maximin"`.

- weights:

  The name of a column of `groups` with the weight of each group, above
  zero. `NULL` (default): every group weighs 1.

- tiers:

  The name of a column of `groups` with the priority tier of each group,
  a whole number from 0, the lowest served first. `NULL` (default): one
  tier.

- eligible:

  The name of a logical column of `groups`: whether each group may
  receive facilities. `NULL` (default): all may.

## Value

A list of two tables:

- `steps`: one row per facility, in the order they are given: `order`,
  `group`, `marginal`, `covered` (the demand covered in all the groups
  after it, what was covered before included) and `lowest_share` (the
  lowest share covered among the groups after it; `NA` without
  `groups`). The first `k` rows are the answer for a budget of `k`.

- `groups`: one row per group: `group`, `units` (how many facilities it
  gets), `gained`, `covered`, `share` (`NA` without `groups`) and
  `priority` (the order in which the groups get their first facility: 1
  for the first group to receive one, `NA` for a group that gets none).

  Tables are tibbles by default;
  `options(peabiru.output = "data.frame")` gives base `data.frame`s and
  `options(peabiru.output = "data.table")` gives `data.table`s (see
  [peabiru-package](peabiru-package.md)).

## Details

The curves come from [`locate_facilities()`](locate_facilities.md) with
`model = "max_cover"` and `swap = FALSE`, one call per group asking for
as many facilities as there are candidates: the `marginal` column of
`sites` is then what each further facility adds, and it does not
increase.

With `criterion = "utilitarian"` each facility goes to the group where
the next one adds most. Because the gains of a group do not increase,
this covers the most demand possible, for the budget asked and for every
smaller one (marginal analysis; Fox, 1966). With `"maximin"` each
facility goes to the group with the lowest share of its demand covered
that still has a facility to open, which gives the highest possible
lowest share, again for every budget. Ties go to the larger gain, then
to the group listed first. The difference between the `covered` of the
two criteria for the same budget is what the fairer allocation costs
(Bertsimas, Farias and Trichakis, 2011).

## Weights, priority tiers and groups left out

Three columns of `groups`, named in `weights`, `tiers` and `eligible`,
change who comes first.

- `weights`: how much the demand of each group counts (say 2 where the
  need is most urgent, 1 elsewhere). `"utilitarian"` multiplies the
  gains of a group by its weight; as the weighted gains still do not
  increase, this is the largest weighted sum for every budget.
  `"maximin"` divides the share covered of a group by its weight, so
  that a group of weight 2 at 60% is served before one of weight 1 at
  40%.

- `tiers`: whole numbers, the lowest first. A facility only goes to a
  group when no group of a lower tier has one left to open that adds
  anything; within the tier the criterion decides. With `"maximin"` this
  is "the worst covered among the most urgent first".

- `eligible`: `FALSE` for the groups that get no facility. They stay in
  the result with zero units and count in `covered` and in
  `lowest_share`.

The result is never weighted: `marginal`, `covered`, `gained` and the
shares are demand as given, so that runs with and without the columns
can be compared.

## References

Bertsimas D, Farias VF, Trichakis N (2011). "The price of fairness."
*Operations Research*, **59**(1), 17-31.
[doi:10.1287/opre.1100.0865](https://doi.org/10.1287/opre.1100.0865) .

Fox B (1966). "Discrete optimization via marginal analysis." *Management
Science*, **13**(3), 210-216.
[doi:10.1287/mnsc.13.3.210](https://doi.org/10.1287/mnsc.13.3.210) .

## See also

Other facility location: [`distance_matrix()`](distance_matrix.md)

## Examples

``` r
curves <- data.frame(
  group = c("north", "north", "north", "south", "south"),
  marginal = c(500, 200, 50, 400, 300)
)
groups <- data.frame(group = c("north", "south"), covered = c(1000, 0),
                     total = c(2000, 1000))
allocate_budget(curves, budget = 3, groups = groups)$steps
#> # A tibble: 3 × 5
#>   order group marginal covered lowest_share
#>   <int> <chr>    <dbl>   <dbl>        <dbl>
#> 1     1 north      500    1500          0  
#> 2     2 south      400    1900          0.4
#> 3     3 south      300    2200          0.7
allocate_budget(curves, budget = 3, groups = groups, criterion = "maximin")$steps
#> # A tibble: 3 × 5
#>   order group marginal covered lowest_share
#>   <int> <chr>    <dbl>   <dbl>        <dbl>
#> 1     1 south      400    1400          0.4
#> 2     2 south      300    1700          0.5
#> 3     3 north      500    2200          0.7

# the south counts double; then the north first, whatever it adds
groups$weight <- c(1, 2)
groups$tier <- c(0, 1)
allocate_budget(curves, budget = 3, groups = groups, weights = "weight")$groups
#> # A tibble: 2 × 6
#>   group units gained covered share priority
#>   <chr> <int>  <dbl>   <dbl> <dbl>    <int>
#> 1 north     1    500    1500  0.75        2
#> 2 south     2    700     700  0.7         1
allocate_budget(curves, budget = 3, groups = groups, tiers = "tier")$groups
#> # A tibble: 2 × 6
#>   group units gained covered share priority
#>   <chr> <int>  <dbl>   <dbl> <dbl>    <int>
#> 1 north     3    750    1750 0.875        1
#> 2 south     0      0       0 0           NA
```
