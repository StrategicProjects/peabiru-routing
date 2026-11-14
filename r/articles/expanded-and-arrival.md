# Expanded and arrival matrices

[`travel_time_matrix()`](../reference/travel_time_matrix.md) reduces the
trips of a departure window to a few percentiles. Two functions keep the
trips themselves:
[`expanded_travel_time_matrix()`](../reference/expanded_travel_time_matrix.md)
returns every one, and
[`arrival_travel_time_matrix()`](../reference/arrival_travel_time_matrix.md)
the one that leaves latest and still arrives in time. Both work with
public transport (`mode = c("WALK", "TRANSIT")`).

![Left: the expanded matrix has one row per departure minute and draw
for each pair, with the breakdown of the trip. Right: the arrival matrix
scans departures from the latest minute back and keeps the first trip
that arrives by the arrival time.](figures/expanded-arrival.svg)

## Every trip

\
`trips`` ``<-`` `[`expanded_travel_time_matrix`](../reference/expanded_travel_time_matrix.md)`(`\
`  ``network``, ``points``[``1``:``2``, ``]``, ``points``,`\
`  mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`  departure_datetime ``=`` ``departure``,`\
`  time_window ``=`` ``3``,`\
`  breakdown ``=`` ``TRUE``,`\
`  seed ``=`` ``42``,`\
`  n_threads ``=`` ``2`\
`)`\
[`nrow`](https://rdrr.io/r/base/nrow.html)`(``trips``)`` ``==`` ``2`` ``*`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``points``)`` ``*`` ``3`` ``*`` ``5`\
`#> [1] TRUE`\
[`head`](https://rdrr.io/r/utils/head.html)`(`[`subset`](https://rdrr.io/r/base/subset.html)`(``trips``, ``routes`` ``!=`` ``"[WALK]"``)``, ``10``)`\
`#> ``# A tibble: 10 × 12`\
`#>    from_id      to_id departure_time draw_number access_time wait_time ride_time`\
`#>    ``<chr>``        ``<chr>`` ``<chr>``                ``<int>``       ``<dbl>``     ``<dbl>``     ``<dbl>`\
`#> `` 1`` 89a8100c00f… 89a8… 14:01:00                 1        16.1       1.4       2.2`\
`#> `` 2`` 89a8100c00f… 89a8… 14:02:00                 1        16.1       1.4       2.2`\
`#> `` 3`` 89a8100c00f… 89a8… 14:02:00                 2        16.1       1.4       2.2`\
`#> `` 4`` 89a8100c00f… 89a8… 14:00:00                 1        14.7       1.3       1.9`\
`#> `` 5`` 89a8100c00f… 89a8… 14:00:00                 2        14.7       1.3       1.9`\
`#> `` 6`` 89a8100c00f… 89a8… 14:00:00                 3        14.7       2.3       1.9`\
`#> `` 7`` 89a8100c00f… 89a8… 14:00:00                 4        14.7       2.4       1.9`\
`#> `` 8`` 89a8100c00f… 89a8… 14:01:00                 1        14.7       1.3       1.9`\
`#> `` 9`` 89a8100c00f… 89a8… 14:01:00                 2        14.7       2.5       1.9`\
`#> ``10`` 89a8100c00f… 89a8… 14:01:00                 3        14.7       2.5       1.9`\
`#> ``# ℹ 5 more variables: transfer_time <dbl>, egress_time <dbl>, routes <chr>,`\
`#> ``#   n_rides <int>, total_time <dbl>`

A row is one departure minute of one origin-destination pair. This feed
is frequency-based, so each minute has `draws_per_minute` rows (5 by
default), one per simulated schedule; with a timetable-based feed there
is one row per minute. Within a minute the draws are **sorted by total
time** and numbered, so `draw_number` is a rank (1 is the fastest draw),
not the order in which the draws were made.

With `breakdown = TRUE` each trip is split into minutes of `access_time`
(walk to the first stop), `wait_time`, `ride_time`, `transfer_time`
(walks between vehicles) and `egress_time`, plus the `routes` taken
(GTFS `route_id`s joined by `|`, or `[WALK]` for a trip on foot) and
`n_rides`. Every time is in minutes with one decimal, and `total_time`
is the sum of the rounded parts:

\
`t`` ``<-`` `[`subset`](https://rdrr.io/r/base/subset.html)`(``trips``, ``routes`` ``!=`` ``"[WALK]"``)``[``1``, ``]`\
[`with`](https://rdrr.io/r/base/with.html)`(``t``, `[`c`](https://rdrr.io/r/base/c.html)`(``sum ``=`` ``access_time`` ``+`` ``wait_time`` ``+`` ``ride_time`` ``+`` ``transfer_time`` ``+`` ``egress_time``,`\
`          total_time ``=`` ``total_time``)``)`\
`#>        sum total_time `\
`#>       22.2       22.2`

Trips longer than `max_trip_duration` keep their row with `NA` times.

## The latest trip that arrives in time

[`arrival_travel_time_matrix()`](../reference/arrival_travel_time_matrix.md)
takes an `arrival_datetime` instead of a departure. It searches
departures minute by minute over the `max_trip_duration` minutes before
the arrival (the window may start on the previous day) and, for each
pair, keeps the first trip, from the latest departure backwards, that
arrives by the arrival time.

\
`arrive`` ``<-`` `[`arrival_travel_time_matrix`](../reference/arrival_travel_time_matrix.md)`(`\
`  ``network``, ``points``[``1``:``2``, ``]``, ``points``,`\
`  mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`  arrival_datetime ``=`` `[`as.POSIXct`](https://rdrr.io/r/base/as.POSIXlt.html)`(``"2019-05-13 15:00:00"``, tz ``=`` ``"America/Sao_Paulo"``)``,`\
`  max_trip_duration ``=`` ``30``,`\
`  breakdown ``=`` ``TRUE``,`\
`  seed ``=`` ``42``,`\
`  n_threads ``=`` ``2`\
`)`\
[`head`](https://rdrr.io/r/utils/head.html)`(``arrive``)`\
`#> ``# A tibble: 6 × 12`\
`#>   from_id       to_id departure_time draw_number access_time wait_time ride_time`\
`#>   ``<chr>``         ``<chr>`` ``<chr>``                ``<int>``       ``<dbl>``     ``<dbl>``     ``<dbl>`\
`#> ``1`` 89a8100c00ff… 89a8… 14:59:00                 1           0         0         0`\
`#> ``2`` 89a8100c00ff… 89a8… 14:52:00                 1           0         0         0`\
`#> ``3`` 89a8100c00ff… 89a8… 14:49:00                 1           0         0         0`\
`#> ``4`` 89a8100c00ff… 89a8… 14:51:00                 1           0         0         0`\
`#> ``5`` 89a8100c00ff… 89a8… 14:46:00                 1           0         0         0`\
`#> ``6`` 89a8100c00ff… 89a8… 14:36:00                 1           0         0         0`\
`#> ``# ℹ 5 more variables: transfer_time <dbl>, egress_time <dbl>, routes <chr>,`\
`#> ``#   n_rides <int>, total_time <dbl>`

There is at most one row per pair; pairs that cannot arrive in time are
absent.

## Limits

These functions keep the path of every trip and accept at most 5,000
destinations. The expanded matrix grows with origins × destinations ×
minutes × draws, so it is meant for a few origins at a time.
