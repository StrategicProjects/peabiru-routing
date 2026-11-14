# Travel-time matrices

[`travel_time_matrix()`](../reference/travel_time_matrix.md) returns,
for every origin and destination, how long the trip takes. This article
explains what decides that number, mode by mode, and which arguments
change it. All the code runs on the sample network bundled with the
package:

\
[`library`](https://rdrr.io/r/base/library.html)`(`[`peabiru`](https://strategicprojects.github.io/peabiru-routing/r/)`)`\
`data_path`` ``<-`` `[`system.file`](https://rdrr.io/r/base/system.file.html)`(``"extdata/spo"``, package ``=`` ``"peabiru"``)`\
`network`` ``<-`` `[`build_network`](../reference/build_network.md)`(``data_path``, n_threads ``=`` ``2``)`\
`points`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_path``, ``"spo_points.csv"``)``)`\
`departure`` ``<-`` `[`as.POSIXct`](https://rdrr.io/r/base/as.POSIXlt.html)`(``"2019-05-13 14:00:00"``, tz ``=`` ``"America/Sao_Paulo"``)`

## Streets: walking, cycling, driving

![Street routing: a point is linked to the nearest street open to the
mode, a search expands directed edges from the origin up to the time
limit, and each destination is reached through either end of its own
edge. Walking uses walk_speed; cycling uses bike_speed up to max_lts and
walks the bike elsewhere; driving uses each edge's speed, turn costs and
turn restrictions.](figures/street-routing.svg)

Each point is first *linked* to the street network: to the nearest
street that the mode may use, looking within 300 m and then within 1600
m. A point with no such street is not routed, and its pairs are missing
from the result. The stretch between the point and the street is walked,
whatever the mode.

The search then runs over the streets until the time limit:
`max_trip_duration`, or the mode’s own limit (`max_walk_time`,
`max_bike_time`, `max_car_time`) when it is lower. Times are rounded
down to whole minutes, and pairs beyond the limit are left out of the
result rather than returned as `NA`.

\
`walk`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``network``, ``points``, ``points``, mode ``=`` ``"WALK"``,`\
`                           n_threads ``=`` ``2``)`\
[`nrow`](https://rdrr.io/r/base/nrow.html)`(``walk``)`` ``==`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``points``)``^``2`\
`#> [1] TRUE`\
\
`walk15`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``network``, ``points``, ``points``, mode ``=`` ``"WALK"``,`\
`                             max_walk_time ``=`` ``15``, n_threads ``=`` ``2``)`\
[`nrow`](https://rdrr.io/r/base/nrow.html)`(``walk15``)`\
`#> [1] 234`\
[`range`](https://rdrr.io/r/base/range.html)`(``walk15``$``travel_time_p50``)`\
`#> [1]  0 15`

Cyclists ride at `bike_speed` on streets whose *level of traffic stress*
(LTS, from 1 to 4, derived from the OpenStreetMap tags) is at most
`max_lts`. On more stressful streets they get off and walk the bike, so
a cautious cyclist is slower:

\
`lts`` ``<-`` `[`sapply`](https://rdrr.io/r/base/lapply.html)`(``1``:``4``, ``function``(``l``)`` ``{`\
`  ``m`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``network``, ``points``, ``points``, mode ``=`` ``"BICYCLE"``,`\
`                          max_lts ``=`` ``l``, n_threads ``=`` ``2``)`\
`  `[`mean`](https://rdrr.io/r/base/mean.html)`(``m``$``travel_time_p50``)`\
`}``)`\
[`setNames`](https://rdrr.io/r/stats/setNames.html)`(`[`round`](https://rdrr.io/r/base/Round.html)`(``lts``, ``2``)``, `[`paste0`](https://rdrr.io/r/base/paste.html)`(``"max_lts = "``, ``1``:``4``)``)`\
`#> max_lts = 1 max_lts = 2 max_lts = 3 max_lts = 4 `\
`#>       12.52       11.82       10.80        8.17`

Cars use each street’s speed (its `maxspeed` tag, or a default for its
road class), pay 10 s for a right turn, 30 s for a left turn and 90 s
for a U-turn, and obey turn restrictions.

## Public transport

![How a public transport travel-time matrix is computed: walk from the
origin, RAPTOR over every departure minute of the window (with Monte
Carlo draws for frequency-based trips), walk to each destination, then
percentiles over the minutes and draws.](figures/transit-matrix.svg)

With `mode = c("WALK", "TRANSIT")` a trip is a walk to a stop, up to
`max_rides` vehicles with walking transfers between them, and a walk to
the destination, or simply a walk all the way. The search is repeated
for every minute of `time_window` starting at `departure_datetime`; the
matrix reports `percentiles` of those travel times.

\
`ttm`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(`\
`  ``network``, ``points``, ``points``,`\
`  mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`  departure_datetime ``=`` ``departure``,`\
`  time_window ``=`` ``10``,`\
`  percentiles ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``10``, ``50``, ``90``)``,`\
`  seed ``=`` ``42``,`\
`  n_threads ``=`` ``2`\
`)`\
[`head`](https://rdrr.io/r/utils/head.html)`(``ttm``)`\
`#> ``# A tibble: 6 × 5`\
`#>   from_id         to_id          travel_time_p10 travel_time_p50 travel_time_p90`\
`#>   ``<chr>``           ``<chr>``                    ``<int>``           ``<int>``           ``<int>`\
`#> ``1`` 89a8100c00fffff 89a8100c00fff…               0               0               0`\
`#> ``2`` 89a8100c00fffff 89a8100c007ff…               7               7               7`\
`#> ``3`` 89a8100c00fffff 89a8100c077ff…              10              10              10`\
`#> ``4`` 89a8100c00fffff 89a8100c03bff…               8               8               8`\
`#> ``5`` 89a8100c00fffff 89a8100c033ff…              13              13              13`\
`#> ``6`` 89a8100c00fffff 89a8100c0afff…              23              23              23`

### Frequency-based feeds and `seed`

When a feed gives frequencies (a bus every 10 minutes) instead of
timetables, the exact departure times are unknown. Each departure minute
is then simulated `draws_per_minute` times with random offsets, so the
percentiles also reflect the uncertainty of the schedule. The draws come
from a random number generator seeded by `seed` and the origin’s
coordinates: with a seed the result is reproducible; without one it
changes from run to run.

\
`again`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(`\
`  ``network``, ``points``, ``points``, mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`  departure_datetime ``=`` ``departure``, percentiles ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``10``, ``50``, ``90``)``,`\
`  seed ``=`` ``42``, n_threads ``=`` ``2`\
`)`\
[`identical`](https://rdrr.io/r/base/identical.html)`(``ttm``, ``again``)`\
`#> [1] TRUE`\
\
`other`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(`\
`  ``network``, ``points``, ``points``, mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`  departure_datetime ``=`` ``departure``, percentiles ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``10``, ``50``, ``90``)``,`\
`  seed ``=`` ``7``, n_threads ``=`` ``2`\
`)`\
[`mean`](https://rdrr.io/r/base/mean.html)`(``other``$``travel_time_p50`` ``!=`` ``ttm``$``travel_time_p50``)`\
`#> [1] 0.09876543`

### The time window

A single departure minute still varies with a frequency-based feed (the
draws), but a wider window also averages over when you leave:

\
`spread`` ``<-`` ``function``(``m``)`` `[`mean`](https://rdrr.io/r/base/mean.html)`(``m``$``travel_time_p90`` ``-`` ``m``$``travel_time_p10``)`\
`tw1`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(`\
`  ``network``, ``points``, ``points``, mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`  departure_datetime ``=`` ``departure``, time_window ``=`` ``1``,`\
`  percentiles ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``10``, ``50``, ``90``)``, seed ``=`` ``42``, n_threads ``=`` ``2`\
`)`\
[`c`](https://rdrr.io/r/base/c.html)`(``time_window_1 ``=`` ``spread``(``tw1``)``, time_window_10 ``=`` ``spread``(``ttm``)``)`\
`#>  time_window_1 time_window_10 `\
`#>      0.8737997      1.1385460`

### Walking limits

`max_walk_time` limits each walk (the access walk, each transfer and the
egress walk) and also the walk-only option; transfers are never longer
than 1 km. Pairs that need more walking than allowed disappear from the
result:

\
`short_walks`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(`\
`  ``network``, ``points``, ``points``, mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`  departure_datetime ``=`` ``departure``, max_walk_time ``=`` ``5``,`\
`  seed ``=`` ``42``, n_threads ``=`` ``2`\
`)`\
[`c`](https://rdrr.io/r/base/c.html)`(``default ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``ttm``)``, max_walk_time_5 ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``short_walks``)``)`\
`#>         default max_walk_time_5 `\
`#>             729             270`

## Threads

`n_threads` (all cores by default) spreads the origins over threads. The
result does not depend on it.

\
`one`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(`\
`  ``network``, ``points``, ``points``, mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`  departure_datetime ``=`` ``departure``, percentiles ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``10``, ``50``, ``90``)``,`\
`  seed ``=`` ``42``, n_threads ``=`` ``1`\
`)`\
[`identical`](https://rdrr.io/r/base/identical.html)`(``one``, ``ttm``)`\
`#> [1] TRUE`

## Stopping a long call

An interrupt (Esc, or Ctrl-C in a terminal) stops any of the routing
functions within a few tenths of a second and returns to the prompt,
with no partial result.
[`setTimeLimit()`](https://rdrr.io/r/base/setTimeLimit.html) works too,
and `tryCatch(interrupt = )` sees the interrupt as it would for R code.
[`build_network()`](../reference/build_network.md) cannot be
interrupted.
