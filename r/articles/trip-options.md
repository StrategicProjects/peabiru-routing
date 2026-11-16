# Trip options

[`trip_options()`](../reference/trip_options.md) is peabiru’s own trip
planner: for each origin and destination it gives every trip worth
considering, the ones that no other trip beats on all the chosen
criteria at once. Where
[`detailed_itineraries()`](../reference/detailed_itineraries.md) answers
“what is the fastest way there?”,
[`trip_options()`](../reference/trip_options.md) answers “what are my
choices?”: faster but dearer, cheaper but slower, fewer transfers, less
walking.

## Not dominated

A trip *dominates* another when it is no worse on every chosen criterion
and better on at least one. The criteria are

- `"TIME"`: door-to-door minutes from the departure minute, waits
  included;
- `"FARE"`: the fare by a fare structure (see `vignette("fares")`);
- `"TRANSFERS"`: rides minus one; a trip without public transport has 0,
  as many as a trip with a single ride;
- `"WALK"`: minutes on foot: the access and egress legs when they are
  walked, and the walks between stops (a bicycle or car leg counts 0).

By default all four are used with a fare structure, all but `"FARE"`
without one. At each departure minute of the window the options are the
set of trips not dominated, with one trip per combination of values.

The search is exact, to the second. What is returned is then compared at
the resolution the values are read at: total time to a tenth of a
minute, as it is reported, and walking to the whole minute (rounded half
up; `walk_resolution` sets the step in seconds, and 1 compares the exact
seconds). Of the trips of a minute, one is left out when another is no
worse on every criterion in those terms and better on one, or equal on
all of them and better in exact terms (less total time, then fewer
transfers, less walking, a lower fare). Without this, a trip that is
slower, dearer and with more transfers would be returned only because it
walks a few seconds less. The values reported stay exact. Rounding
cannot make a trip look better than one that beats it to the second, so
the result is the same as comparing every trip at that resolution.

The trips follow the same network rules as
[`detailed_itineraries()`](../reference/detailed_itineraries.md) (the
same access and egress searches, transfers between stops and boarding
rules), so the times agree with the other functions.

The example uses the sample data of Porto Alegre from the r5r project,
whose GTFS has timetables, and the fare structure that comes with it,
downloaded once into the temporary directory.

\
`poa`` ``<-`` ``poa_sample``(``)`\
`poa_network`` ``<-`` `[`build_network`](../reference/build_network.md)`(``poa``, n_threads ``=`` ``2``)`\
`fares_zip`` ``<-`` `[`file.path`](https://rdrr.io/r/base/file.path.html)`(`[`tempdir`](https://rdrr.io/r/base/tempfile.html)`(``)``, ``"fares_poa.zip"``)`\
`if`` ``(``!`[`file.exists`](https://rdrr.io/r/base/files.html)`(``fares_zip``)``)`` ``{`\
`  `[`download.file`](https://rdrr.io/r/utils/download.file.html)`(`[`paste0`](https://rdrr.io/r/base/paste.html)`(``"https://raw.githubusercontent.com/ipea/r5r/v2.5.0/r-package/"``,`\
`                       ``"inst/extdata/poa/fares/fares_poa.zip"``)``,`\
`                ``fares_zip``, mode ``=`` ``"wb"``, quiet ``=`` ``TRUE``)`\
`}`\
`fares`` ``<-`` `[`read_fare_structure`](../reference/read_fare_structure.md)`(``fares_zip``)`\
`origin`` ``<-`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` ``"89a90128053ffff"``, lat ``=`` ``-``30.0722552482611``, lon ``=`` ``-``51.1863771572777``)`\
`destination`` ``<-`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` ``"89a90129c9bffff"``, lat ``=`` ``-``30.0511737750423``, lon ``=`` ``-``51.1733521719011``)`

## Time and fare

With time and fare as the criteria, the options over the ten minutes
from 14:00 trade minutes for reais:

\
`res`` ``<-`` `[`trip_options`](../reference/trip_options.md)`(``poa_network``, ``origin``, ``destination``,`\
`                    departure_datetime ``=`` ``departure``,`\
`                    criteria ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"TIME"``, ``"FARE"``)``, fare_structure ``=`` ``fares``,`\
`                    n_threads ``=`` ``2``)`\
`res``$``options``[``, `[`c`](https://rdrr.io/r/base/c.html)`(``"option"``, ``"total_time"``, ``"fare"``, ``"transfers"``, ``"walk_time"``,`\
`                ``"departure_time"``, ``"n_departures"``, ``"description"``)``]`\
`#> ``# A tibble: 5 × 8`\
`#>   option total_time  fare transfers walk_time departure_time n_departures`\
`#>    ``<int>``      ``<dbl>`` ``<dbl>``     ``<int>``     ``<dbl>`` ``<chr>``                 ``<int>`\
`#> ``1``      1       30.8   7.2         1      16.8 14:05:00                  5`\
`#> ``2``      2       35.1   4.8         0      24.7 14:05:00                  8`\
`#> ``3``      3       35.8   7.2         1      20.4 14:00:00                  5`\
`#> ``4``      4       42.1   4.8         0      33.8 14:07:00                  2`\
`#> ``5``      5       57.7   0           0      57.7 14:00:00                 10`\
`#> ``# ℹ 1 more variable: description <chr>`

Each row is an *option*: trips with the same street modes, the same
routes between the same stops, and the same fare, transfers and minutes
on foot, found at `n_departures` of the window’s minutes. Its total time
changes with the minute (the wait at the start does), so the row reports
the trip of its best minute, `departure_time`. Over the whole window an
option can look dominated by another that was not available at its own
minutes; `by_departure = TRUE` adds the frontier of every minute:

\
`res`` ``<-`` `[`trip_options`](../reference/trip_options.md)`(``poa_network``, ``origin``, ``destination``,`\
`                    departure_datetime ``=`` ``departure``, time_window ``=`` ``3L``,`\
`                    criteria ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"TIME"``, ``"FARE"``)``, fare_structure ``=`` ``fares``,`\
`                    by_departure ``=`` ``TRUE``, itineraries ``=`` ``FALSE``, n_threads ``=`` ``2``)`\
`res``$``departures`\
`#> ``# A tibble: 9 × 6`\
`#>   from_id         to_id           option departure_time arrival_time total_time`\
`#>   ``<chr>``           ``<chr>``            ``<int>`` ``<chr>``          ``<chr>``             ``<dbl>`\
`#> ``1`` 89a90128053ffff 89a90129c9bffff      1 14:01:00       14:35:48           34.8`\
`#> ``2`` 89a90128053ffff 89a90129c9bffff      1 14:02:00       14:35:48           33.8`\
`#> ``3`` 89a90128053ffff 89a90129c9bffff      2 14:00:00       14:35:48           35.8`\
`#> ``4`` 89a90128053ffff 89a90129c9bffff      3 14:00:00       14:40:03           40.1`\
`#> ``5`` 89a90128053ffff 89a90129c9bffff      3 14:01:00       14:40:03           39.1`\
`#> ``6`` 89a90128053ffff 89a90129c9bffff      3 14:02:00       14:40:03           38.1`\
`#> ``7`` 89a90128053ffff 89a90129c9bffff      4 14:00:00       14:57:40           57.7`\
`#> ``8`` 89a90128053ffff 89a90129c9bffff      4 14:01:00       14:58:40           57.7`\
`#> ``9`` 89a90128053ffff 89a90129c9bffff      4 14:02:00       14:59:40           57.7`

## All four criteria and the legs

With the default criteria (all four, since there is a fare structure)
the trade-offs with transfers and walking come in too. The legs of each
option are in `itineraries`, in the format of
[`detailed_itineraries()`](../reference/detailed_itineraries.md) and
keyed by `from_id`, `to_id` and `option`: an `sf` of lines, or a table
without geometry with `drop_geometry = TRUE`.

\
`res`` ``<-`` `[`trip_options`](../reference/trip_options.md)`(``poa_network``, ``origin``, ``destination``,`\
`                    departure_datetime ``=`` ``departure``, fare_structure ``=`` ``fares``,`\
`                    drop_geometry ``=`` ``TRUE``, n_threads ``=`` ``2``)`\
[`nrow`](https://rdrr.io/r/base/nrow.html)`(``res``$``options``)`\
`#> [1] 4`\
`legs`` ``<-`` ``res``$``itineraries`\
`legs``[``legs``$``option`` ``==`` ``1``, `[`c`](https://rdrr.io/r/base/c.html)`(``"segment"``, ``"mode"``, ``"cumulative_fare"``,`\
`                         ``"segment_duration"``, ``"wait"``, ``"distance"``, ``"route"``)``]`\
`#> ``# A tibble: 4 × 7`\
`#>   segment mode  cumulative_fare segment_duration  wait distance route`\
`#>     ``<int>`` ``<chr>``           ``<dbl>``            ``<dbl>`` ``<dbl>``    ``<int>`` ``<chr>`\
`#> ``1``       1 WALK              0               10.9   0        694 ``""``   `\
`#> ``2``       2 BUS               4.8              7.4   1.2     ``2``113 ``"``T4``"`` `\
`#> ``3``       3 BUS               7.2              2.7   2.9     ``1``392 ``"``T1D``"`\
`#> ``4``       4 WALK              7.2              5.8   0        350 ``""`

`max_options` keeps the first options of each pair, `max_fare` and
`max_trip_duration` leave out the trips beyond them, and
`itineraries = FALSE` skips the legs. `output_dir` writes one CSV file
per pair instead.

## Fares are not additive

A fare is not the sum of its rides: a transfer within the time allowance
is discounted, some types have unlimited transfers, a cap may apply. Two
partial trips are only compared on fare when one is at least as cheap
for every way both can continue; the comparison makes no assumption
about the fare structure (a discounted transfer may even cost more than
two full fares). This exactness has a price: partial trips that could
still get a discount are kept apart until they cannot, which makes the
fare criterion the slowest one.

## Frequency-based routes

GTFS `frequencies.txt` gives a headway, not departure times. The matrix
functions draw random departures (Monte Carlo);
[`trip_options()`](../reference/trip_options.md) does not: a
frequency-based vehicle is boarded at its expected time, half a headway
after the traveller is ready to board (or after the period starts),
within the period, and with `exact_times = 1` at the first scheduled
departure. The results do not change from call to call, and differ on
purpose from those of the Monte Carlo draws. The Sao Paulo sample
shipped with the package runs by frequency:

\
`res`` ``<-`` `[`trip_options`](../reference/trip_options.md)`(``network``, ``points``[``1``, ``]``, ``points``[``27``, ``]``,`\
`                    departure_datetime ``=`` ``departure``, time_window ``=`` ``5L``,`\
`                    itineraries ``=`` ``FALSE``, n_threads ``=`` ``2``)`\
`res``$``options``[``, `[`c`](https://rdrr.io/r/base/c.html)`(``"option"``, ``"total_time"``, ``"transfers"``, ``"walk_time"``,`\
`                ``"n_departures"``, ``"description"``)``]`\
`#> ``# A tibble: 6 × 6`\
`#>   option total_time transfers walk_time n_departures description                `\
`#>    ``<int>``      ``<dbl>``     ``<int>``     ``<dbl>``        ``<int>`` ``<chr>``                      `\
`#> ``1``      1       33.1         1      20.4            5 WALK > SUBWAY METRÔ L1 > S…`\
`#> ``2``      2       33.5         0      24.7            5 WALK > SUBWAY METRÔ L3 > W…`\
`#> ``3``      3       35.1         1      18.8            5 WALK > SUBWAY METRÔ L1 > B…`\
`#> ``4``      4       37.5         0      22.7            5 WALK > BUS 2002-10 > WALK  `\
`#> ``5``      5       54.9         1      17.7            5 WALK > BUS 5290-10 > BUS 2…`\
`#> ``6``      6       60           2      15.7            5 WALK > BUS 5290-10 > BUS 5…`

## Compared with the other functions

- [`pareto_frontier()`](../reference/pareto_frontier.md) gives, for each
  fare cutoff, the percentiles of the travel time over Monte Carlo draws
  of departure times: a summary of time against budget, without the
  trips.
- [`detailed_itineraries()`](../reference/detailed_itineraries.md) gives
  the fastest trip, or with `shortest_path = FALSE` the alternatives
  (one per sequence of routes, the fastest of each).
- [`trip_options()`](../reference/trip_options.md) gives the trade-offs
  themselves, trip by trip, with their legs, on any of four criteria.
  With `criteria = "TIME"` and `time_window = 1`, its first option has
  the total time of
  [`detailed_itineraries()`](../reference/detailed_itineraries.md)’s
  fastest trip for that minute.
