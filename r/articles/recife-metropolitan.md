# Example: the Recife metropolitan region by public transport

The Região Metropolitana do Recife is 14 municipalities and 3.7 million
people on the coast of Pernambuco, Brazil. Its buses are run by ten
operators under one authority, the Grande Recife Consórcio de Transporte
Metropolitano, and meet the metro and the VLT (light rail) of CBTU at
integrated terminals. The consortium publishes the timetables of all of
them as one GTFS feed. This example builds the region as one network,
from that feed and from OpenStreetMap, and asks what a planner would ask
of it: how far one gets from the centre by each mode, how many schools
and health units are within reach of each resident on foot and by public
transport, what the rail lines add, and how much a trip depends on the
minute one leaves.

Everything is computed by peabiru; the output shown is what the code
prints, computed on 2026-11-13. The [Python
example](../../python/example-recife-metropolitan/) repeats the network,
the schools and the isochrones, and prints the same numbers.

![Four maps of central Recife and its neighbours, one per mode, each
shading the areas reached from Praça do Derby within 15, 30 and 45
minutes: a small area walking, more cycling, the whole map driving with
free-flowing traffic, and arms along the bus corridors and the metro
lines with public transport.](figures/isochrones-rmr.svg)

## The data

| Data | Source | Licence |
|----|----|----|
| Public transport | [Grande Recife Consórcio de Transporte Metropolitano](https://www.granderecife.pe.gov.br), GTFS feed at `https://www.granderecife.pe.gov.br/gtfs/gtfs.zip`, `feed_version` **v2026.10.26** (files of 2026-10-26; `feed_info.txt`: 2020-05-01 to 2027-10-26; 38 MB): 395 bus routes of ten operators and, of CBTU’s Metrorec, 3 metro and 2 VLT routes | published by the consortium at that address; no licence is stated in the feed |
| Streets | [OpenStreetMap](https://www.openstreetmap.org/copyright), extract of the Nordeste region by [Geofabrik](https://download.geofabrik.de/south-america/brazil/nordeste.html), dated 2026-11-01 (445 MB, MD5 `93318fcddd85a53e8386eaa93588847c`) | ODbL 1.0 |
| Municipal boundaries | [IBGE, malhas municipais](https://servicodados.ibge.gov.br/api/docs/malhas?versao=3) (API, version 3) | public data, IBGE |
| Population | [IBGE, Censo Demográfico 2022, census tracts with their population](https://ftp.ibge.gov.br/Censos/Censo_Demografico_2022/Agregados_por_Setores_Censitarios/malha_com_atributos/setores/gpkg/UF/PE/), variable `v0001` (44 MB for Pernambuco) | public data, IBGE |
| Schools | INEP, Catálogo de Escolas 2023, with the coordinates of each school, as published by Ipea’s [geobr](https://ipeagit.github.io/geobr/) (`schools_2023`, 101 MB for Brazil) | public data, INEP; geobr is MIT |
| Health units | [CNES](https://cnes.datasus.gov.br/), Ministério da Saúde, through [OpenDataSUS](https://opendatasus.saude.gov.br/) (`cnes_estabelecimentos_csv.zip`, 57 MB for Brazil, updated daily; downloaded on 2026-11-12) | open data, Ministério da Saúde |

The feed is not in this repository and is not redistributed here:
download it from the consortium’s address. That address always serves
the current file, so a later download is another version of the network
and gives other numbers; the checksum below identifies the file used
here. The region has no open grid of jobs: Ipea’s Access to
Opportunities data covers the municipality of Recife only, so the
opportunities here are the public schools and the primary health care
units of the registers, and the residents themselves.

The streets, the grid and the units are those of the [terrain
example](terrain.md), prepared by the functions of
[`site/figures/pernambuco-data.R`](https://github.com/StrategicProjects/peabiru-routing/blob/main/site/figures/pernambuco-data.R)
as the [Recife and Caruaru example](recife-caruaru.md) explains step by
step: the streets of the 14 municipalities cut with 3 km around them
(needs [osmium](https://osmcode.org/osmium-tool/)), the census
population on hexagonal cells 500 m apart, the public schools in
operation and the primary health care units (CNES types 1 and 2).
[`site/figures/rmr-data.R`](https://github.com/StrategicProjects/peabiru-routing/blob/main/site/figures/rmr-data.R)
adds the feed: `prepare_rmr()` uses the file
`rmr-data/raw/granderecife-v2026.10.26.zip` if it is there, or else
downloads the consortium’s current file, and puts it next to the street
extract in the directory
[`build_network()`](../reference/build_network.md) reads.

\
[`library`](https://rdrr.io/r/base/library.html)`(`[`peabiru`](https://strategicprojects.github.io/peabiru-routing/r/)`)`\
[`library`](https://rdrr.io/r/base/library.html)`(`[`sf`](https://r-spatial.github.io/sf/)`)`\
[`options`](https://rdrr.io/r/base/options.html)`(``timeout ``=`` ``3600``)`\
\
`scripts`` ``<-`` `[`Sys.getenv`](https://rdrr.io/r/base/Sys.getenv.html)`(``"PEABIRU_FIGURES_DIR"``, `[`paste0`](https://rdrr.io/r/base/paste.html)`(`\
`  ``"https://raw.githubusercontent.com/StrategicProjects/peabiru-routing/main/site/figures"``)``)`\
[`Sys.setenv`](https://rdrr.io/r/base/Sys.setenv.html)`(``PEABIRU_PE_CACHE ``=`` ``"pe-data"``, PEABIRU_RMR_CACHE ``=`` ``"rmr-data"``)`\
[`source`](https://rdrr.io/r/base/source.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``scripts``, ``"pernambuco-data.R"``)``)``   ``# prepare_city(), fetch()`\
[`source`](https://rdrr.io/r/base/source.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``scripts``, ``"rmr-data.R"``)``)``          ``# prepare_rmr(), the feed`\
`rmr`` ``<-`` ``prepare_rmr``(``)`\
\
[`c`](https://rdrr.io/r/base/c.html)`(``MB ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`file.size`](https://rdrr.io/r/base/file.info.html)`(``rmr``$``feed``)`` ``/`` ``1e6``, ``1``)``, sha256 ``=`` `[`substr`](https://rdrr.io/r/base/substr.html)`(`[`unname`](https://rdrr.io/r/base/unname.html)`(``tools``::`[`sha256sum`](https://rdrr.io/r/tools/sha256sum.html)`(``rmr``$``feed``)``)``, ``1``, ``16``)``)`\
`#>                 MB             sha256 `\
`#>             "38.3" "1b0657fb8f6bcb70"`\
`feed`` ``<-`` ``rmr_feed_summary``(``rmr``$``feed``)`\
[`str`](https://rdrr.io/r/utils/str.html)`(``feed``[`[`c`](https://rdrr.io/r/base/c.html)`(``"publisher"``, ``"version"``, ``"agencies"``, ``"routes"``, ``"stops"``, ``"stations"``, ``"trips"``,`\
`           ``"frequencies"``, ``"transfer_rules"``)``]``, give.attr ``=`` ``FALSE``)`\
`#> List of 9`\
`#>  $ publisher     : chr "Grande Recife Consórcio de Transporte Metropolitano"`\
`#>  $ version       : chr "v2026.10.26"`\
`#>  $ agencies      : int 11`\
`#>  $ routes        : int 400`\
`#>  $ stops         : int 7238`\
`#>  $ stations      : int 30`\
`#>  $ trips         : int 87678`\
`#>  $ frequencies   : logi FALSE`\
`#>  $ transfer_rules: int 0`\
`feed``$``by_mode`\
`#> `\
`#>              bus light rail (VLT)            metro `\
`#>              395                2                3`\
`feed``$``fare_prices`\
`#> `\
`#> R$ 15.40 R$ 22.50  R$ 3.00  R$ 4.25  R$ 4.50  R$ 5.80  R$ 8.70 `\
`#>        1        1        2        5      385        1        5`

Timetables only: no line is given by headway (`frequencies.txt`), so
nothing below is drawn at random. The 30 stations are the 26 integrated
terminals and four rail stations.

**No fares in this example.** The feed has a price for each route (R\$
4.50 on 385 of the 400) and no rule for transfers: the integration at
the terminals, where passengers change lines without paying again, is
not in it. A fare structure made from the feed would charge every
boarding, and results with a fare limit would understate what a fare
buys; so travel time, transfers and walking are measured here, and money
is not.

## The network

\
`threads`` ``<-`` ``4``   ``# of the 10 of the laptop; the default is all of them`\
`(``seconds`` ``<-`` `[`system.time`](https://rdrr.io/r/base/system.time.html)`(``network`` ``<-`` `[`build_network`](../reference/build_network.md)`(``rmr``$``network``, n_threads ``=`` ``threads``)``)``[[``"elapsed"``]``]``)`\
`#> [1] 6.998`\
`network`\
`#> <peabiru_network>`\
`#>   data path: rmr-data/network`\
`#>   streets:   153,798 vertices, 395,862 edges`\
`#>   transit:   1 feed(s) (granderecife-v2026.10.26), 7,238 stops, 1,215 patterns, 1,143 services`\
`#>   elevation: NONE`\
[`nrow`](https://rdrr.io/r/base/nrow.html)`(`[`get_gtfs_errors`](../reference/get_gtfs_errors.md)`(``network``)``)`\
`#> [1] 0`\
[`check_transit_availability`](../reference/check_transit_availability.md)`(``network``, dates ``=`` `[`as.Date`](https://rdrr.io/r/base/as.Date.html)`(``"2026-11-10"``)``)`\
`#> # A tibble: 1 × 4`\
`#>   date       total_services active_services pct_active`\
`#>   <date>              <int>           <int>      <dbl>`\
`#> 1 2026-11-10           1143             395      0.346`

One street extract of 21 MB and one feed, built in 7 seconds on a laptop
busy with other work; the feed has no error. `pct_active` is low because
the feed keeps a service for each route and kind of day (weekday,
Saturday, Sunday and holiday): on a Tuesday the weekday ones run.

The analysis is of **Tuesday 10 November 2026, leaving at each minute
from 07:00 to 07:59**; a travel time is the median of those 60
departures unless said otherwise. Walking is at 3.6 km/h and cycling at
12 km/h on streets of low traffic stress, the defaults; driving is at
the speed limits, with no congestion.

\
`departure`` ``<-`` `[`as.POSIXct`](https://rdrr.io/r/base/as.POSIXlt.html)`(``"2026-11-10 07:00:00"``, tz ``=`` ``"America/Recife"``)`\
`transit`` ``<-`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)`\
\
`cells`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``rmr``$``streets``, ``"cells.csv"``)``, colClasses ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``id ``=`` ``"character"``)``)`\
`cells`` ``<-`` ``cells``[``cells``$``population`` ``>`` ``0``, ``]`\
`schools`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``rmr``$``streets``, ``"schools.csv"``)``, colClasses ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``id ``=`` ``"character"``)``)`\
`health`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``rmr``$``streets``, ``"health.csv"``)``, colClasses ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``id ``=`` ``"character"``)``)`\
`people`` ``<-`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``cells``$``population``)`\
[`c`](https://rdrr.io/r/base/c.html)`(``cells ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``cells``)``, people ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``people``)``, schools ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``schools``)``, health_units ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``health``)``)`\
`#>        cells       people      schools health_units `\
`#>        13096      3730664         1203          691`

## What the timetable assumes

A timetable is a plan. One written for empty streets would put the bus
through the morning peak at the speed of the small hours, and every
result below would overstate what public transport reaches at 07:00. So,
before routing: the scheduled speed of every trip of the day, the length
of its path (`shape_dist_traveled`) over the time between its first and
its last stop, by hour of departure.

\
`speeds`` ``<-`` ``rmr_trip_speeds``(``rmr``$``feed``, `[`as.Date`](https://rdrr.io/r/base/as.Date.html)`(``"2026-11-10"``)``)`\
`speeds``$``kmh`` ``<-`` ``speeds``$``shape_km`` ``/`` ``(``speeds``$``minutes`` ``/`` ``60``)`\
[`table`](https://rdrr.io/r/base/table.html)`(``speeds``$``mode``)`\
`#> `\
`#>   Bus Metro   VLT `\
`#> 39463   358    56`\
`hours`` ``<-`` `[`c`](https://rdrr.io/r/base/c.html)`(``4``, ``7``, ``12``, ``17``, ``22``)`\
`kmh`` ``<-`` `[`sapply`](https://rdrr.io/r/base/lapply.html)`(`[`split`](https://rdrr.io/r/base/split.html)`(``speeds``, ``speeds``$``mode``)``, ``function``(``m``)`` `[`tapply`](https://rdrr.io/r/base/tapply.html)`(``m``$``kmh``, ``m``$``hour``, ``median``)``[`[`as.character`](https://rdrr.io/r/base/character.html)`(``hours``)``]``)`\
[`round`](https://rdrr.io/r/base/Round.html)`(``kmh``, ``1``)`\
`#>     Bus Metro  VLT`\
`#> 4  24.8    NA   NA`\
`#> 7  15.4  38.6 31.1`\
`#> 12 17.0  38.6 31.1`\
`#> 17 14.8  38.6 30.8`\
`#> 22 22.7  38.6   NA`\
\
`# the same path through the day: its running time over its own shortest`\
`bus`` ``<-`` ``speeds``[``speeds``$``mode`` ``==`` ``"Bus"``, ``]`\
`by_hour`` ``<-`` `[`aggregate`](https://rdrr.io/r/stats/aggregate.html)`(``minutes`` ``~`` ``shape_id`` ``+`` ``hour``, data ``=`` ``bus``, FUN ``=`` ``median``)`\
`by_hour``$``ratio`` ``<-`` ``by_hour``$``minutes`` ``/`` `[`ave`](https://rdrr.io/r/stats/ave.html)`(``by_hour``$``minutes``, ``by_hour``$``shape_id``, FUN ``=`` ``min``)`\
[`round`](https://rdrr.io/r/base/Round.html)`(`[`tapply`](https://rdrr.io/r/base/tapply.html)`(``by_hour``$``ratio``, ``by_hour``$``hour``, ``median``)``[`[`as.character`](https://rdrr.io/r/base/character.html)`(``hours``)``]``, ``2``)`\
`#>    4    7   12   17   22 `\
`#> 1.00 1.50 1.43 1.60 1.00`

This timetable does slow down: the median bus trip that starts at 07:00
is scheduled at 15.4 km/h and the one at 17:00 at 14.8, against 24.8 at
04:00 and 22.7 at 22:00, and the same path takes 1.5 times its shortest
running time of the day at 07:00 and 1.6 times at 17:00. The metro and
the VLT keep one speed all day (their medians move with the mix of
lines). So the feed carries peak running times, and the access measured
below is not that of an empty city. It is still a schedule: these are
planned times, not observed ones, and nothing here says how often the
plan holds.

![Lines of the median scheduled speed of the trips of the feed by hour
of departure on a weekday: buses fall from about 25 km/h at 04:00 to 15
at 07:00, stay near 17 through the day, fall to 15 at 17:00 and recover
in the evening; the metro and the VLT keep the same speed all
day.](figures/speeds-rmr.svg)

## Isochrones by mode

From Praça do Derby, between the centre of Recife and its west and
north: 74 of the feed’s 400 routes stop within 400 m of the square on a
weekday. [`isochrone()`](../reference/isochrone.md) traces the areas on
a grid of points, and the points just off the coast are reached from the
seafront, so the polygons are cut at the coast (the municipal
boundaries) before their area is measured.
([`isochrone()`](../reference/isochrone.md) warns when making a traced
polygon valid on the sphere would change its area, and repairs it on the
plane instead; it did for three of these polygons, and the warnings are
left out of the output.)

\
`derby`` ``<-`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` ``"Praça do Derby"``, lon ``=`` ``-``34.8990``, lat ``=`` ``-``8.0570``)`\
`modes`` ``<-`` `[`list`](https://rdrr.io/r/base/list.html)`(``walk ``=`` ``"WALK"``, bicycle ``=`` ``"BICYCLE"``, car ``=`` ``"CAR"``, transit ``=`` ``transit``)`\
`utm`` ``<-`` ``31985``   ``# SIRGAS 2000 / UTM zone 25S, in metres`\
\
`boundaries`` ``<-`` `[`st_read`](https://r-spatial.github.io/sf/reference/st_read.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``rmr``$``streets``, ``"boundary.geojson"``)``, quiet ``=`` ``TRUE``)`\
`land`` ``<-`` `[`st_union`](https://r-spatial.github.io/sf/reference/geos_combine.html)`(`[`st_make_valid`](https://r-spatial.github.io/sf/reference/valid.html)`(`[`st_transform`](https://r-spatial.github.io/sf/reference/st_transform.html)`(``boundaries``, ``utm``)``)``)`\
`km2`` ``<-`` `[`sapply`](https://rdrr.io/r/base/lapply.html)`(``modes``, ``function``(``mode``)`` ``{`\
`  ``iso`` ``<-`` `[`isochrone`](../reference/isochrone.md)`(``network``, ``derby``, mode ``=`` ``mode``, cutoffs ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``15``, ``30``, ``45``)``, zoom ``=`` ``11``,`\
`                   departure_datetime ``=`` ``departure``, time_window ``=`` ``60``, n_threads ``=`` ``threads``)`\
`  ``on_land`` ``<-`` `[`st_intersection`](https://r-spatial.github.io/sf/reference/geos_binary_ops.html)`(`[`st_make_valid`](https://r-spatial.github.io/sf/reference/valid.html)`(`[`st_geometry`](https://r-spatial.github.io/sf/reference/st_geometry.html)`(`[`st_transform`](https://r-spatial.github.io/sf/reference/st_transform.html)`(``iso``, ``utm``)``)``)``, ``land``)`\
`  `[`setNames`](https://rdrr.io/r/stats/setNames.html)`(`[`as.numeric`](https://rdrr.io/r/base/numeric.html)`(`[`st_area`](https://r-spatial.github.io/sf/reference/geos_measures.html)`(``on_land``)``)`` ``/`` ``1e6``, `[`paste`](https://rdrr.io/r/base/paste.html)`(``iso``$``isochrone``, ``"min"``)``)`\
`}``)`\
[`round`](https://rdrr.io/r/base/Round.html)`(`[`t`](https://rdrr.io/r/base/t.html)`(``km2``)``, ``1``)`\
`#>         45 min 30 min 15 min`\
`#> walk      15.2    6.6    1.5`\
`#> bicycle   43.5   16.4    2.1`\
`#> car     1453.0  618.4  105.9`\
`#> transit  108.1   34.8    3.8`

In 45 minutes public transport reaches 108 km² of land, cycling 44 and
walking 15; a car in free-flowing traffic reaches 1,453 km², which no
car does at 07:00. The figure at the top of the page draws these
polygons.

## From one point to every cell

The same question on the grid: the travel time from Praça do Derby to
each of the 13,096 inhabited cells, and the residents within reach.
`bus` is public transport without the metro and the VLT, for later.

\
`from_derby`` ``<-`` `[`sapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``modes``, `[`list`](https://rdrr.io/r/base/list.html)`(``bus ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"BUS"``)``)``)``, ``function``(``mode``)`` ``{`\
`  ``m`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``network``, ``derby``, ``cells``, mode ``=`` ``mode``, departure_datetime ``=`` ``departure``,`\
`                          time_window ``=`` ``60``, max_trip_duration ``=`` ``120``, n_threads ``=`` ``threads``)`\
`  ``m``$``travel_time_p50``[`[`match`](https://rdrr.io/r/base/match.html)`(``cells``$``id``, ``m``$``to_id``)``]`\
`}``)`\
`within`` ``<-`` ``function``(``minutes``, ``limit``)`` ``{`\
`  `[`sum`](https://rdrr.io/r/base/sum.html)`(``cells``$``population``[``!`[`is.na`](https://rdrr.io/r/base/NA.html)`(``minutes``)`` ``&`` ``minutes`` ``<=`` ``limit``]``)`` ``/`` ``people`\
`}`\
`shares`` ``<-`` `[`sapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``` `30 min`  ```=`` ``30``` , `60 min`  ```=`` ``60``` , `90 min`  ```=`` ``90``` , `120 min`  ```=`` ``120``)``,`\
`                 ``function``(``limit``)`` `[`apply`](https://rdrr.io/r/base/apply.html)`(``from_derby``, ``2``, ``within``, limit ``=`` ``limit``)``)`\
[`round`](https://rdrr.io/r/base/Round.html)`(``100`` ``*`` ``shares``, ``1``)`\
`#>         30 min 60 min 90 min 120 min`\
`#> walk       1.6    7.6   16.6    26.4`\
`#> bicycle    5.0   25.2   44.3    63.3`\
`#> car       84.0   98.7   99.4    99.4`\
`#> transit    9.2   52.5   84.5    91.3`\
`#> bus        9.2   50.7   82.5    91.0`

Within an hour of the square are 53% of the region’s residents by public
transport, 25% by bicycle and 8% on foot; within 90 minutes, 85% by
public transport. These trips leave the centre in the morning, against
the main flow.

![Two maps of the conurbation of Recife with the cells of a grid
coloured by the median travel time from Praça do Derby at the morning
peak: walking, a patch of five kilometres around the square; with public
transport, most of the built-up area within 90
minutes.](figures/travel-time-rmr.svg)![Step curves of the share of the
population of the Recife metropolitan region reached from Praça do Derby
within 0 to 120 minutes, by mode: driving in free flow first, then
walking plus public transport, cycling and
walking.](figures/reach-rmr.svg)

## Schools, health units and people within reach

Cumulative access from every inhabited cell: the public schools, the
primary health care units and the residents of the region reached within
30, 45 and 60 minutes, on foot, by public transport, and by bus alone.
One call per mode, with the three kinds of opportunity as columns of one
table of destinations.

\
`destinations`` ``<-`` `[`rbind`](https://rdrr.io/r/base/cbind.html)`(`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` `[`paste0`](https://rdrr.io/r/base/paste.html)`(``"s"``, ``schools``$``id``)``, lon ``=`` ``schools``$``lon``, lat ``=`` ``schools``$``lat``,`\
`             schools ``=`` ``1``, health ``=`` ``0``, population ``=`` ``0``)``,`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` `[`paste0`](https://rdrr.io/r/base/paste.html)`(``"h"``, ``health``$``id``)``, lon ``=`` ``health``$``lon``, lat ``=`` ``health``$``lat``,`\
`             schools ``=`` ``0``, health ``=`` ``1``, population ``=`` ``0``)``,`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` `[`paste0`](https://rdrr.io/r/base/paste.html)`(``"c"``, ``cells``$``id``)``, lon ``=`` ``cells``$``lon``, lat ``=`` ``cells``$``lat``,`\
`             schools ``=`` ``0``, health ``=`` ``0``, population ``=`` ``cells``$``population``)``)`\
\
`access_of`` ``<-`` ``function``(``mode``)`` ``{`\
`  ``a`` ``<-`` `[`accessibility`](../reference/accessibility.md)`(``network``, ``cells``, ``destinations``,`\
`                     opportunities_colnames ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"schools"``, ``"health"``, ``"population"``)``,`\
`                     mode ``=`` ``mode``, departure_datetime ``=`` ``departure``, time_window ``=`` ``60``,`\
`                     percentiles ``=`` ``50``, cutoffs ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``30``, ``45``, ``60``)``, max_trip_duration ``=`` ``60``,`\
`                     n_threads ``=`` ``threads``, progress ``=`` ``FALSE``)`\
`  ``# one row per cell, one column per opportunity and limit: schools_30, ...`\
`  ``a``$``what`` ``<-`` `[`paste`](https://rdrr.io/r/base/paste.html)`(``a``$``opportunity``, ``a``$``cutoff``, sep ``=`` ``"_"``)`\
`  ``wide`` ``<-`` `[`reshape`](https://rdrr.io/r/stats/reshape.html)`(`[`as.data.frame`](https://rdrr.io/r/base/as.data.frame.html)`(``a``[``, `[`c`](https://rdrr.io/r/base/c.html)`(``"id"``, ``"what"``, ``"accessibility"``)``]``)``, idvar ``=`` ``"id"``,`\
`                  timevar ``=`` ``"what"``, direction ``=`` ``"wide"``)`\
`  `[`names`](https://rdrr.io/r/base/names.html)`(``wide``)`` ``<-`` `[`sub`](https://rdrr.io/r/base/grep.html)`(``"accessibility."``, ``""``, `[`names`](https://rdrr.io/r/base/names.html)`(``wide``)``, fixed ``=`` ``TRUE``)`\
`  ``wide``[`[`match`](https://rdrr.io/r/base/match.html)`(``cells``$``id``, ``wide``$``id``)``, ``]`\
`}`\
`seconds`` ``<-`` `[`system.time`](https://rdrr.io/r/base/system.time.html)`(`\
`  ``access`` ``<-`` `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`list`](https://rdrr.io/r/base/list.html)`(``walk ``=`` ``"WALK"``, transit ``=`` ``transit``, bus ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"BUS"``)``)``, ``access_of``)`\
`)``[[``"elapsed"``]``]`\
[`c`](https://rdrr.io/r/base/c.html)`(``origins ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``cells``)``, destinations ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``destinations``)``, seconds ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``seconds``)``)`\
`#>      origins destinations      seconds `\
`#>        13096        14990           34`

\
`average`` ``<-`` ``function``(``column``)`` ``{`\
`  `[`sapply`](https://rdrr.io/r/base/lapply.html)`(``access``[`[`c`](https://rdrr.io/r/base/c.html)`(``"walk"``, ``"transit"``)``]``, ``function``(``a``)`` `[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``a``[[``column``]``]``, ``cells``$``population``)``)`\
`}`\
[`round`](https://rdrr.io/r/base/Round.html)`(`[`t`](https://rdrr.io/r/base/t.html)`(`[`sapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"schools_30"``, ``"schools_45"``, ``"health_30"``, ``"health_45"``, ``"population_60"``)``, ``average``)``)``, ``1``)`\
`#>                   walk  transit`\
`#> schools_30        12.8     24.4`\
`#> schools_45        26.5     79.4`\
`#> health_30          6.2     11.7`\
`#> health_45         12.7     40.3`\
`#> population_60 150756.9 590000.0`\
`none`` ``<-`` ``function``(``column``)`` ``{`\
`  `[`sapply`](https://rdrr.io/r/base/lapply.html)`(``access``[`[`c`](https://rdrr.io/r/base/c.html)`(``"walk"``, ``"transit"``)``]``, ``function``(``a``)`` `[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``a``[[``column``]``]`` ``==`` ``0``, ``cells``$``population``)``)`\
`}`\
[`round`](https://rdrr.io/r/base/Round.html)`(``100`` ``*`` `[`t`](https://rdrr.io/r/base/t.html)`(`[`sapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"schools_30"``, ``"health_30"``)``, ``none``)``)``, ``1``)`\
`#>            walk transit`\
`#> schools_30  3.1     2.8`\
`#> health_30   5.0     3.6`

The average resident has 13 public schools within half an hour’s walk
and 24 within half an hour by public transport; with 45 minutes, 26 and
79. For primary health care units the numbers are 6 and 12 in 30
minutes, 13 and 40 in 45. The gain grows with the time allowed: public
transport doubles the count in half an hour and triples it in three
quarters. The last table is the share of residents with none within 30
minutes: 3.1% have no public school within half an hour’s walk and 2.8%
none by public transport either; for a health unit, 5.0% and 3.6%.

On the laptop this was written on (Apple M2 Pro, 4 of its 10 threads,
busy with other work at the time) the three calls, each from 13,096
origins to 14,990 destinations, took 34 seconds together.

![Two maps of the conurbation of Recife with the cells of a grid
coloured by the number of public schools reached within 30 minutes at
the morning peak, walking and with public transport: with public
transport the cells along the corridors that lead to the centre of
Recife reach 80 or more.](figures/access-rmr.svg)

By municipality of residence, the share of the region’s residents within
an hour (the population of each cell is split by municipality in
`cells_muni.csv`):

\
`names_of`` ``<-`` `[`c`](https://rdrr.io/r/base/c.html)`(``"2611606"`` ``=`` ``"Recife"``, ``"2609600"`` ``=`` ``"Olinda"``, ``"2607901"`` ``=`` ``"Jaboatão dos Guararapes"``,`\
`              ``"2610707"`` ``=`` ``"Paulista"``, ``"2603454"`` ``=`` ``"Camaragibe"``, ``"2613701"`` ``=`` ``"São Lourenço da Mata"``,`\
`              ``"2602902"`` ``=`` ``"Cabo de Santo Agostinho"``, ``"2607208"`` ``=`` ``"Ipojuca"``, ``"2609402"`` ``=`` ``"Moreno"``,`\
`              ``"2606804"`` ``=`` ``"Igarassu"``, ``"2600054"`` ``=`` ``"Abreu e Lima"``, ``"2607752"`` ``=`` ``"Itapissuma"``,`\
`              ``"2607604"`` ``=`` ``"Ilha de Itamaracá"``, ``"2601052"`` ``=`` ``"Araçoiaba"``)`\
`by_muni`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``rmr``$``streets``, ``"cells_muni.csv"``)``,`\
`                    colClasses ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``id ``=`` ``"character"``, code ``=`` ``"character"``)``)`\
`k`` ``<-`` `[`match`](https://rdrr.io/r/base/match.html)`(``by_muni``$``id``, ``cells``$``id``)`\
`by_muni`` ``<-`` ``by_muni``[``!`[`is.na`](https://rdrr.io/r/base/NA.html)`(``k``)``, ``]`\
`k`` ``<-`` ``k``[``!`[`is.na`](https://rdrr.io/r/base/NA.html)`(``k``)``]`\
`for`` ``(``m`` ``in`` `[`names`](https://rdrr.io/r/base/names.html)`(``access``)``)`` ``by_muni``[[``m``]``]`` ``<-`` ``access``[[``m``]``]``$``population_60``[``k``]`\
`municipalities`` ``<-`` `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`split`](https://rdrr.io/r/base/split.html)`(``by_muni``, ``by_muni``$``code``)``, ``function``(``r``)`` ``{`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``municipality ``=`` ``names_of``[[``r``$``code``[``1``]``]``]``, residents ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`sum`](https://rdrr.io/r/base/sum.html)`(``r``$``population``)``)``,`\
`             walk ``=`` `[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``r``$``walk``, ``r``$``population``)``, bus ``=`` `[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``r``$``bus``, ``r``$``population``)``,`\
`             transit ``=`` `[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``r``$``transit``, ``r``$``population``)``)`\
`}``)``)`\
`municipalities`` ``<-`` ``municipalities``[`[`order`](https://rdrr.io/r/base/order.html)`(``-``municipalities``$``transit``)``, ``]`\
`municipalities``$``share`` ``<-`` `[`round`](https://rdrr.io/r/base/Round.html)`(``100`` ``*`` ``municipalities``$``transit`` ``/`` ``people``, ``1``)`\
`municipalities``$``rail_gain`` ``<-`` `[`round`](https://rdrr.io/r/base/Round.html)`(``100`` ``*`` ``(``municipalities``$``transit`` ``/`` ``municipalities``$``bus`` ``-`` ``1``)``, ``1``)`\
[`print`](https://rdrr.io/r/base/print.html)`(`[`transform`](https://rdrr.io/r/base/transform.html)`(``municipalities``, walk ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``walk``)``, bus ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``bus``)``, transit ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``transit``)``)``,`\
`      row.names ``=`` ``FALSE``)`\
`#>             municipality residents   walk    bus transit share rail_gain`\
`#>                   Recife   1488924 226475 874573  925812  24.8       5.9`\
`#>                   Olinda    349974 180841 734408  734832  19.7       0.1`\
`#>                 Paulista    342160 100084 434679  434679  11.7       0.0`\
`#>  Jaboatão dos Guararapes    644037 124999 356771  411342  11.0      15.3`\
`#>               Camaragibe    147771  90217 287313  349820   9.4      21.8`\
`#>             Abreu e Lima     98462  68258 284454  284454   7.6       0.0`\
`#>                 Igarassu    115196  45289 169416  169416   4.5       0.0`\
`#>     São Lourenço da Mata    111249  59041 151898  156725   4.2       3.2`\
`#>  Cabo de Santo Agostinho    203440  52909 118551  120704   3.2       1.8`\
`#>                   Moreno     55292  29018  67326   67851   1.8       0.8`\
`#>               Itapissuma     31320  16436  62106   62106   1.7       0.0`\
`#>                  Ipojuca     99057  18917  38969   38969   1.0       0.0`\
`#>        Ilha de Itamaracá     24540  11096  29454   29454   0.8       0.0`\
`#>                Araçoiaba     19243  16319  16546   16546   0.4       0.0`

![A dot plot with one row per municipality of the Recife metropolitan
region: the share of the residents of the region that the average
resident of the municipality reaches within 60 minutes on foot and with
public transport; Recife first with a quarter of the region, Olinda next
with a fifth, and the outer municipalities with under 5
percent.](figures/municipalities-rmr.svg)

## What the metro and the VLT add

`mode` chooses the kinds of public transport: `c("WALK", "BUS")` is the
same network without the rail lines. The bus timetables are the same in
both calls, so this measures what the rail lines add to the service as
it runs, not what the bus network would be without them. `rail_gain`,
above, is that difference for the average resident of each municipality;
for the region:

\
`with_rail`` ``<-`` ``access``$``transit``$``population_60`\
`bus_only`` ``<-`` ``access``$``bus``$``population_60`\
[`c`](https://rdrr.io/r/base/c.html)`(``with_rail ``=`` `[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``with_rail``, ``cells``$``population``)``,`\
`  bus_only ``=`` `[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``bus_only``, ``cells``$``population``)``)`\
`#> with_rail  bus_only `\
`#>  590000.0  557344.5`\
`gain`` ``<-`` `[`ifelse`](https://rdrr.io/r/base/ifelse.html)`(``bus_only`` ``>`` ``0``, ``with_rail`` ``/`` ``bus_only`` ``-`` ``1``, ``0``)`\
[`round`](https://rdrr.io/r/base/Round.html)`(``100`` ``*`` `[`c`](https://rdrr.io/r/base/c.html)`(``` `10% more or over`  ```=`` `[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``gain`` ``>=`` ``0.10``, ``cells``$``population``)``,`\
``               `25% more or over`  ```=`` `[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``gain`` ``>=`` ``0.25``, ``cells``$``population``)``)``, ``1``)`\
`#> 10% more or over 25% more or over `\
`#>             14.7              7.7`

The average resident reaches 590,000 people within an hour with the rail
lines and 557,345 without, 5.9% more. The gain is where the lines are:
22% in Camaragibe and 15% in Jaboatão dos Guararapes, at the ends of the
two branches of the Centro line, and nothing in the north of the region
(Olinda, Paulista, Abreu e Lima, Igarassu), which the rail does not
reach.

![On the left, a map of the west and south of the conurbation of Recife
with the metro and VLT lines and the cells of a grid coloured by how
many more residents they reach within an hour by public transport with
the rail lines than with buses alone: the gain follows the lines to
Camaragibe, Jaboatão and Cajueiro Seco. On the right, bars of the gain
by municipality: 22% in Camaragibe, 15% in Jaboatão dos Guararapes, 6%
in Recife, none in the north.](figures/rail-rmr.svg)

## How much a trip varies

A travel time by public transport depends on the minute one leaves. With
`percentiles`, the matrix gives the spread over the departure window:
here, from each of the 26 integrated terminals to Praça do Derby, the
5th, 25th, 50th, 75th and 95th percentiles of the 60 travel times.

\
`stops`` ``<-`` ``feed_table``(``rmr``$``feed``, ``"stops"``)`\
`terminals`` ``<-`` ``stops``[``stops``$``location_type`` ``==`` ``"1"`` ``&`` `[`grepl`](https://rdrr.io/r/base/grep.html)`(``"^Ti "``, ``stops``$``stop_name``)``, ``]`\
`terminals`` ``<-`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` `[`sub`](https://rdrr.io/r/base/grep.html)`(``" - [0-9]+$"``, ``""``, ``terminals``$``stop_name``)``,`\
`                        lon ``=`` `[`as.numeric`](https://rdrr.io/r/base/numeric.html)`(``terminals``$``stop_lon``)``, lat ``=`` `[`as.numeric`](https://rdrr.io/r/base/numeric.html)`(``terminals``$``stop_lat``)``)`\
`p`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``network``, ``terminals``, ``derby``, mode ``=`` ``transit``, departure_datetime ``=`` ``departure``,`\
`                        time_window ``=`` ``60``, percentiles ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``5``, ``25``, ``50``, ``75``, ``95``)``,`\
`                        max_trip_duration ``=`` ``180``, n_threads ``=`` ``threads``)`\
`p`` ``<-`` `[`as.data.frame`](https://rdrr.io/r/base/as.data.frame.html)`(``p``[`[`order`](https://rdrr.io/r/base/order.html)`(``p``$``travel_time_p50``, ``p``$``travel_time_p95``)``, ``]``)`\
[`names`](https://rdrr.io/r/base/names.html)`(``p``)`` ``<-`` `[`sub`](https://rdrr.io/r/base/grep.html)`(``"travel_time_"``, ``""``, `[`names`](https://rdrr.io/r/base/names.html)`(``p``)``)`\
[`print`](https://rdrr.io/r/base/print.html)`(``p``[``, `[`c`](https://rdrr.io/r/base/c.html)`(``"from_id"``, ``"p05"``, ``"p25"``, ``"p50"``, ``"p75"``, ``"p95"``)``]``, row.names ``=`` ``FALSE``)`\
`#>                from_id p05 p25 p50 p75 p95`\
`#>       Ti Joana Bezerra  11  13  13  14  15`\
`#>      Ti Getulio Vargas  16  17  18  19  20`\
`#>            Ti Afogados  21  23  24  25  29`\
`#>              Ti Recife  22  24  25  26  28`\
`#>                 Ti Cdu  22  25  26  27  28`\
`#>        Ti Largo Da Paz  24  26  28  30  31`\
`#>             Ti Caxangá  27  29  31  32  33`\
`#>         Ti Santa Luzia  31  34  37  40  44`\
`#>           Ti Aeroporto  34  36  39  43  46`\
`#>               Ti Pe-15  37  39  40  41  43`\
`#>      Ti Tancredo Neves  35  37  40  44  47`\
`#>               Ti Barro  38  40  43  46  50`\
`#>               Ti Xambá  43  44  46  48  50`\
`#>           Ti Macaxeira  42  44  46  48  51`\
`#>            Ti Prazeres  40  43  46  49  52`\
`#>           Ti Cavaleiro  38  43  48  52  56`\
`#>       Ti Cajueiro Seco  47  50  53  56  60`\
`#>      Ti Cosme E Damião  51  53  54  56  57`\
`#>            Ti Jaboatão  45  50  55  61  65`\
`#>          Ti Camaragibe  51  54  56  59  61`\
`#>                 Ti Tip  48  52  57  62  66`\
`#>  Ti Pelópidas Silveira  56  60  61  63  66`\
`#>        Ti Abreu E Lima  61  64  68  71  74`\
`#>            Ti Rio Doce  65  68  72  75  78`\
`#>                Ti Cabo  82  89  96 102 108`\
`#>            Ti Igarassu  92  94  97  99 101`

![For each of the 26 integrated terminals of Grande Recife, sorted by
median travel time to Praça do Derby at the morning peak, a range bar
from the 5th to the 95th percentile of the travel times over an hour of
departures, a thicker bar from the 25th to the 75th, and a dot at the
median.](figures/percentiles-rmr.svg)

[`expanded_travel_time_matrix()`](../reference/expanded_travel_time_matrix.md)
shows where the spread comes from: one row per departure minute, with
the lines taken and, with `breakdown = TRUE`, the parts of the trip.
From the terminal of Camaragibe, the end of a metro line, to Praça de
Boa Viagem, by the beach in the south of Recife:

\
`camaragibe`` ``<-`` ``terminals``[``terminals``$``id`` ``==`` ``"Ti Camaragibe"``, ``]`\
`boa_viagem`` ``<-`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` ``"Praça de Boa Viagem"``, lon ``=`` ``-``34.9014``, lat ``=`` ``-``8.1317``)`\
`e`` ``<-`` `[`expanded_travel_time_matrix`](../reference/expanded_travel_time_matrix.md)`(``network``, ``camaragibe``, ``boa_viagem``, mode ``=`` ``transit``,`\
`                                 departure_datetime ``=`` ``departure``, time_window ``=`` ``30``,`\
`                                 breakdown ``=`` ``TRUE``, n_threads ``=`` ``threads``)`\
`e`` ``<-`` `[`as.data.frame`](https://rdrr.io/r/base/as.data.frame.html)`(``e``)`\
`e``[`[`c`](https://rdrr.io/r/base/c.html)`(``1``:``3``, ``21``:``24``)``, `[`c`](https://rdrr.io/r/base/c.html)`(``"departure_time"``, ``"access_time"``, ``"wait_time"``, ``"ride_time"``, ``"transfer_time"``,`\
`                   ``"egress_time"``, ``"routes"``, ``"total_time"``)``]`\
`#>    departure_time access_time wait_time ride_time transfer_time egress_time`\
`#> 1        07:00:00           5       5.2      51.0           1.0         0.1`\
`#> 2        07:01:00           5      30.6      46.7           1.0         0.1`\
`#> 3        07:02:00           5      29.6      46.7           1.0         0.1`\
`#> 21       07:20:00           5      11.6      46.7           1.0         0.1`\
`#> 22       07:21:00           5      10.6      46.7           1.0         0.1`\
`#> 23       07:22:00           5      24.8      52.5           1.1         0.1`\
`#> 24       07:23:00           5      23.8      52.5           1.1         0.1`\
`#>                         routes total_time`\
`#> 1  METRO_CENTRO1|METRO_SUL|118       62.3`\
`#> 2  METRO_CENTRO1|METRO_SUL|195       83.4`\
`#> 3  METRO_CENTRO1|METRO_SUL|195       82.4`\
`#> 21 METRO_CENTRO1|METRO_SUL|195       64.4`\
`#> 22 METRO_CENTRO1|METRO_SUL|195       63.4`\
`#> 23      METRO_CENTRO1|115|2040       83.5`\
`#> 24      METRO_CENTRO1|115|2040       82.5`\
[`range`](https://rdrr.io/r/base/range.html)`(``e``$``total_time``)`\
`#> [1] 62.3 83.5`

Leaving at 07:00 the trip takes 62 minutes, and one minute later 83: the
two metro rides are the same, and the last leg is a bus that the 07:00
departure catches and the 07:01 does not. Every departure from 07:01 to
07:21 arrives on the same bus; the ride hardly changes, the wait does.

![Thirty stacked bars, one per departure minute from 07:00 to 07:29,
from the terminal of Camaragibe to Praça de Boa Viagem, split into
walking, waiting and riding, with the metro and bus lines taken; totals
between 62 and 84 minutes, the wait making the
difference.](figures/trip-breakdown-rmr.svg)

## The options of one trip

[`trip_options()`](../reference/trip_options.md) lists the trips that no
other beats on every criterion at once. From Praça do Carmo, at the foot
of the historic centre of Olinda, to Praça de Boa Viagem, leaving at
07:20, with time, transfers and walking as criteria (no `"FARE"`: see
above):

\
`carmo`` ``<-`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` ``"Praça do Carmo, Olinda"``, lon ``=`` ``-``34.8485``, lat ``=`` ``-``8.0170``)`\
`res`` ``<-`` `[`trip_options`](../reference/trip_options.md)`(``network``, ``carmo``, ``boa_viagem``,`\
`                    departure_datetime ``=`` `[`as.POSIXct`](https://rdrr.io/r/base/as.POSIXlt.html)`(``"2026-11-10 07:20:00"``, tz ``=`` ``"America/Recife"``)``,`\
`                    time_window ``=`` ``1``, criteria ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"TIME"``, ``"TRANSFERS"``, ``"WALK"``)``,`\
`                    n_threads ``=`` ``threads``, progress ``=`` ``FALSE``)`\
`op`` ``<-`` `[`as.data.frame`](https://rdrr.io/r/base/as.data.frame.html)`(``res``$``options``)`\
`op``[``, `[`c`](https://rdrr.io/r/base/c.html)`(``"option"``, ``"total_time"``, ``"transfers"``, ``"walk_time"``, ``"arrival_time"``, ``"description"``)``]`\
`#>   option total_time transfers walk_time arrival_time                      description`\
`#> 1      1       72.1         1       5.6     08:32:06 WALK > BUS 1983 > BUS 050 > WALK`\
`#> 2      2       76.1         1       2.1     08:36:06 WALK > BUS 2920 > BUS 050 > WALK`\
`#> 3      3       84.2         0       8.3     08:44:10            WALK > BUS 910 > WALK`

The fastest takes 72 minutes with one transfer; a second pair of buses
takes 76 minutes with 2 minutes on foot instead of 6; a single bus, with
no transfer, takes 84. `res$itineraries` has the legs of each option, as
an `sf` of lines.

![A map of Olinda and Recife with three trips by bus from Praça do
Carmo, in Olinda, to Praça de Boa Viagem, and beside it the time, the
transfers and the walking of each: the fastest, the one with least
walking, and a single bus without a
transfer.](figures/trip-options-rmr.svg)

## What is measured, and what is not

- **Scheduled times, not observed ones.** The timetable of the feed, as
  published: its buses run slower at the peaks (see above), but delays,
  bunching, crowding and trips that do not run are not in it.
- **One day, one hour.** Departures from 07:00 to 07:59 on Tuesday 10
  November 2026. Trips from Praça do Derby leave the centre against the
  morning flow; the access tables are from every home.
- **No fares.** The feed has no rule for the integration at the
  terminals, so nothing here is limited by money or counts it.
- **Rail included, as scheduled.** The metro and the VLT are in the feed
  with their timetables; “buses only” removes them from the routing and
  leaves the bus timetables as they are.
- **Flat ground.** The network was built without an elevation raster.
  The [terrain example](terrain.md) measures what the hills of the
  region add on foot: two minutes or more to the nearest health unit for
  8% of the residents.
- **Driving is free flow**, at the speed limits, with no congestion and
  no parking: an upper bound, drawn for comparison only. Cycling uses
  streets of low traffic stress (`max_lts = 2`).
- **Cells, not addresses.** Every trip starts and ends at the centre of
  a hexagonal cell, 500 m from its neighbours, linked to the nearest
  street; schools and health units are at the coordinates of their
  registers, and the ones without coordinates are left out.
- **Public schools and primary health care units only**, of 2023 and of
  November 2026; residents of the Censo 2022; no jobs (no open grid of
  jobs covers the region).
- **The feed, as it is.** No licence is stated in it; it is used here as
  the consortium publishes it and is not redistributed.

## Variations

- **Another hour or day.** Change `departure`;
  [`check_transit_availability()`](../reference/check_transit_availability.md)
  says what runs on a date.
- **Only some modes.** `mode = c("WALK", "SUBWAY", "TRAM")` routes on
  the rail lines alone; `mode_egress = "BICYCLE"` ends the trip by
  bicycle.
- **Slopes.** `build_network(elevation = "TOBLER")` with a raster in the
  directory, as in the [terrain example](terrain.md).
- **Large matrices on disk.** `output_dir` writes one CSV file per
  origin.

## Sources

Timetables: Grande Recife Consórcio de Transporte Metropolitano, GTFS
feed v2026.10.26, as published at
`https://www.granderecife.pe.gov.br/gtfs/gtfs.zip`. Map data ©
OpenStreetMap contributors, under the ODbL. Boundaries and population ©
IBGE (Censo 2022). Schools: INEP, Catálogo de Escolas 2023, through
geobr. Health units: CNES, Ministério da Saúde. The figures are drawn by
[`site/figures/rmr.R`](https://github.com/StrategicProjects/peabiru-routing/blob/main/site/figures/rmr.R)
from the same results.
