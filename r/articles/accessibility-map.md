# Example: mapping access to jobs and schools

How many jobs can people reach from where they live? This example
answers that for Porto Alegre, on a grid of hexagonal cells, first
walking and then walking with public transport, and draws the answer as
a map. Everything below is one script; the output shown is the output it
prints.

## The data

The Porto Alegre sample of r5r 2.5.0 (an OpenStreetMap extract, the bus
and train GTFS feeds, and a grid of 1,227 hexagonal cells with their
population, jobs and schools), downloaded once into a temporary
directory:

\
[`library`](https://rdrr.io/r/base/library.html)`(`[`peabiru`](https://strategicprojects.github.io/peabiru-routing/r/)`)`\
[`library`](https://rdrr.io/r/base/library.html)`(`[`sf`](https://r-spatial.github.io/sf/)`)`\
[`library`](https://rdrr.io/r/base/library.html)`(`[`ggplot2`](https://ggplot2.tidyverse.org)`)`\
\
`poa`` ``<-`` `[`file.path`](https://rdrr.io/r/base/file.path.html)`(`[`tempdir`](https://rdrr.io/r/base/tempfile.html)`(``)``, ``"poa"``)`\
[`dir.create`](https://rdrr.io/r/base/files2.html)`(``poa``, showWarnings ``=`` ``FALSE``)`\
`url`` ``<-`` ``"https://raw.githubusercontent.com/ipea/r5r/v2.5.0/r-package/inst/extdata/poa/"`\
`for`` ``(``f`` ``in`` `[`c`](https://rdrr.io/r/base/c.html)`(``"poa_osm.pbf"``, ``"poa_eptc.zip"``, ``"poa_trensurb.zip"``,`\
`            ``"poa_hexgrid.csv"``, ``"poa_points_of_interest.csv"``)``)`` ``{`\
`  ``if`` ``(``!`[`file.exists`](https://rdrr.io/r/base/files.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``f``)``)``)`` ``{`\
`    `[`download.file`](https://rdrr.io/r/utils/download.file.html)`(`[`paste0`](https://rdrr.io/r/base/paste.html)`(``url``, ``f``)``, `[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``f``)``, mode ``=`` ``"wb"``, quiet ``=`` ``TRUE``)`\
`  ``}`\
`}`\
\
`network`` ``<-`` `[`build_network`](../reference/build_network.md)`(``poa``)`\
`grid`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_hexgrid.csv"``)``)`\
[`head`](https://rdrr.io/r/utils/head.html)`(``grid``)`\
`#>                id       lon       lat population schools jobs healthcare`\
`#> 1 89a901291abffff -51.15825 -30.05385          0       0 1214          0`\
`#> 2 89a9012a3cfffff -51.21187 -30.10058        159       0    0          0`\
`#> 3 89a901295b7ffff -51.16521 -30.07544       1008       0    3          1`\
`#> 4 89a901284a3ffff -51.20535 -30.09005         92       0    0          0`\
`#> 5 89a9012809bffff -51.19575 -30.07839        577       0    9          0`\
`#> 6 89a901285cfffff -51.21108 -30.08124       1170       0  427          0`\
[`colSums`](https://rdrr.io/r/base/colSums.html)`(``grid``[``, `[`c`](https://rdrr.io/r/base/c.html)`(``"population"``, ``"schools"``, ``"jobs"``)``]``, na.rm ``=`` ``TRUE``)`\
`#> population    schools       jobs `\
`#>     812935        194     337921`

Each cell is a point at its centre. The bus feed of the sample only has
trips between late morning and early evening on weekdays, so public
transport is routed from 14:00 on a Monday, over a 30-minute window of
departures.

\
`departure`` ``<-`` `[`as.POSIXct`](https://rdrr.io/r/base/as.POSIXlt.html)`(``"2019-05-13 14:00:00"``, tz ``=`` ``"America/Sao_Paulo"``)`

## Travel times from one point

A first look: the travel time from one place, Farroupilha Park, to the
centre of every cell.
[`travel_time_matrix()`](../reference/travel_time_matrix.md) leaves out
the pairs that take longer than `max_trip_duration`.

\
`poi`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_points_of_interest.csv"``)``)`\
`park`` ``<-`` ``poi``[``poi``$``id`` ``==`` ``"farroupilha_park"``, ``]`\
\
`from_park`` ``<-`` `[`rbind`](https://rdrr.io/r/base/cbind.html)`(`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``mode ``=`` ``"Walking"``, `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(`\
`    ``network``, ``park``, ``grid``, mode ``=`` ``"WALK"``, max_trip_duration ``=`` ``60``)``)``,`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``mode ``=`` ``"Walking + public transport"``, `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(`\
`    ``network``, ``park``, ``grid``, mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`    departure_datetime ``=`` ``departure``, time_window ``=`` ``30``, max_trip_duration ``=`` ``60``)``)`\
`)`\
[`sapply`](https://rdrr.io/r/base/lapply.html)`(`[`split`](https://rdrr.io/r/base/split.html)`(``from_park``$``travel_time_p50``, ``from_park``$``mode``)``,`\
`       ``function``(``t``)`` `[`c`](https://rdrr.io/r/base/c.html)`(``cells ``=`` `[`length`](https://rdrr.io/r/base/length.html)`(``t``)``, median_minutes ``=`` `[`median`](https://rdrr.io/r/stats/median.html)`(``t``)``)``)`\
`#>                Walking Walking + public transport`\
`#> cells              268                       1100`\
`#> median_minutes      41                         39`

Within an hour, public transport reaches about four times as many cells
as walking does. On a map, with each cell drawn as a dot at its centre:

\
`cells`` ``<-`` `[`st_as_sf`](https://r-spatial.github.io/sf/reference/st_as_sf.html)`(``grid``, coords ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"lon"``, ``"lat"``)``, crs ``=`` ``4326``)`\
`map`` ``<-`` `[`merge`](https://rdrr.io/r/base/merge.html)`(``cells``, ``from_park``, by.x ``=`` ``"id"``, by.y ``=`` ``"to_id"``)`\
[`ggplot`](https://ggplot2.tidyverse.org/reference/ggplot.html)`(``map``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(`[`aes`](https://ggplot2.tidyverse.org/reference/aes.html)`(``colour ``=`` ``travel_time_p50``)``, size ``=`` ``1.2``)`` ``+`\
`  `[`scale_colour_gradient`](https://ggplot2.tidyverse.org/reference/scale_gradient.html)`(``low ``=`` ``"#7a3318"``, high ``=`` ``"#f5d3b6"``, name ``=`` ``"Minutes"``)`` ``+`\
`  `[`facet_wrap`](https://ggplot2.tidyverse.org/reference/facet_wrap.html)`(``~``mode``)`` ``+`\
`  `[`theme_void`](https://ggplot2.tidyverse.org/reference/ggtheme.html)`(``)`` ``+`\
`  `[`theme`](https://ggplot2.tidyverse.org/reference/theme.html)`(``strip.text ``=`` `[`element_text`](https://ggplot2.tidyverse.org/reference/element.html)`(``face ``=`` ``"bold"``, hjust ``=`` ``0``)``)`

![Two maps of Porto Alegre made of dots, one per grid cell reached
within an hour of Farroupilha Park, coloured by travel time: walking
reaches a compact patch around the park; with public transport, dots
cover most of the city, the closest ones along the bus
corridors.](accessibility-map_files/figure-html/ttm-map-1.png)

## Jobs and schools within 30 minutes

[`accessibility()`](../reference/accessibility.md) adds up, for each
origin, the opportunities at every destination reached within the cutoff
(the `"step"` decay function, the default). Five cells have no job
count; they count as zero, with a warning.

\
`access`` ``<-`` `[`rbind`](https://rdrr.io/r/base/cbind.html)`(`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``mode ``=`` ``"Walking"``, `[`accessibility`](../reference/accessibility.md)`(`\
`    ``network``, ``grid``, ``grid``, opportunities_colnames ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"jobs"``, ``"schools"``)``,`\
`    mode ``=`` ``"WALK"``, cutoffs ``=`` ``30``)``)``,`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``mode ``=`` ``"Walking + public transport"``, `[`accessibility`](../reference/accessibility.md)`(`\
`    ``network``, ``grid``, ``grid``, opportunities_colnames ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"jobs"``, ``"schools"``)``,`\
`    mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``, departure_datetime ``=`` ``departure``,`\
`    time_window ``=`` ``30``, cutoffs ``=`` ``30``)``)`\
`)`\
`` #> Warning: `jobs` has 5 missing values, treated as 0. ``\
`` #> Warning: `jobs` has 5 missing values, treated as 0. ``\
[`head`](https://rdrr.io/r/utils/head.html)`(``access``)`\
`#>      mode              id opportunity percentile cutoff accessibility`\
`#> 1 Walking 89a901291abffff        jobs         50     30         15978`\
`#> 2 Walking 89a901291abffff     schools         50     30            12`\
`#> 3 Walking 89a9012a3cfffff        jobs         50     30           641`\
`#> 4 Walking 89a9012a3cfffff     schools         50     30             0`\
`#> 5 Walking 89a901295b7ffff        jobs         50     30          1884`\
`#> 6 Walking 89a901295b7ffff     schools         50     30            14`

Weighting each cell by the people who live there gives what the average
resident reaches:

\
`access`` ``<-`` `[`merge`](https://rdrr.io/r/base/merge.html)`(``access``, ``grid``[``, `[`c`](https://rdrr.io/r/base/c.html)`(``"id"``, ``"population"``)``]``, by ``=`` ``"id"``)`\
[`sapply`](https://rdrr.io/r/base/lapply.html)`(`[`split`](https://rdrr.io/r/base/split.html)`(``access``, `[`list`](https://rdrr.io/r/base/list.html)`(``access``$``opportunity``, ``access``$``mode``)``)``, ``function``(``a``)`\
`  `[`round`](https://rdrr.io/r/base/Round.html)`(`[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``a``$``accessibility``, ``a``$``population``)``)``)`\
`#>                       jobs.Walking                    schools.Walking `\
`#>                              21343                                 12 `\
`#>    jobs.Walking + public transport schools.Walking + public transport `\
`#>                              67725                                 31`

Public transport roughly triples both: about 21 thousand jobs and 12
schools within half an hour on foot, against 68 thousand jobs and 31
schools with the buses and the train.

And the map of jobs reachable from each cell:

\
`jobs`` ``<-`` `[`merge`](https://rdrr.io/r/base/merge.html)`(``cells``, ``access``[``access``$``opportunity`` ``==`` ``"jobs"``, ``]``, by ``=`` ``"id"``)`\
[`ggplot`](https://ggplot2.tidyverse.org/reference/ggplot.html)`(``jobs``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(`[`aes`](https://ggplot2.tidyverse.org/reference/aes.html)`(``colour ``=`` ``accessibility`` ``/`` ``1000``)``, size ``=`` ``1.2``)`` ``+`\
`  `[`scale_colour_gradient`](https://ggplot2.tidyverse.org/reference/scale_gradient.html)`(``low ``=`` ``"#e3eadb"``, high ``=`` ``"#1f5139"``,`\
`                        name ``=`` ``"Thousand jobs"``)`` ``+`\
`  `[`facet_wrap`](https://ggplot2.tidyverse.org/reference/facet_wrap.html)`(``~``mode``)`` ``+`\
`  `[`theme_void`](https://ggplot2.tidyverse.org/reference/ggtheme.html)`(``)`` ``+`\
`  `[`theme`](https://ggplot2.tidyverse.org/reference/theme.html)`(``strip.text ``=`` `[`element_text`](https://ggplot2.tidyverse.org/reference/element.html)`(``face ``=`` ``"bold"``, hjust ``=`` ``0``)``)`

![Two maps of Porto Alegre made of dots, one per grid cell, coloured by
the number of jobs reachable within 30 minutes: walking, only the city
centre reaches more than 50 thousand; with public transport, the centre
and the corridors leading to it reach more than 100
thousand.](accessibility-map_files/figure-html/access-map-1.png)

## Variations

- **Gradual access.** `decay_function = "logistic"` (or `"exponential"`,
  `"linear"`, `"fixed_exponential"`) weights every job by its travel
  time instead of counting those under a hard limit; see the article
  [Accessibility](accessibility.md).
- **Several cutoffs at once.** `cutoffs = c(15, 30, 45)` returns one row
  per cutoff, from a single routing pass.
- **Percentiles.** With `percentiles = c(25, 50, 75)`, the accessibility
  of a good, a typical and a bad departure in the window.

The maps on the [home page](../../index.html#gallery) draw the same
results as hexagons, with `site/figures/figures.R` in the repository.
