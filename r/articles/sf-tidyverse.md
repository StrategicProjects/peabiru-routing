# peabiru with sf and the tidyverse

peabiru returns tibbles, and `sf` tibbles where the result has a
geometry, so its results go straight into ‘dplyr’ and ‘ggplot2’. This
article walks from a set of points to maps of travel times,
accessibility, isochrones and trips, on the São Paulo sample network
shipped with the package (the streets around Avenida Paulista and the
SPTrans GTFS feed), so that it runs offline in a few seconds.

\
[`library`](https://rdrr.io/r/base/library.html)`(`[`peabiru`](https://strategicprojects.github.io/peabiru-routing/r/)`)`\
[`library`](https://rdrr.io/r/base/library.html)`(`[`sf`](https://r-spatial.github.io/sf/)`)`\
[`library`](https://rdrr.io/r/base/library.html)`(`[`dplyr`](https://dplyr.tidyverse.org)`)`\
[`library`](https://rdrr.io/r/base/library.html)`(`[`ggplot2`](https://ggplot2.tidyverse.org)`)`\
\
`data_path`` ``<-`` `[`system.file`](https://rdrr.io/r/base/system.file.html)`(``"extdata/spo"``, package ``=`` ``"peabiru"``)`\
`network`` ``<-`` `[`build_network`](../reference/build_network.md)`(``data_path``, n_threads ``=`` ``2``)`\
`departure`` ``<-`` `[`as.POSIXct`](https://rdrr.io/r/base/as.POSIXlt.html)`(``"2019-05-13 14:00:00"``, tz ``=`` ``"America/Sao_Paulo"``)`

## Points and a grid as sf

The sample has 27 points with their population and the jobs, schools and
health care in each. As an `sf` tibble they keep those columns next to
the geometry:

\
`points`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_path``, ``"spo_points.csv"``)``)`` ``|>`\
`  `[`as_tibble`](https://tibble.tidyverse.org/reference/as_tibble.html)`(``)`` ``|>`\
`  `[`st_as_sf`](https://r-spatial.github.io/sf/reference/st_as_sf.html)`(``coords ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"lon"``, ``"lat"``)``, crs ``=`` ``4326``)`\
`points`\
`#> Simple feature collection with 27 features and 5 fields`\
`#> Geometry type: POINT`\
`#> Dimension:     XY`\
`#> Bounding box:  xmin: -46.64119 ymin: -23.5577 xmax: -46.6286 ymax: -23.54209`\
`#> Geodetic CRS:  WGS 84`\
`#> ``# A tibble: 27 × 6`\
`#>    id              population schools  jobs healthcare              geometry`\
`#>  ``*`` ``<chr>``                ``<int>``   ``<int>`` ``<int>``      ``<int>``           ``<POINT [°]>`\
`#> `` 1`` 89a8100c00fffff       ``3``648       0   153          0 (-46.62896 -23.55631)`\
`#> `` 2`` 89a8100c007ffff       ``3``936       0   551          0  (-46.63207 -23.5577)`\
`#> `` 3`` 89a8100c077ffff       ``6``514       1  ``1``124          0 (-46.62887 -23.55325)`\
`#> `` 4`` 89a8100c03bffff       ``3``670       0  ``7``112          0 (-46.63198 -23.55464)`\
`#> `` 5`` 89a8100c033ffff        893       0  ``3``298          0 (-46.63508 -23.55603)`\
`#> `` 6`` 89a8100c0afffff       ``2``185       0  ``1``098          0 (-46.63818 -23.55742)`\
`#> `` 7`` 89a8100c067ffff       ``1``337       0   214          3 (-46.62878 -23.55018)`\
`#> `` 8`` 89a8100c02bffff        177       0  ``1``696          0 (-46.63189 -23.55157)`\
`#> `` 9`` 89a8100c023ffff        652       0  ``2``800          0 (-46.63499 -23.55296)`\
`#> ``10`` 89a8100c037ffff       ``2``532       0  ``1``471          0 (-46.63809 -23.55435)`\
`#> ``# ℹ 17 more rows`

A hexagonal grid over the network gives the areas to map. Grids are
drawn in metres, so this one is made in SIRGAS 2000 / UTM zone 23S
(EPSG:31983), the projected system of São Paulo, and kept there: peabiru
takes `sf` points in any coordinate reference system and reprojects them
to WGS 84 itself.

\
`area`` ``<-`` `[`street_network_bbox`](../reference/street_network_bbox.md)`(``network``)`` ``|>`` `[`st_transform`](https://r-spatial.github.io/sf/reference/st_transform.html)`(``31983``)`\
`hexagons`` ``<-`` `[`st_make_grid`](https://r-spatial.github.io/sf/reference/st_make_grid.html)`(``area``, cellsize ``=`` ``300``, square ``=`` ``FALSE``)`` ``|>`\
`  `[`st_as_sf`](https://r-spatial.github.io/sf/reference/st_as_sf.html)`(``)`` ``|>`\
`  `[`st_set_geometry`](https://r-spatial.github.io/sf/reference/st_geometry.html)`(``"geometry"``)`` ``|>`\
`  `[`mutate`](https://dplyr.tidyverse.org/reference/mutate.html)`(``id ``=`` `[`sprintf`](https://rdrr.io/r/base/sprintf.html)`(``"h%03d"``, `[`row_number`](https://dplyr.tidyverse.org/reference/row_number.html)`(``)``)``, .before ``=`` ``1``)`\
`# the attributes (the id) hold for the whole hexagon, so for its centre too`\
`centres`` ``<-`` ``hexagons`` ``|>`` `[`st_set_agr`](https://r-spatial.github.io/sf/reference/st_agr.html)`(``"constant"``)`` ``|>`` `[`st_centroid`](https://r-spatial.github.io/sf/reference/geos_unary.html)`(``)`\
[`nrow`](https://rdrr.io/r/base/nrow.html)`(``hexagons``)`\
`#> [1] 196`

## A travel-time matrix with the pipe

The network is the first argument of every routing function, so a call
reads from left to right with the native pipe. From the centre of every
hexagon to the 27 points, walking and by public transport:

\
`ttm`` ``<-`` ``network`` ``|>`\
`  `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``origins ``=`` ``centres``, destinations ``=`` ``points``,`\
`                     mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``, departure_datetime ``=`` ``departure``,`\
`                     max_trip_duration ``=`` ``60``, seed ``=`` ``42``, n_threads ``=`` ``2``)`\
`#> 'origins' reprojected from SIRGAS 2000 / UTM zone 23S (EPSG:31983) to WGS 84 (EPSG:4326).`\
`ttm`\
`#> ``# A tibble: 5,204 × 3`\
`#>    from_id to_id           travel_time_p50`\
`#>    ``<chr>``   ``<chr>``                     ``<int>`\
`#> `` 1`` h001    89a8100c00fffff              45`\
`#> `` 2`` h001    89a8100c007ffff              40`\
`#> `` 3`` h001    89a8100c077ffff              50`\
`#> `` 4`` h001    89a8100c03bffff              43`\
`#> `` 5`` h001    89a8100c033ffff              37`\
`#> `` 6`` h001    89a8100c0afffff              28`\
`#> `` 7`` h001    89a8100c067ffff              48`\
`#> `` 8`` h001    89a8100c02bffff              40`\
`#> `` 9`` h001    89a8100c023ffff              38`\
`#> ``10`` h001    89a8100c037ffff              29`\
`#> ``# ℹ 5,194 more rows`

The centres are in UTM, so peabiru says it reprojected them (the points
were already in WGS 84). The message has its own class,
`peabiru_reprojected`;
[`suppressMessages()`](https://rdrr.io/r/base/message.html) silences it.

## dplyr on the result

The result is a tibble with one row per pair, ready for dplyr verbs. For
each hexagon, the median time to the 27 points and how many of them are
within 20 minutes:

\
`by_hexagon`` ``<-`` ``ttm`` ``|>`\
`  `[`group_by`](https://dplyr.tidyverse.org/reference/group_by.html)`(``id ``=`` ``from_id``)`` ``|>`\
`  `[`summarise`](https://dplyr.tidyverse.org/reference/summarise.html)`(``median_time ``=`` `[`median`](https://rdrr.io/r/stats/median.html)`(``travel_time_p50``)``,`\
`            within_20 ``=`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``travel_time_p50`` ``<=`` ``20``)``,`\
`            .groups ``=`` ``"drop"``)`\
`by_hexagon`` ``|>`` `[`arrange`](https://dplyr.tidyverse.org/reference/arrange.html)`(``median_time``)`\
`#> ``# A tibble: 196 × 3`\
`#>    id    median_time within_20`\
`#>    ``<chr>``       ``<dbl>``     ``<int>`\
`#> `` 1`` h060           14        22`\
`#> `` 2`` h067           14        23`\
`#> `` 3`` h081           14        26`\
`#> `` 4`` h066           15        25`\
`#> `` 5`` h068           15        19`\
`#> `` 6`` h074           15        24`\
`#> `` 7`` h075           15        23`\
`#> `` 8`` h046           16        20`\
`#> `` 9`` h054           16        20`\
`#> ``10`` h069           16        18`\
`#> ``# ℹ 186 more rows`

## Back to the geometry

To map the summary, join it to the hexagons. **Put the `sf` object on
the left**:
[`left_join()`](https://dplyr.tidyverse.org/reference/mutate-joins.html)
keeps the class of its first argument, so
`hexagons |> left_join(by_hexagon)` is an `sf` with the new columns,
while `by_hexagon |> left_join(hexagons)` is a plain tibble that carries
the geometry as a column without being `sf`
([`st_as_sf()`](https://r-spatial.github.io/sf/reference/st_as_sf.html)
would turn it back).

\
`mapped`` ``<-`` ``hexagons`` ``|>`` `[`left_join`](https://dplyr.tidyverse.org/reference/mutate-joins.html)`(``by_hexagon``, by ``=`` ``"id"``)`\
[`class`](https://rdrr.io/r/base/class.html)`(``mapped``)`\
`#> [1] "sf"         "data.frame"`\
[`class`](https://rdrr.io/r/base/class.html)`(``by_hexagon`` ``|>`` `[`left_join`](https://dplyr.tidyverse.org/reference/mutate-joins.html)`(``hexagons``, by ``=`` ``"id"``)``)`\
`#> [1] "tbl_df"     "tbl"        "data.frame"`

A hexagon whose centre reached none of the points within the hour would
have no row in the matrix; the join would leave it `NA`, and
[`geom_sf()`](https://ggplot2.tidyverse.org/reference/ggsf.html) would
draw it grey.

\
`streets`` ``<-`` `[`street_network_to_sf`](../reference/street_network_to_sf.md)`(``network``)``$``edges`` ``|>`\
`  `[`filter`](https://dplyr.tidyverse.org/reference/filter.html)`(``street_class`` `[`%in%`](https://rdrr.io/r/base/match.html)` `[`c`](https://rdrr.io/r/base/c.html)`(``"PRIMARY"``, ``"SECONDARY"``)``)`\
\
[`ggplot`](https://ggplot2.tidyverse.org/reference/ggplot.html)`(``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(``data ``=`` ``mapped``, `[`aes`](https://ggplot2.tidyverse.org/reference/aes.html)`(``fill ``=`` ``median_time``)``, colour ``=`` ``"white"``, linewidth ``=`` ``0.2``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(``data ``=`` ``streets``, colour ``=`` ``"#4a5a5f"``, linewidth ``=`` ``0.25``, alpha ``=`` ``0.6``)`` ``+`\
`  `[`scale_fill_gradient`](https://ggplot2.tidyverse.org/reference/scale_gradient.html)`(``low ``=`` ``"#7a3318"``, high ``=`` ``"#f5d3b6"``, na.value ``=`` ``"grey85"``,`\
`                      name ``=`` ``"Median\nminutes"``)`` ``+`\
`  `[`theme_void`](https://ggplot2.tidyverse.org/reference/ggtheme.html)`(``)`

![A map of hexagons over central São Paulo, with the main streets,
coloured by the median travel time walking and by public transport from
each hexagon to the 27 sample points: about 15 minutes in the middle and
to the west, where the points are, rising to about 50 minutes at the
eastern edge.](sf-tidyverse_files/figure-html/median-map-1.png)

## Accessibility as a choropleth

[`accessibility()`](../reference/accessibility.md) counts, for each
origin, the opportunities within the cutoff. The destinations carry the
opportunity columns; the origins are the hexagon centres again:

\
`jobs`` ``<-`` ``network`` ``|>`\
`  `[`accessibility`](../reference/accessibility.md)`(``origins ``=`` ``centres``, destinations ``=`` ``points``,`\
`                opportunities_colnames ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"jobs"``, ``"schools"``)``,`\
`                mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``, departure_datetime ``=`` ``departure``,`\
`                cutoffs ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``15``, ``30``)``, seed ``=`` ``42``, n_threads ``=`` ``2``)`` ``|>`\
`  `[`suppressMessages`](https://rdrr.io/r/base/message.html)`(``)`\
`jobs`` ``|>`\
`  `[`group_by`](https://dplyr.tidyverse.org/reference/group_by.html)`(``opportunity``, ``cutoff``)`` ``|>`\
`  `[`summarise`](https://dplyr.tidyverse.org/reference/summarise.html)`(``mean ``=`` `[`mean`](https://rdrr.io/r/base/mean.html)`(``accessibility``)``, max ``=`` `[`max`](https://rdrr.io/r/base/Extremes.html)`(``accessibility``)``, .groups ``=`` ``"drop"``)`\
`#> ``# A tibble: 4 × 4`\
`#>   opportunity cutoff      mean    max`\
`#>   ``<chr>``        ``<int>``     ``<dbl>``  ``<dbl>`\
`#> ``1`` jobs            15 ``11``045.     ``85``849`\
`#> ``2`` jobs            30 ``52``623.    ``110``736`\
`#> ``3`` schools         15     0.541      4`\
`#> ``4`` schools         30     2.78       7`

One cutoff and one opportunity per map, or
[`facet_wrap()`](https://ggplot2.tidyverse.org/reference/facet_wrap.html)
over the two cutoffs after the join:

\
`hexagons`` ``|>`\
`  `[`inner_join`](https://dplyr.tidyverse.org/reference/mutate-joins.html)`(`[`filter`](https://dplyr.tidyverse.org/reference/filter.html)`(``jobs``, ``opportunity`` ``==`` ``"jobs"``)``, by ``=`` ``"id"``)`` ``|>`\
`  `[`mutate`](https://dplyr.tidyverse.org/reference/mutate.html)`(``cutoff ``=`` `[`paste`](https://rdrr.io/r/base/paste.html)`(``cutoff``, ``"minutes"``)``)`` ``|>`\
`  `[`ggplot`](https://ggplot2.tidyverse.org/reference/ggplot.html)`(``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(`[`aes`](https://ggplot2.tidyverse.org/reference/aes.html)`(``fill ``=`` ``accessibility`` ``/`` ``1000``)``, colour ``=`` ``"white"``, linewidth ``=`` ``0.2``)`` ``+`\
`  `[`scale_fill_gradient`](https://ggplot2.tidyverse.org/reference/scale_gradient.html)`(``low ``=`` ``"#e8efe9"``, high ``=`` ``"#2f6b4f"``, name ``=`` ``"Jobs\n(thousands)"``)`` ``+`\
`  `[`facet_wrap`](https://ggplot2.tidyverse.org/reference/facet_wrap.html)`(``~``cutoff``)`` ``+`\
`  `[`theme_void`](https://ggplot2.tidyverse.org/reference/ggtheme.html)`(``)`` ``+`\
`  `[`theme`](https://ggplot2.tidyverse.org/reference/theme.html)`(``strip.text ``=`` `[`element_text`](https://ggplot2.tidyverse.org/reference/element.html)`(``face ``=`` ``"bold"``, hjust ``=`` ``0``)``)`

![Two maps of hexagons, for 15 and 30 minutes, coloured by the number of
jobs reachable walking and by public transport: within 15 minutes a few
hexagons west of the centre reach more than 50 thousand jobs; within 30
minutes most of the western two thirds reach about 100 thousand, and the
eastern edge few.](sf-tidyverse_files/figure-html/access-map-1.png)

[`inner_join()`](https://dplyr.tidyverse.org/reference/mutate-joins.html)
keeps only the hexagons with a value, and the result is still `sf`,
because the `sf` object is on the left.

## Isochrones and itineraries as layers

[`isochrone()`](../reference/isochrone.md),
[`detailed_itineraries()`](../reference/detailed_itineraries.md) and
[`trip_options()`](../reference/trip_options.md) return `sf` tibbles in
WGS 84 that
[`geom_sf()`](https://ggplot2.tidyverse.org/reference/ggsf.html) draws
as they are. ggplot2 puts every layer in the coordinate system of the
first, so the UTM hexagons and the WGS 84 results share one map.

\
`origin`` ``<-`` ``points`` ``|>`` `[`filter`](https://dplyr.tidyverse.org/reference/filter.html)`(``id`` ``==`` ``"89a8100c037ffff"``)`\
`destination`` ``<-`` ``points`` ``|>`` `[`filter`](https://dplyr.tidyverse.org/reference/filter.html)`(``id`` ``==`` ``"89a8100c39bffff"``)`\
\
`reach`` ``<-`` ``network`` ``|>`\
`  `[`isochrone`](../reference/isochrone.md)`(``origins ``=`` ``origin``, mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``, cutoffs ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``0``, ``10``, ``20``, ``30``)``,`\
`            departure_datetime ``=`` ``departure``, seed ``=`` ``42``, n_threads ``=`` ``2``)`\
`reach`\
`#> Simple feature collection with 3 features and 3 fields`\
`#> Geometry type: POLYGON`\
`#> Dimension:     XY`\
`#> Bounding box:  xmin: -46.64726 ymin: -23.56588 xmax: -46.62186 ymax: -23.53692`\
`#> Geodetic CRS:  WGS 84`\
`#> ``# A tibble: 3 × 4`\
`#>   id              isochrone percentile                                  polygons`\
`#>   ``<chr>``               ``<dbl>`` ``<chr>``                                  ``<POLYGON [°]>`\
`#> ``1`` 89a8100c037ffff        30 p50        ((-46.63322 -23.56462, -46.63301 -23.563…`\
`#> ``2`` 89a8100c037ffff        20 p50        ((-46.63902 -23.5621, -46.63765 -23.5616…`\
`#> ``3`` 89a8100c037ffff        10 p50        ((-46.63902 -23.55706, -46.63765 -23.556…`\
\
`cycling`` ``<-`` ``network`` ``|>`\
`  `[`detailed_itineraries`](../reference/detailed_itineraries.md)`(``origins ``=`` ``origin``, destinations ``=`` ``destination``, mode ``=`` ``"BICYCLE"``,`\
`                       departure_datetime ``=`` ``departure``, n_threads ``=`` ``2``)`\
`cycling`` ``|>`` `[`select`](https://dplyr.tidyverse.org/reference/select.html)`(``mode``, ``segment_duration``, ``distance``)`\
`#> Simple feature collection with 1 feature and 3 fields`\
`#> Geometry type: LINESTRING`\
`#> Dimension:     XY`\
`#> Bounding box:  xmin: -46.63834 ymin: -23.55562 xmax: -46.62929 ymax: -23.54593`\
`#> Geodetic CRS:  WGS 84`\
`#> ``# A tibble: 1 × 4`\
`#>   mode    segment_duration distance                                     geometry`\
`#>   ``<chr>``              ``<dbl>``    ``<int>``                             ``<LINESTRING [°]>`\
`#> ``1`` BICYCLE             19.7     ``1``899 (-46.63834 -23.55562, -46.63763 -23.55469, …`

The bundled feed is frequency-based, so
[`detailed_itineraries()`](../reference/detailed_itineraries.md) routes
street modes only; the public transport trip comes from
[`trip_options()`](../reference/trip_options.md) below.

## trip_options() with dplyr

[`trip_options()`](../reference/trip_options.md) returns a list of
tables: `options` (one row per trip worth considering), `itineraries`
(an `sf` of their legs) and, with `by_departure = TRUE`, `departures`.
Its options for the same pair:

\
`trips`` ``<-`` ``network`` ``|>`\
`  `[`trip_options`](../reference/trip_options.md)`(``origins ``=`` ``origin``, destinations ``=`` ``destination``,`\
`               departure_datetime ``=`` ``departure``, n_threads ``=`` ``2``)`\
`trips``$``options`` ``|>`` `[`select`](https://dplyr.tidyverse.org/reference/select.html)`(``option``, ``total_time``, ``transfers``, ``walk_time``, ``description``)`\
`#> ``# A tibble: 4 × 5`\
`#>   option total_time transfers walk_time description                             `\
`#>    ``<int>``      ``<dbl>``     ``<int>``     ``<dbl>`` ``<chr>``                                   `\
`#> ``1``      1       31.8         0      26.1 WALK > SUBWAY METRÔ L3 > WALK           `\
`#> ``2``      2       34.3         1      22.4 WALK > BUS 2002-10 > SUBWAY METRÔ L3 > …`\
`#> ``3``      3       34.7         0      24.2 WALK > BUS 2002-10 > WALK               `\
`#> ``4``      4       34.9         0      19.7 WALK > BUS 2002-10 > WALK`

The legs summarise with dplyr like any table;
[`st_drop_geometry()`](https://r-spatial.github.io/sf/reference/st_geometry.html)
first when the lines are not needed, which keeps the summary light:

\
`trips``$``itineraries`` ``|>`\
`  `[`st_drop_geometry`](https://r-spatial.github.io/sf/reference/st_geometry.html)`(``)`` ``|>`\
`  `[`group_by`](https://dplyr.tidyverse.org/reference/group_by.html)`(``option``)`` ``|>`\
`  `[`summarise`](https://dplyr.tidyverse.org/reference/summarise.html)`(``legs ``=`` `[`n`](https://dplyr.tidyverse.org/reference/context.html)`(``)``, riding ``=`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``segment_duration``[``mode`` ``!=`` ``"WALK"``]``)``,`\
`            walking ``=`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``segment_duration``[``mode`` ``==`` ``"WALK"``]``)``, metres ``=`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``distance``)``)`\
`#> ``# A tibble: 4 × 5`\
`#>   option  legs riding walking metres`\
`#>    ``<int>`` ``<int>``  ``<dbl>``   ``<dbl>``  ``<int>`\
`#> ``1``      1     3    3.2    26.2   ``2``464`\
`#> ``2``      2     5    5.4    22.5   ``2``945`\
`#> ``3``      3     3    6.5    24.2   ``2``639`\
`#> ``4``      4     3   11.2    19.8   ``2``657`

On one map: the isochrones, the cycling route and the fastest public
transport option, its legs coloured by mode.

\
`fastest`` ``<-`` ``trips``$``itineraries`` ``|>`` `[`filter`](https://dplyr.tidyverse.org/reference/filter.html)`(``option`` ``==`` ``1``)`\
\
[`ggplot`](https://ggplot2.tidyverse.org/reference/ggplot.html)`(``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(``data ``=`` ``hexagons``, fill ``=`` ``NA``, colour ``=`` ``"grey80"``, linewidth ``=`` ``0.2``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(``data ``=`` ``reach`` ``|>`` `[`arrange`](https://dplyr.tidyverse.org/reference/arrange.html)`(`[`desc`](https://dplyr.tidyverse.org/reference/desc.html)`(``isochrone``)``)``, `[`aes`](https://ggplot2.tidyverse.org/reference/aes.html)`(``fill ``=`` `[`factor`](https://rdrr.io/r/base/factor.html)`(``isochrone``)``)``,`\
`          colour ``=`` ``NA``, alpha ``=`` ``0.75``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(``data ``=`` ``cycling``, colour ``=`` ``"#4a5a5f"``, linewidth ``=`` ``0.9``, linetype ``=`` ``"22"``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(``data ``=`` ``fastest``, `[`aes`](https://ggplot2.tidyverse.org/reference/aes.html)`(``colour ``=`` ``mode``)``, linewidth ``=`` ``1.4``)`` ``+`\
`  `[`geom_sf`](https://ggplot2.tidyverse.org/reference/ggsf.html)`(``data ``=`` `[`bind_rows`](https://dplyr.tidyverse.org/reference/bind_rows.html)`(``origin``, ``destination``)``, size ``=`` ``2.5``, shape ``=`` ``21``,`\
`          fill ``=`` ``"white"``, colour ``=`` ``"black"``)`` ``+`\
`  `[`scale_fill_manual`](https://ggplot2.tidyverse.org/reference/scale_manual.html)`(``values ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``` `10`  ```=`` ``"#a2502a"``` , `20`  ```=`` ``"#d46f3d"``` , `30`  ```=`` ``"#f5d3b6"``)``,`\
`                    name ``=`` ``"Within\n(minutes)"``)`` ``+`\
`  `[`scale_colour_manual`](https://ggplot2.tidyverse.org/reference/scale_manual.html)`(``values ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``WALK ``=`` ``"#c98a1d"``, SUBWAY ``=`` ``"#2f6b4f"``, BUS ``=`` ``"#2f6b4f"``,`\
`                                 RAIL ``=`` ``"#2f6b4f"``)``,`\
`                      name ``=`` ``"Fastest by\npublic transport"``)`` ``+`\
`  `[`theme_void`](https://ggplot2.tidyverse.org/reference/ggtheme.html)`(``)`` ``+`\
`  `[`theme`](https://ggplot2.tidyverse.org/reference/theme.html)`(``legend.position ``=`` ``"bottom"``, legend.box ``=`` ``"vertical"``,`\
`        plot.margin ``=`` `[`margin`](https://ggplot2.tidyverse.org/reference/element.html)`(``5``, ``5``, ``12``, ``5``)``)`

![A map over the hexagon grid with three nested areas reached from one
point within 10, 20 and 30 minutes walking and by public transport, the
dashed cycling route to a point about two kilometres to the north-east,
and the fastest public transport option to it: a walk, a metro ride and
another walk.](sf-tidyverse_files/figure-html/layers-map-1.png)

## Choosing the class of the results

Every function returns tibbles because `options(peabiru.output = )` is
`"tibble"` by default. It is read at each call; set it once at the top
of a script:

- `"data.frame"` gives base data frames (and plain `sf`), for code
  written for them;
- `"data.table"` gives `data.table`s (`sf` `data.table`s for isochrone
  polygons and itineraries): the better choice for large matrices, where
  data.table’s grouping by reference saves time and memory.

\
[`library`](https://rdrr.io/r/base/library.html)`(`[`data.table`](https://r-datatable.com)`)`\
[`options`](https://rdrr.io/r/base/options.html)`(``peabiru.output ``=`` ``"data.table"``)`\
`ttm_dt`` ``<-`` ``network`` ``|>`\
`  `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``origins ``=`` ``centres``, destinations ``=`` ``points``,`\
`                     mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``, departure_datetime ``=`` ``departure``,`\
`                     max_trip_duration ``=`` ``60``, seed ``=`` ``42``, n_threads ``=`` ``2``)`` ``|>`\
`  `[`suppressMessages`](https://rdrr.io/r/base/message.html)`(``)`\
[`class`](https://rdrr.io/r/base/class.html)`(``ttm_dt``)`\
`#> [1] "data.table" "data.frame"`\
`ttm_dt``[``, ``.``(``median_time ``=`` `[`median`](https://rdrr.io/r/stats/median.html)`(``travel_time_p50``)``)``, by ``=`` ``.``(``id ``=`` ``from_id``)``]``[`[`order`](https://rdrr.io/r/base/order.html)`(``median_time``)``]``[``1``:``3``]`\
`#>        id median_time`\
`#>    <char>       <num>`\
`#> 1:   h060          14`\
`#> 2:   h067          14`\
`#> 3:   h081          14`\
[`options`](https://rdrr.io/r/base/options.html)`(``peabiru.output ``=`` ``NULL``)`` ``# back to the default, tibbles`

The rows, columns, types and values are the same whatever the class: for
a table without geometry,
[`as.data.frame()`](https://rdrr.io/r/base/as.data.frame.html) of any of
the three gives the same data frame.
