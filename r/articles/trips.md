# Example: how much a trip by public transport varies

By car or on foot, a trip takes the same time whenever it starts. By
public transport it does not: it depends on when the next vehicle
leaves. This example measures that variation in two ways, with
percentiles over a window of departures, and trip by trip. Everything
below is one script; the output shown is the output it prints.

\
[`library`](https://rdrr.io/r/base/library.html)`(`[`peabiru`](https://strategicprojects.github.io/peabiru-routing/r/)`)`\
[`library`](https://rdrr.io/r/base/library.html)`(`[`ggplot2`](https://ggplot2.tidyverse.org)`)`

## Percentiles on a frequency-based network

The São Paulo sample that ships with the package has a feed described by
frequencies (“a bus every 8 minutes”) rather than timetables. For such
routes peabiru draws random schedules consistent with the frequencies, 5
per departure minute by default, so the travel times of a window of
departures form a distribution. `seed` makes the draws reproducible.

\
`spo`` ``<-`` `[`system.file`](https://rdrr.io/r/base/system.file.html)`(``"extdata/spo"``, package ``=`` ``"peabiru"``)`\
`network`` ``<-`` `[`build_network`](../reference/build_network.md)`(``spo``)`\
`points`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``spo``, ``"spo_points.csv"``)``)`\
`departure`` ``<-`` `[`as.POSIXct`](https://rdrr.io/r/base/as.POSIXlt.html)`(``"2019-05-13 14:00:00"``, tz ``=`` ``"America/Sao_Paulo"``)`\
\
`ttm`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(`\
`  ``network``, ``points``[``1``, ``]``, ``points``[``-``1``, ``]``,`\
`  mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`  departure_datetime ``=`` ``departure``,`\
`  time_window ``=`` ``60``,`\
`  percentiles ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``5``, ``25``, ``50``, ``75``, ``95``)``,`\
`  seed ``=`` ``42`\
`)`\
`ttm`` ``<-`` ``ttm``[`[`order`](https://rdrr.io/r/base/order.html)`(``ttm``$``travel_time_p50``, ``ttm``$``travel_time_p95``)``, ``]`\
[`head`](https://rdrr.io/r/utils/head.html)`(``ttm``)`\
`#> ``# A tibble: 6 × 7`\
`#>   from_id  to_id travel_time_p05 travel_time_p25 travel_time_p50 travel_time_p75`\
`#>   ``<chr>``    ``<chr>``           ``<int>``           ``<int>``           ``<int>``           ``<int>`\
`#> ``1`` 89a8100… 89a8…               7               7               7               7`\
`#> ``2`` 89a8100… 89a8…               8               8               8               8`\
`#> ``3`` 89a8100… 89a8…              10              10              10              10`\
`#> ``4`` 89a8100… 89a8…              13              13              13              13`\
`#> ``5`` 89a8100… 89a8…              14              14              14              14`\
`#> ``6`` 89a8100… 89a8…              17              17              17              17`\
`#> ``# ℹ 1 more variable: travel_time_p95 <int>`

The destinations closest to the origin are walked to, and every
departure takes the same time. Further away, a bus or the metro saves
time, and the spread between the 5th and 95th percentiles is the price
of not knowing when it comes:

\
`ttm``$``destination`` ``<-`` `[`factor`](https://rdrr.io/r/base/factor.html)`(``ttm``$``to_id``, levels ``=`` `[`rev`](https://rdrr.io/r/base/rev.html)`(``ttm``$``to_id``)``)`\
[`ggplot`](https://ggplot2.tidyverse.org/reference/ggplot.html)`(``ttm``, `[`aes`](https://ggplot2.tidyverse.org/reference/aes.html)`(``y ``=`` ``destination``)``)`` ``+`\
`  `[`geom_linerange`](https://ggplot2.tidyverse.org/reference/geom_linerange.html)`(`[`aes`](https://ggplot2.tidyverse.org/reference/aes.html)`(``xmin ``=`` ``travel_time_p05``, xmax ``=`` ``travel_time_p95``)``,`\
`                 colour ``=`` ``"#c98a1d"``, linewidth ``=`` ``1``)`` ``+`\
`  `[`geom_linerange`](https://ggplot2.tidyverse.org/reference/geom_linerange.html)`(`[`aes`](https://ggplot2.tidyverse.org/reference/aes.html)`(``xmin ``=`` ``travel_time_p25``, xmax ``=`` ``travel_time_p75``)``,`\
`                 colour ``=`` ``"#b4532a"``, linewidth ``=`` ``3``)`` ``+`\
`  `[`geom_point`](https://ggplot2.tidyverse.org/reference/geom_point.html)`(`[`aes`](https://ggplot2.tidyverse.org/reference/aes.html)`(``x ``=`` ``travel_time_p50``)``, shape ``=`` ``21``, fill ``=`` ``"white"``, size ``=`` ``2``)`` ``+`\
`  `[`labs`](https://ggplot2.tidyverse.org/reference/labs.html)`(``x ``=`` ``"Travel time (minutes)"``, y ``=`` ``NULL``)`` ``+`\
`  `[`theme_minimal`](https://ggplot2.tidyverse.org/reference/ggtheme.html)`(``)`` ``+`\
`  `[`theme`](https://ggplot2.tidyverse.org/reference/theme.html)`(``axis.text.y ``=`` `[`element_blank`](https://ggplot2.tidyverse.org/reference/element.html)`(``)``, panel.grid.major.y ``=`` `[`element_blank`](https://ggplot2.tidyverse.org/reference/element.html)`(``)``)`

![Range bars of the travel time from one point in São Paulo to 26
others, sorted by median: the nearest destinations have no spread; those
reached by bus or metro spread by up to about ten minutes between the
5th and 95th percentiles.](trips_files/figure-html/spread-1.png)

## Trip by trip

[`expanded_travel_time_matrix()`](../reference/expanded_travel_time_matrix.md)
keeps one row per departure minute (and per draw, on a frequency-based
network) instead of percentiles. With `breakdown = TRUE`, each trip is
split into its parts, and the routes taken are listed. Here, from
Farroupilha Park to the PUCRS university campus in Porto Alegre, whose
sample data (from the r5r project, version 2.5.0) has bus timetables:

\
`poa`` ``<-`` `[`file.path`](https://rdrr.io/r/base/file.path.html)`(`[`tempdir`](https://rdrr.io/r/base/tempfile.html)`(``)``, ``"poa"``)`\
[`dir.create`](https://rdrr.io/r/base/files2.html)`(``poa``, showWarnings ``=`` ``FALSE``)`\
`url`` ``<-`` ``"https://raw.githubusercontent.com/ipea/r5r/v2.5.0/r-package/inst/extdata/poa/"`\
`for`` ``(``f`` ``in`` `[`c`](https://rdrr.io/r/base/c.html)`(``"poa_osm.pbf"``, ``"poa_eptc.zip"``, ``"poa_trensurb.zip"``,`\
`            ``"poa_points_of_interest.csv"``)``)`` ``{`\
`  ``if`` ``(``!`[`file.exists`](https://rdrr.io/r/base/files.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``f``)``)``)`` ``{`\
`    `[`download.file`](https://rdrr.io/r/utils/download.file.html)`(`[`paste0`](https://rdrr.io/r/base/paste.html)`(``url``, ``f``)``, `[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``f``)``, mode ``=`` ``"wb"``, quiet ``=`` ``TRUE``)`\
`  ``}`\
`}`\
`poa_network`` ``<-`` `[`build_network`](../reference/build_network.md)`(``poa``)`\
`poi`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_points_of_interest.csv"``)``)`\
\
`trips`` ``<-`` `[`expanded_travel_time_matrix`](../reference/expanded_travel_time_matrix.md)`(`\
`  ``poa_network``,`\
`  origins ``=`` ``poi``[``poi``$``id`` ``==`` ``"farroupilha_park"``, ``]``,`\
`  destinations ``=`` ``poi``[``poi``$``id`` ``==`` ``"pucrs"``, ``]``,`\
`  mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`  departure_datetime ``=`` ``departure``,`\
`  time_window ``=`` ``30``,`\
`  breakdown ``=`` ``TRUE`\
`)`\
[`head`](https://rdrr.io/r/utils/head.html)`(``trips``[``, ``-``(``1``:``2``)``]``, ``10``)`\
`#> ``# A tibble: 10 × 10`\
`#>    departure_time draw_number access_time wait_time ride_time transfer_time`\
`#>    ``<chr>``                ``<int>``       ``<dbl>``     ``<dbl>``     ``<dbl>``         ``<dbl>`\
`#> `` 1`` 14:00:00                 1        10.1       5.1      16.5           1.4`\
`#> `` 2`` 14:01:00                 1        10.1       4.1      16.5           1.4`\
`#> `` 3`` 14:02:00                 1         6.9      10.3      16.4           0  `\
`#> `` 4`` 14:03:00                 1         6.9       9.3      16.4           0  `\
`#> `` 5`` 14:04:00                 1         6.9       8.3      16.4           0  `\
`#> `` 6`` 14:05:00                 1         6.9       7.3      16.4           0  `\
`#> `` 7`` 14:06:00                 1         6.9       6.3      16.4           0  `\
`#> `` 8`` 14:07:00                 1         6.9       2.5      21.8           4.8`\
`#> `` 9`` 14:08:00                 1         6.9       7.8      16.1           4.7`\
`#> ``10`` 14:09:00                 1         6.9       6.8      16.1           4.7`\
`#> ``# ℹ 4 more variables: egress_time <dbl>, routes <chr>, n_rides <int>,`\
`#> ``#   total_time <dbl>`

Leaving at 14:02 or at 14:20 means boarding the same line, but waiting
longer for it; at 14:07 a different pair of buses wins. Stacked bars
show where the minutes go:

\
`parts`` ``<-`` `[`c`](https://rdrr.io/r/base/c.html)`(``access_time ``=`` ``"Walk to the stop"``, wait_time ``=`` ``"Wait"``,`\
`           ride_time ``=`` ``"Ride"``, transfer_time ``=`` ``"Transfer"``,`\
`           egress_time ``=`` ``"Walk from the stop"``)`\
`long`` ``<-`` `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`names`](https://rdrr.io/r/base/names.html)`(``parts``)``, ``function``(``p``)`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``departure ``=`` `[`substr`](https://rdrr.io/r/base/substr.html)`(``trips``$``departure_time``, ``1``, ``5``)``,`\
`             part ``=`` `[`factor`](https://rdrr.io/r/base/factor.html)`(``parts``[[``p``]``]``, levels ``=`` `[`rev`](https://rdrr.io/r/base/rev.html)`(``parts``)``)``,`\
`             minutes ``=`` ``trips``[[``p``]``]``)``)``)`\
[`ggplot`](https://ggplot2.tidyverse.org/reference/ggplot.html)`(``long``, `[`aes`](https://ggplot2.tidyverse.org/reference/aes.html)`(``minutes``, ``departure``, fill ``=`` ``part``)``)`` ``+`\
`  `[`geom_col`](https://ggplot2.tidyverse.org/reference/geom_bar.html)`(``width ``=`` ``0.7``)`` ``+`\
`  `[`scale_y_discrete`](https://ggplot2.tidyverse.org/reference/scale_discrete.html)`(``limits ``=`` ``rev``)`` ``+`\
`  `[`scale_fill_manual`](https://ggplot2.tidyverse.org/reference/scale_manual.html)`(``values ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"#8db9a0"``, ``"#3d4f56"``, ``"#b4532a"``, ``"#c98a1d"``, ``"#2f6b4f"``)``,`\
`                    breaks ``=`` `[`unname`](https://rdrr.io/r/base/unname.html)`(``parts``)``, name ``=`` ``NULL``)`` ``+`\
`  `[`labs`](https://ggplot2.tidyverse.org/reference/labs.html)`(``x ``=`` ``"Minutes"``, y ``=`` ``"Departure"``)`` ``+`\
`  `[`theme_minimal`](https://ggplot2.tidyverse.org/reference/ggtheme.html)`(``)`` ``+`\
`  `[`theme`](https://ggplot2.tidyverse.org/reference/theme.html)`(``legend.position ``=`` ``"top"``, panel.grid.major.y ``=`` `[`element_blank`](https://ggplot2.tidyverse.org/reference/element.html)`(``)``)`

![Thirty stacked horizontal bars, one per departure minute from 14:00 to
14:29, split into walking to the stop, waiting, riding, transferring and
walking from the stop; the totals range from about 30 to 40 minutes, and
the waiting time changes most.](trips_files/figure-html/breakdown-1.png)

## Variations

- **Arrive by.**
  [`arrival_travel_time_matrix()`](../reference/arrival_travel_time_matrix.md)
  answers the reverse question: the latest departure that arrives by a
  given time.
- **Fewer transfers.** `max_rides = 1` keeps only the trips with a
  single vehicle; compare the totals.
- **More draws.** `draws_per_minute` sets how many random schedules are
  drawn per minute on frequency-based routes.

The figures on the [home page](../../index.html#gallery) draw the same
results with `site/figures/figures.R` in the repository.
