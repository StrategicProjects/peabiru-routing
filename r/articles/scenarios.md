# Scenarios: car speeds and traffic stress

Every routing function takes three arguments that change the street
network for that call only: `new_carspeeds` (car speeds by
‘OpenStreetMap’ way, or by area), `carspeed_scale` (a factor for every
other street) and `new_lts` (levels of traffic stress for cyclists, by
way or by line). The network itself does not change. This page uses the
Porto Alegre sample of r5r 2.5.0 and the scenario tables that come with
it.

\
[`library`](https://rdrr.io/r/base/library.html)`(`[`peabiru`](https://strategicprojects.github.io/peabiru-routing/r/)`)`\
`poa`` ``<-`` ``poa_sample``(`[`c`](https://rdrr.io/r/base/c.html)`(``"poa_osm.pbf"``, ``"poa_hexgrid.csv"``, ``"poa_osm_congestion.csv"``,`\
`                    ``"poa_poly_congestion.rds"``, ``"poa_osm_lts.csv"``, ``"poa_ls_lts.rds"``)``)`\
`poa_network`` ``<-`` `[`build_network`](../reference/build_network.md)`(``poa``, n_threads ``=`` ``2``)`\
`hexes`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_hexgrid.csv"``)``)`\
`pts`` ``<-`` ``hexes``[`[`seq`](https://rdrr.io/r/base/seq.html)`(``1``, `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``hexes``)``, by ``=`` ``25``)``, ``]`\
\
`car`` ``<-`` ``function``(``...``)`` ``{`\
`  `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``poa_network``, ``pts``, ``pts``, mode ``=`` ``"CAR"``, n_threads ``=`` ``2``, ``...``)`\
`}`\
`base`` ``<-`` ``car``(``)`\
[`mean`](https://rdrr.io/r/base/mean.html)`(``base``$``travel_time_p50``)`\
`#> [1] 13.9768`

## Car speeds by way

A data frame with `osm_id`, `max_speed` and `speed_type` gives, for each
way, a factor of its car speed (`"scale"`) or a new speed (`"km/h"`); a
speed of 0 closes the way to cars.

\
`congestion`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_osm_congestion.csv"``)``)`\
[`head`](https://rdrr.io/r/utils/head.html)`(``congestion``)`\
`#>      osm_id max_speed speed_type`\
`#> 1  27184648       0.5      scale`\
`#> 2 762361901       0.5      scale`\
`#> 3 568609955       0.5      scale`\
`#> 4 709834913       0.5      scale`\
`#> 5 709834914       0.5      scale`\
`#> 6  77705540       0.5      scale`\
`slow`` ``<-`` ``car``(``new_carspeeds ``=`` ``congestion``)`\
[`mean`](https://rdrr.io/r/base/mean.html)`(``slow``$``travel_time_p50``)`\
`#> [1] 13.9836`

`carspeed_scale` multiplies the car speed of every street the table
leaves out (all of them without a table):

\
`rush_hour`` ``<-`` ``car``(``carspeed_scale ``=`` ``0.7``)`\
[`mean`](https://rdrr.io/r/base/mean.html)`(``rush_hour``$``travel_time_p50``)`\
`#> [1] 18.2156`

Car speeds are kept as 16-bit centimetres per second, so a factor or a
speed that would bring a speed to 0 or less, or beyond that range, is an
error rather than a wrapped-around speed.

## Car speeds by area

An `sf` data frame of polygons with `poly_id`, `scale` and `priority`
gives each street the factor of the polygon it meets with the highest
priority (ties: the one it overlaps most).

\
`areas`` ``<-`` `[`readRDS`](https://rdrr.io/r/base/readRDS.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_poly_congestion.rds"``)``)`\
`areas`\
`#> Simple feature collection with 2 features and 3 fields`\
`#> Geometry type: POLYGON`\
`#> Dimension:     XY`\
`#> Bounding box:  xmin: -51.24537 ymin: -30.04352 xmax: -51.21426 ymax: -30.02548`\
`#> Geodetic CRS:  WGS 84`\
`#>   poly_id scale priority                       geometry`\
`#> 1       1   0.7        1 POLYGON ((-51.22463 -30.034...`\
`#> 2       2   0.8        2 POLYGON ((-51.21426 -30.034...`\
`by_area`` ``<-`` ``car``(``new_carspeeds ``=`` ``areas``)`\
[`mean`](https://rdrr.io/r/base/mean.html)`(``by_area``$``travel_time_p50``)`\
`#> [1] 14.0304`

## Traffic stress

`new_lts` changes the level of traffic stress (1 to 4) cyclists meet, by
way (`osm_id`, `lts`) or along lines (an `sf` of `LINESTRING`s with
`line_id`, `lts` and `priority`, each street taking the level of the
nearest line). A cyclist rides only on streets up to `max_lts` and walks
the bicycle elsewhere:

\
`lts`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``poa``, ``"poa_osm_lts.csv"``)``)`\
[`head`](https://rdrr.io/r/utils/head.html)`(``lts``)`\
`#>      osm_id lts`\
`#> 1  27184648   1`\
`#> 2 762361901   1`\
`#> 3 568609955   1`\
`#> 4 709834913   1`\
`#> 5 709834914   1`\
`#> 6  77705540   1`\
`bike`` ``<-`` ``function``(``...``)`` ``{`\
`  `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``poa_network``, ``pts``, ``pts``, mode ``=`` ``"BICYCLE"``, max_lts ``=`` ``2``,`\
`                     n_threads ``=`` ``2``, ``...``)`\
`}`\
[`c`](https://rdrr.io/r/base/c.html)`(``base ``=`` `[`mean`](https://rdrr.io/r/base/mean.html)`(``bike``(``)``$``travel_time_p50``)``,`\
`  scenario ``=`` `[`mean`](https://rdrr.io/r/base/mean.html)`(``bike``(``new_lts ``=`` ``lts``)``$``travel_time_p50``)``)`\
`#>     base scenario `\
`#> 51.53024 51.48387`

A scenario changes the street search from the origins (the trip without
public transport, and access to it by bicycle or car) but not the egress
from the stops, and, on a network with an elevation raster, the call
runs without slope costs, with a warning (see
[Elevation](elevation.md)).
