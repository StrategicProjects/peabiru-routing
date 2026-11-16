# Accessibility

[`accessibility()`](../reference/accessibility.md) summarises, for each
origin, how many opportunities (jobs, schools, hospitals…) can be
reached, either counting those within a time limit or weighting every
one of them by how long it takes to get there. The travel times come
from the same computation as
[`travel_time_matrix()`](../reference/travel_time_matrix.md).

![Accessibility is the sum over destinations of the opportunities times
a weight that decays with the travel time; five decay functions are
drawn for a 30 minute cutoff: step, exponential, fixed exponential,
linear and logistic.](figures/accessibility.svg)

Two details of the computation: the travel time fed to the decay
function is the percentile in **seconds**, not rounded to minutes as in
the matrix, and the exponentials are computed with Apache Commons Math’s
`FastMath.exp` algorithm, not with the C library’s, so the values do not
depend on the platform’s maths library.

## Cumulative opportunities

The `"step"` function counts the opportunities reached in less than each
cutoff. Each origin gets one row per opportunity column, percentile and
cutoff:

\
`access`` ``<-`` `[`accessibility`](../reference/accessibility.md)`(`\
`  ``network``, ``points``, ``points``,`\
`  opportunities_colnames ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"jobs"``, ``"schools"``)``,`\
`  mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`  departure_datetime ``=`` ``departure``,`\
`  decay_function ``=`` ``"step"``,`\
`  cutoffs ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``15``, ``30``)``,`\
`  percentiles ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``25``, ``75``)``,`\
`  seed ``=`` ``42``,`\
`  n_threads ``=`` ``2`\
`)`\
[`head`](https://rdrr.io/r/utils/head.html)`(``access``, ``8``)`\
`#> ``# A tibble: 8 × 5`\
`#>   id              opportunity percentile cutoff accessibility`\
`#>   ``<chr>``           ``<chr>``            ``<int>``  ``<int>``         ``<dbl>`\
`#> ``1`` 89a8100c00fffff jobs                25     15         ``12``452`\
`#> ``2`` 89a8100c00fffff jobs                25     30         ``95``249`\
`#> ``3`` 89a8100c00fffff jobs                75     15         ``12``452`\
`#> ``4`` 89a8100c00fffff jobs                75     30         ``81``835`\
`#> ``5`` 89a8100c00fffff schools             25     15             1`\
`#> ``6`` 89a8100c00fffff schools             25     30             4`\
`#> ``7`` 89a8100c00fffff schools             75     15             1`\
`#> ``8`` 89a8100c00fffff schools             75     30             4`

With `"step"`, trips longer than the largest cutoff are not routed at
all (`max_trip_duration` becomes `max(cutoffs)`), and
`max_trip_duration` may not be shorter than the largest cutoff with any
function.

## Decay functions

The other functions weight every opportunity by its travel time. Their
`decay_value` means different things: the width of the ramp in minutes
for `"linear"`, the standard deviation in minutes for `"logistic"`, the
decay constant per second for `"fixed_exponential"`; `"exponential"`
uses the cutoff as its half-life.

\
`one`` ``<-`` ``function``(``fun``, ``value`` ``=`` ``NULL``, ``cutoffs`` ``=`` ``20``)`` ``{`\
`  ``a`` ``<-`` `[`accessibility`](../reference/accessibility.md)`(`\
`    ``network``, ``points``[``1``:``3``, ``]``, ``points``,`\
`    opportunities_colnames ``=`` ``"jobs"``,`\
`    mode ``=`` ``"WALK"``,`\
`    decay_function ``=`` ``fun``, decay_value ``=`` ``value``, cutoffs ``=`` ``cutoffs``,`\
`    n_threads ``=`` ``2`\
`  ``)`\
`  `[`setNames`](https://rdrr.io/r/stats/setNames.html)`(`[`round`](https://rdrr.io/r/base/Round.html)`(``a``$``accessibility``)``, ``a``$``id``)`\
`}`\
[`rbind`](https://rdrr.io/r/base/cbind.html)`(`\
`  step ``=`` ``one``(``"step"``)``,`\
`  exponential ``=`` ``one``(``"exponential"``)``,`\
`  linear ``=`` ``one``(``"linear"``, ``10``)``,`\
`  logistic ``=`` ``one``(``"logistic"``, ``5``)``,`\
`  fixed_exponential ``=`` ``one``(``"fixed_exponential"``, ``0.001``, cutoffs ``=`` ``NULL``)`\
`)`\
`#>                   89a8100c00fffff 89a8100c007ffff 89a8100c077ffff`\
`#> step                        16948           24325           31926`\
`#> exponential                 42932           45737           48674`\
`#> linear                      18205           22571           29017`\
`#> logistic                    20363           25331           31449`\
`#> fixed_exponential           23096           25598           28264`

`"fixed_exponential"` ignores cutoffs, so `cutoffs` must be `NULL` and
the result has no `cutoff` column.

## Street modes

Any street mode works too; the times are then the same for every
percentile.

\
`by_bike`` ``<-`` `[`accessibility`](../reference/accessibility.md)`(`\
`  ``network``, ``points``, ``points``,`\
`  opportunities_colnames ``=`` ``"jobs"``,`\
`  mode ``=`` ``"BICYCLE"``,`\
`  decay_function ``=`` ``"logistic"``, decay_value ``=`` ``5``, cutoffs ``=`` ``15``,`\
`  n_threads ``=`` ``2`\
`)`\
[`summary`](https://rdrr.io/r/base/summary.html)`(``by_bike``$``accessibility``)`\
`#>    Min. 1st Qu.  Median    Mean 3rd Qu.    Max. `\
`#>   27175   65097   85186   78586   94017  102146`

## Missing counts

Missing opportunity counts count as zero, with a warning:

\
`holes`` ``<-`` ``points`\
`holes``$``jobs``[``2``]`` ``<-`` ``NA`\
`a`` ``<-`` `[`accessibility`](../reference/accessibility.md)`(``network``, ``points``[``1``, ``]``, ``holes``,`\
`                   opportunities_colnames ``=`` ``"jobs"``, mode ``=`` ``"WALK"``,`\
`                   decay_function ``=`` ``"step"``, cutoffs ``=`` ``30``, n_threads ``=`` ``2``)`\
`` #> Warning: `jobs` has 1 missing value, treated as 0. ``\
`a`\
`#> ``# A tibble: 1 × 5`\
`#>   id              opportunity percentile cutoff accessibility`\
`#>   ``<chr>``           ``<chr>``            ``<int>``  ``<int>``         ``<dbl>`\
`#> ``1`` 89a8100c00fffff jobs                50     30         ``57``664`
