# Example: isochrones by mode

An isochrone is the area within a given travel time of a point. This
example draws them for the four ways of travelling peabiru routes, from
one park in Porto Alegre, and shows how far each mode gets in 10, 20 and
30 minutes. Everything below is one script; the output shown is the
output it prints.

## The data

The example uses the Porto Alegre sample of r5r 2.5.0: an OpenStreetMap
extract of the city, the GTFS feeds of its buses (EPTC) and of its
metropolitan train (Trensurb), a hexagonal grid with population, jobs
and schools, and a few points of interest. The files are downloaded
once, into a temporary directory (about 2 MB).

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
`network`\
`#> <peabiru_network>`\
`#>   data path: /private/var/folders/j9/7g_srh2x0d71c5q0pbj5mxh40000gn/T/RtmpX2t3Tv/poa`\
`#>   streets:   26,880 vertices, 66,652 edges`\
`#>   transit:   2 feed(s) (poa_trensurb, poa_eptc), 4,010 stops, 226 patterns, 118 services`\
`#>   elevation: NONE`

The bus feed of the sample only has trips between late morning and early
evening on weekdays, so public transport is routed at 14:00 on a Monday.

\
`poi`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_points_of_interest.csv"``)``)`\
`park`` ``<-`` ``poi``[``poi``$``id`` ``==`` ``"farroupilha_park"``, ``]`\
`park`\
`#>                 id       lat       lon`\
`#> 8 farroupilha_park -30.03675 -51.21611`\
`departure`` ``<-`` `[`as.POSIXct`](https://rdrr.io/r/base/as.POSIXlt.html)`(``"2019-05-13 14:00:00"``, tz ``=`` ``"America/Sao_Paulo"``)`

## Isochrones, mode by mode

[`isochrone()`](../reference/isochrone.md) returns an `sf` data frame
with one polygon per origin and cutoff. The polygons are made from a
grid of travel times; `zoom` sets how fine that grid is (from 9 to 12;
at 11 a cell is about 65 m wide here). With public transport, the travel
time of each cell is the median over the departures of `time_window`,
here the 30 minutes from 14:00.

\
`modes`` ``<-`` `[`list`](https://rdrr.io/r/base/list.html)`(`\
`  ``"Walking"`` ``=`` ``"WALK"``,`\
`  ``"Cycling"`` ``=`` ``"BICYCLE"``,`\
`  ``"Driving"`` ``=`` ``"CAR"``,`\
`  ``"Walking + public transport"`` ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)`\
`)`\
`iso`` ``<-`` `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`names`](https://rdrr.io/r/base/names.html)`(``modes``)``, ``function``(``label``)`` ``{`\
`  ``i`` ``<-`` `[`isochrone`](../reference/isochrone.md)`(``network``, ``park``, mode ``=`` ``modes``[[``label``]``]``, cutoffs ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``10``, ``20``, ``30``)``,`\
`                 zoom ``=`` ``11``, departure_datetime ``=`` ``departure``, time_window ``=`` ``30``)`\
`  ``i``$``mode`` ``<-`` ``label`\
`  ``i`\
`}``)``)`\
`#> Warning: sf::st_make_valid() with spherical geometry (s2) changed the area of 1`\
`#> isochrone, of farroupilha_park (10 min). The polygons of that origin were`\
`#> repaired on the plane instead, which leaves the area as traced;`\
`#> options(peabiru.isochrone_repair = "spherical") keeps the s2 result.`\
`iso``$``mode`` ``<-`` `[`factor`](https://rdrr.io/r/base/factor.html)`(``iso``$``mode``, levels ``=`` `[`names`](https://rdrr.io/r/base/names.html)`(``modes``)``)`\
[`st_drop_geometry`](https://r-spatial.github.io/sf/reference/st_geometry.html)`(``iso``)`\
`#> ``# A tibble: 12 × 4`\
`#>    id               isochrone percentile mode                      `\
`#>  ``*`` ``<chr>``                ``<dbl>`` ``<chr>``      ``<fct>``                     `\
`#> `` 1`` farroupilha_park        30 p50        Walking                   `\
`#> `` 2`` farroupilha_park        20 p50        Walking                   `\
`#> `` 3`` farroupilha_park        10 p50        Walking                   `\
`#> `` 4`` farroupilha_park        30 p50        Cycling                   `\
`#> `` 5`` farroupilha_park        20 p50        Cycling                   `\
`#> `` 6`` farroupilha_park        10 p50        Cycling                   `\
`#> `` 7`` farroupilha_park        30 p50        Driving                   `\
`#> `` 8`` farroupilha_park        20 p50        Driving                   `\
`#> `` 9`` farroupilha_park        10 p50        Driving                   `\
`#> ``10`` farroupilha_park        30 p50        Walking + public transport`\
`#> ``11`` farroupilha_park        20 p50        Walking + public transport`\
`#> ``12`` farroupilha_park        10 p50        Walking + public transport`

The area of each isochrone, in square kilometres, sums up the picture:

\
`iso``$``area_km2`` ``<-`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`as.numeric`](https://rdrr.io/r/base/numeric.html)`(`[`st_area`](https://r-spatial.github.io/sf/reference/geos_measures.html)`(``iso``)``)`` ``/`` ``1e6``, ``1``)`\
[`xtabs`](https://rdrr.io/r/stats/xtabs.html)`(``area_km2`` ``~`` ``mode`` ``+`` ``isochrone``, data ``=`` `[`st_drop_geometry`](https://r-spatial.github.io/sf/reference/st_geometry.html)`(``iso``)``)`\
`#>                             isochrone`\
`#> mode                            10    20    30`\
`#>   Walking                      0.6   2.7   6.4`\
`#>   Cycling                      0.3   3.2  11.2`\
`#>   Driving                     15.1 103.0 142.9`\
`#>   Walking + public transport   0.6   4.5  20.4`

## A map

The main streets of the extract give the map some context; the
isochrones are drawn from the largest to the smallest, so that the
shorter times stay on top.

\
`streets`` ``<-`` `[`st_read`](https://r-spatial.github.io/sf/reference/st_read.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_osm.pbf"``)``, ``"lines"``, quiet ``=`` ``TRUE``)`\
`streets`` ``<-`` ``streets``[``streets``$``highway`` `[`%in%`](https://rdrr.io/r/base/match.html)` `[`c`](https://rdrr.io/r/base/c.html)`(``"motorway"``, ``"trunk"``, ``"primary"``,`\
`                                          ``"secondary"``, ``"tertiary"``)``, ``]`\
\
`iso`` ``<-`` ``iso``[`[`order`](https://rdrr.io/r/base/order.html)`(``-``iso``$``isochrone``)``, ``]`\
[`ggplot`](https://ggplot2.tidyverse.org/reference/ggplot.html)`(``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(``data ``=`` ``streets``, colour ``=`` ``"#dcd3c4"``, linewidth ``=`` ``0.2``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(``data ``=`` ``iso``, `[`aes`](https://ggplot2.tidyverse.org/reference/aes.html)`(``fill ``=`` `[`factor`](https://rdrr.io/r/base/factor.html)`(``isochrone``)``)``, colour ``=`` ``NA``, alpha ``=`` ``0.8``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(``data ``=`` `[`st_as_sf`](https://r-spatial.github.io/sf/reference/st_as_sf.html)`(``park``, coords ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"lon"``, ``"lat"``)``, crs ``=`` ``4326``)``,`\
`          shape ``=`` ``21``, size ``=`` ``2.5``, fill ``=`` ``"#1f2a2e"``, colour ``=`` ``"white"``)`` ``+`\
`  `[`scale_fill_manual`](https://ggplot2.tidyverse.org/reference/scale_manual.html)`(``values ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"10"`` ``=`` ``"#7a3318"``, ``"20"`` ``=`` ``"#cf7445"``, ``"30"`` ``=`` ``"#e8a57b"``)``,`\
`                    name ``=`` ``"Minutes"``)`` ``+`\
`  `[`coord_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(``xlim ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``-``51.25``, ``-``51.14``)``, ylim ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``-``30.11``, ``-``29.99``)``, datum ``=`` ``NA``)`` ``+`\
`  `[`facet_wrap`](https://ggplot2.tidyverse.org/reference/facet_wrap.html)`(``~``mode``)`` ``+`\
`  `[`theme_void`](https://ggplot2.tidyverse.org/reference/ggtheme.html)`(``)`` ``+`\
`  `[`theme`](https://ggplot2.tidyverse.org/reference/theme.html)`(``legend.position ``=`` ``"top"``, strip.text ``=`` `[`element_text`](https://ggplot2.tidyverse.org/reference/element.html)`(``face ``=`` ``"bold"``, hjust ``=`` ``0``)``)`

![Four maps of Porto Alegre, one per mode, with the areas reached from
Farroupilha Park within 10, 20 and 30 minutes: a small area walking, a
few kilometres around the park cycling, most of the map driving, and
long arms along the bus corridors with public
transport.](isochrones_files/figure-html/map-1.png)

Walking covers a couple of kilometres around the park in half an hour.
Cycling covers no more than walking in the first ten minutes from this
park, then pulls ahead. Driving reaches most of the map within 20
minutes. Public transport sits in between, and its isochrones follow the
bus corridors leaving the city centre: a stop within a short walk of the
park and a fast ride extend the area far along those lines, and nowhere
else.

## People within reach

Polygons answer “where”; a matrix to the hexagonal grid of the sample,
which carries the population of each cell, answers “how many”. The share
of the city’s population reached from the park, by mode and travel time:

\
`grid`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_hexgrid.csv"``)``)`\
`reach`` ``<-`` `[`sapply`](https://rdrr.io/r/base/lapply.html)`(``modes``, ``function``(``mode``)`` ``{`\
`  ``ttm`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``network``, ``park``, ``grid``, mode ``=`` ``mode``,`\
`                            departure_datetime ``=`` ``departure``, time_window ``=`` ``30``,`\
`                            max_trip_duration ``=`` ``60``)`\
`  ``people`` ``<-`` ``grid``$``population``[`[`match`](https://rdrr.io/r/base/match.html)`(``ttm``$``to_id``, ``grid``$``id``)``]`\
`  `[`sapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``15``, ``30``, ``45``, ``60``)``, ``function``(``t``)`\
`    `[`round`](https://rdrr.io/r/base/Round.html)`(``100`` ``*`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``people``[``ttm``$``travel_time_p50`` ``<=`` ``t``]``)`` ``/`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``grid``$``population``)``)``)`\
`}``)`\
[`rownames`](https://rdrr.io/r/base/colnames.html)`(``reach``)`` ``<-`` `[`paste`](https://rdrr.io/r/base/paste.html)`(``"within"``, `[`c`](https://rdrr.io/r/base/c.html)`(``15``, ``30``, ``45``, ``60``)``, ``"min (%)"``)`\
`reach`\
`#>                   Walking Cycling Driving Walking + public transport`\
`#> within 15 min (%)       2       4      73                          2`\
`#> within 30 min (%)      12      17     100                         30`\
`#> within 45 min (%)      22      44     100                         79`\
`#> within 60 min (%)      32      69     100                         98`

![Step curves of the share of the population of Porto Alegre reached
from Farroupilha Park within 0 to 60 minutes, by mode: driving reaches
nearly everyone within 20 minutes, walking plus public transport most
people within an hour, cycling about two thirds and walking about a
third.](figures/reach-by-mode.svg)

## Variations

- **Several origins.** Give [`isochrone()`](../reference/isochrone.md)
  more than one point; the `id` column tells the polygons apart.
- **Streets instead of areas.** `polygon_output = FALSE` returns the
  street segments reached, each with its travel time, instead of
  polygons.
- **Traffic stress.** Cycling uses the streets whose level of traffic
  stress is at most `max_lts` (2 by default) and walks the bike on the
  others. With `max_lts = 4`, the 30-minute cycling isochrone from the
  park grows from 11.2 to 34.2 km².
- **Uncertainty.** With `percentiles = c(25, 50, 75)`, public transport
  isochrones come in one set per percentile: the area reached on a
  lucky, a typical and an unlucky departure.

The figure on the [home page](../../index.html#gallery) draws the same
isochrones, styled for the documentation, with `site/figures/figures.R`
in the repository.
