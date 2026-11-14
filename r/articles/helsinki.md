# Example: jobs by public transport in Helsinki

peabiru reads any OpenStreetMap extract and any valid GTFS feed; nothing
in it is particular to Brazil. This example builds the network of the
Helsinki capital region (Helsinki, Espoo, Vantaa and Kauniainen, 1.25
million residents on HSY’s grid) from open data, and measures access to
jobs by public transport at the morning peak.

**Why Helsinki.** The city abroad was chosen by its data: one official
GTFS feed for every mode of the region (HSL’s buses, trams, metro,
commuter trains and ferries), current, under an explicit open licence
(CC BY 4.0); and residents *and* jobs on a 250 m grid, also under CC BY
4.0, from the region’s own environmental services authority (HSY). Of
the other candidates, Lisbon has hills that would have made an elevation
example, but the current feeds of its core, Metro de Lisboa’s and
Carris’s, are listed without a licence in the Mobility Database
catalogue (as downloaded on 2026-11-12; the catalogue gives CC0 only for
older copies, now deprecated, from the Transporlis portal), so it was
dropped rather than used on an assumption. Bogotá, Mexico City and
Montréal were not examined once Helsinki met every requirement.
Helsinki’s terrain is gentle, and this example models no elevation.

The output shown is what the code printed; it was computed on
2026-11-12. The same steps in Python are in the [Python
example](../../python/example-helsinki/).

## The data

| Data | Source | Licence |
|----|----|----|
| Streets | [OpenStreetMap](https://www.openstreetmap.org/copyright), extract of Finland by [Geofabrik](https://download.geofabrik.de/europe/finland.html), dated 2026-11-11 (776 MB, MD5 `ff7c5c52ce53880611989d15378b13af`), cut with osmium to the box 24.45–25.30° E, 60.03–60.42° N | ODbL 1.0 |
| Public transport | [HSL](https://www.hsl.fi/en/opendata) (Helsinki Region Transport), GTFS feed `https://infopalvelut.storage.hsldev.com/gtfs/hsl.zip`, published 2026-11-12 05:42 (feed_version), valid 2026-11-11 to 2027-01-09, downloaded on 2026-11-12 (88 MB, SHA-256 `1cd6f532…7223c`); the feed is replaced every day | CC BY 4.0 (HSL’s open data terms); © HSL 2026 |
| Residents | HSY (Helsinki Region Environmental Services), [*Väestötietoruudukko*](https://www.hsy.fi/en/environmental-information/open-data/avoin-data---sivut/population-grid-of-helsinki-metropolitan-area/) 2025, residents on a 250 m grid (HSY’s page of the dataset: updated 2026-07-08), from HSY’s open WFS `kartta.hsy.fi` | CC BY 4.0, © HSY |
| Jobs | HSY, [*Työpaikkaruudukko*](https://www.hsy.fi/en/environmental-information/open-data/avoin-data---sivut/grid-data-on-enterprises-and-establishments-in-the-helsinki-region/) 2024, jobs (full-time equivalents) on a 250 m grid (updated 2026-08-26), same WFS | CC BY 4.0, © HSY; original source: Statistics Finland’s Business Register |

Three things to know about the grids, each visible in the numbers below.

- **Residents.** Cells with fewer than five residents are left out of
  HSY’s population grid. One of its 5,826 cells is not a place: HSY puts
  the residents it cannot assign to a building in a cell in the sea, at
  the south-east corner of the grid, and in 2025 that cell holds 33,575
  people, 2.6% of the total. It is left out here; routed as an origin it
  would count as 33,575 residents who reach no job.
- **Jobs.** The job grid counts jobs in companies and non-profit
  organisations only, **not public-sector jobs** (schools, hospitals,
  the administration), as full-time equivalents, and gives `-1` for
  cells with three or fewer employees, which are counted as 0 here. So
  “jobs” below are private and third-sector jobs, a lower bound.
- **Coverage.** The job grid covers the wider Helsinki region, 572,915
  jobs; the box of this example keeps the capital region, 486,976 of
  them. Of the rest, 43,423 are in the outer municipalities and 42,516
  are in cells in the sea, south of the box (near 59.94° N, 25.23° E),
  where HSY puts the establishments without a location.

\
[`library`](https://rdrr.io/r/base/library.html)`(`[`peabiru`](https://strategicprojects.github.io/peabiru-routing/r/)`)`\
[`library`](https://rdrr.io/r/base/library.html)`(`[`sf`](https://r-spatial.github.io/sf/)`)`\
\
`root`` ``<-`` `[`Sys.getenv`](https://rdrr.io/r/base/Sys.getenv.html)`(``"PEABIRU_EXAMPLE_DATA"``, ``"example-data"``)`\
`raw`` ``<-`` `[`file.path`](https://rdrr.io/r/base/file.path.html)`(``root``, ``"raw"``)`\
`data_dir`` ``<-`` `[`file.path`](https://rdrr.io/r/base/file.path.html)`(``root``, ``"helsinki"``)`\
[`dir.create`](https://rdrr.io/r/base/files2.html)`(``raw``, recursive ``=`` ``TRUE``, showWarnings ``=`` ``FALSE``)`\
[`options`](https://rdrr.io/r/base/options.html)`(``timeout ``=`` ``3600``)``   ``# the extract of Finland is 776 MB`\
\
`fetch`` ``<-`` ``function``(``url``, ``name`` ``=`` `[`basename`](https://rdrr.io/r/base/basename.html)`(``url``)``)`` ``{`\
`  ``dest`` ``<-`` `[`file.path`](https://rdrr.io/r/base/file.path.html)`(``raw``, ``name``)`\
`  ``if`` ``(``!`[`file.exists`](https://rdrr.io/r/base/files.html)`(``dest``)``)`` `[`download.file`](https://rdrr.io/r/utils/download.file.html)`(``url``, ``dest``, mode ``=`` ``"wb"``, quiet ``=`` ``TRUE``)`\
`  ``dest`\
`}`\
`finland`` ``<-`` ``fetch``(``"https://download.geofabrik.de/europe/finland-261111.osm.pbf"``)`\
[`stopifnot`](https://rdrr.io/r/base/stopifnot.html)`(``tools``::`[`md5sum`](https://rdrr.io/r/tools/md5sum.html)`(``finland``)`` ``==`` ``"ff7c5c52ce53880611989d15378b13af"``)`\
`hsl`` ``<-`` ``fetch``(``"https://infopalvelut.storage.hsldev.com/gtfs/hsl.zip"``)`\
`hsy`` ``<-`` `[`paste0`](https://rdrr.io/r/base/paste.html)`(``"https://kartta.hsy.fi/geoserver/wfs?service=WFS&version=2.0.0"``,`\
`              ``"&request=GetFeature&outputFormat=application/json&srsName=EPSG:4326"``,`\
`              ``"&typeNames=asuminen_ja_maankaytto:"``)`\
`residents_file`` ``<-`` ``fetch``(`[`paste0`](https://rdrr.io/r/base/paste.html)`(``hsy``, ``"Vaestotietoruudukko_2025"``)``, ``"Vaestotietoruudukko_2025.geojson"``)`\
`jobs_file`` ``<-`` ``fetch``(`[`paste0`](https://rdrr.io/r/base/paste.html)`(``hsy``, ``"Tyopaikkaruudukko_2024"``)``, ``"Tyopaikkaruudukko_2024.geojson"``)`

## The network

The streets: the capital region and a margin, cut from the extract of
Finland. The timetables: HSL’s feed holds two months of service, and its
`stop_times.txt` alone is 1.1 GB uncompressed. Routing one weekday
morning needs the trips of that day only, so the feed is cut to the
trips that run on Tuesday 17 November 2026 and the rows of the other
tables they use. The cut copies those rows unchanged (it changes no
value), keeps every stop, and leaves out the tables peabiru does not
read (translations, areas): in the hours analysed the network routes
that day as one built from the whole feed would (that equality was not
run here: the whole feed was not loaded; trips of Monday’s service still
running after midnight are not in the cut), with 681,718 rows of
`stop_times.txt` instead of two months of them.

\
[`dir.create`](https://rdrr.io/r/base/files2.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_dir``, ``"network"``)``, recursive ``=`` ``TRUE``, showWarnings ``=`` ``FALSE``)`\
`status`` ``<-`` `[`system2`](https://rdrr.io/r/base/system2.html)`(``"osmium"``, `[`c`](https://rdrr.io/r/base/c.html)`(``"extract"``, ``"--overwrite"``, ``"-s"``, ``"smart"``,`\
`                              ``"-b"``, ``"24.45,60.03,25.30,60.42"``, ``finland``,`\
`                              ``"-o"``, `[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_dir``, ``"network"``, ``"helsinki.osm.pbf"``)``)``)`\
[`stopifnot`](https://rdrr.io/r/base/stopifnot.html)`(``status`` ``==`` ``0``)`\
\
`` # the trips of a GTFS feed that run on `date`, and the rows of the other ``\
`# tables that belong to them, copied line by line`\
`gtfs_one_day`` ``<-`` ``function``(``src``, ``date``, ``out``)`` ``{`\
`  ``day`` ``<-`` `[`c`](https://rdrr.io/r/base/c.html)`(``"sunday"``, ``"monday"``, ``"tuesday"``, ``"wednesday"``, ``"thursday"``, ``"friday"``,`\
`           ``"saturday"``)``[`[`as.POSIXlt`](https://rdrr.io/r/base/as.POSIXlt.html)`(`[`as.Date`](https://rdrr.io/r/base/as.Date.html)`(``date``, ``"%Y%m%d"``)``)``$``wday`` ``+`` ``1``]`\
`  ``table`` ``<-`` ``function``(``name``)`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`unz`](https://rdrr.io/r/base/connections.html)`(``src``, ``name``)``, colClasses ``=`` ``"character"``,`\
`                                   check.names ``=`` ``FALSE``, fileEncoding ``=`` ``"UTF-8-BOM"``)`\
`  ``cal`` ``<-`` `[`table`](https://rdrr.io/r/base/table.html)`(``"calendar.txt"``)`\
`  ``active`` ``<-`` ``cal``$``service_id``[``cal``$``start_date`` ``<=`` ``date`` ``&`` ``cal``$``end_date`` ``>=`` ``date`` ``&`` ``cal``[[``day``]``]`` ``==`` ``"1"``]`\
`  ``cd`` ``<-`` `[`table`](https://rdrr.io/r/base/table.html)`(``"calendar_dates.txt"``)`\
`  ``cd`` ``<-`` ``cd``[``cd``$``date`` ``==`` ``date``, ``]`\
`  ``active`` ``<-`` `[`union`](https://rdrr.io/r/base/sets.html)`(`[`setdiff`](https://rdrr.io/r/base/sets.html)`(``active``, ``cd``$``service_id``[``cd``$``exception_type`` ``==`` ``"2"``]``)``,`\
`                  ``cd``$``service_id``[``cd``$``exception_type`` ``==`` ``"1"``]``)`\
`  ``trips`` ``<-`` `[`table`](https://rdrr.io/r/base/table.html)`(``"trips.txt"``)`\
`  ``trips`` ``<-`` ``trips``[``trips``$``service_id`` `[`%in%`](https://rdrr.io/r/base/match.html)` ``active``, ``]`\
`  ``keep`` ``<-`` `[`list`](https://rdrr.io/r/base/list.html)`(``trips.txt ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"trip_id"``, ``trips``$``trip_id``)``, stop_times.txt ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"trip_id"``, ``trips``$``trip_id``)``,`\
`               routes.txt ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"route_id"``, `[`unique`](https://rdrr.io/r/base/unique.html)`(``trips``$``route_id``)``)``,`\
`               shapes.txt ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"shape_id"``, `[`unique`](https://rdrr.io/r/base/unique.html)`(``trips``$``shape_id``)``)``,`\
`               calendar.txt ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"service_id"``, ``active``)``, calendar_dates.txt ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"service_id"``, ``active``)``)`\
`  ``tables`` ``<-`` `[`intersect`](https://rdrr.io/r/base/sets.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"agency.txt"``, ``"stops.txt"``, ``"routes.txt"``, ``"trips.txt"``, ``"stop_times.txt"``,`\
`                        ``"calendar.txt"``, ``"calendar_dates.txt"``, ``"frequencies.txt"``, ``"transfers.txt"``,`\
`                        ``"shapes.txt"``, ``"feed_info.txt"``, ``"fare_attributes.txt"``, ``"fare_rules.txt"``)``,`\
`                      `[`unzip`](https://rdrr.io/r/utils/unzip.html)`(``src``, list ``=`` ``TRUE``)``$``Name``)`\
`  ``tmp`` ``<-`` `[`tempfile`](https://rdrr.io/r/base/tempfile.html)`(``)`\
`  `[`dir.create`](https://rdrr.io/r/base/files2.html)`(``tmp``)`\
`  ``for`` ``(``name`` ``in`` ``tables``)`` ``{`\
`    ``con`` ``<-`` `[`unz`](https://rdrr.io/r/base/connections.html)`(``src``, ``name``, open ``=`` ``"rb"``)`\
`    ``to`` ``<-`` `[`file`](https://rdrr.io/r/base/connections.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``tmp``, ``name``)``, ``"wb"``)`\
`    ``header`` ``<-`` `[`readLines`](https://rdrr.io/r/base/readLines.html)`(``con``, n ``=`` ``1``)`\
`    `[`writeLines`](https://rdrr.io/r/base/writeLines.html)`(``header``, ``to``)`\
`    ``k`` ``<-`` ``keep``[[``name``]``]`\
`    ``col`` ``<-`` ``if`` ``(`[`is.null`](https://rdrr.io/r/base/NULL.html)`(``k``)``)`` ``NA`` ``else`` `[`match`](https://rdrr.io/r/base/match.html)`(``k``[``1``]``, `[`strsplit`](https://rdrr.io/r/base/strsplit.html)`(`[`sub`](https://rdrr.io/r/base/grep.html)`(``"^﻿"``, ``""``, `[`sub`](https://rdrr.io/r/base/grep.html)`(``"\r$"``, ``""``, ``header``)``)``, ``","``)``[[``1``]``]``)`\
`    ``repeat`` ``{`\
`      ``lines`` ``<-`` `[`readLines`](https://rdrr.io/r/base/readLines.html)`(``con``, n ``=`` ``500000``)`\
`      ``if`` ``(`[`length`](https://rdrr.io/r/base/length.html)`(``lines``)`` ``==`` ``0``)`` ``break`\
`      ``if`` ``(``!`[`is.na`](https://rdrr.io/r/base/NA.html)`(``col``)``)`` ``{`\
`        ``key`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(``text ``=`` ``lines``, header ``=`` ``FALSE``, colClasses ``=`` ``"character"``)``[[``col``]``]`\
`        `[`stopifnot`](https://rdrr.io/r/base/stopifnot.html)`(`[`length`](https://rdrr.io/r/base/length.html)`(``key``)`` ``==`` `[`length`](https://rdrr.io/r/base/length.html)`(``lines``)``)``   ``# no line breaks inside fields`\
`        ``lines`` ``<-`` ``lines``[``key`` `[`%in%`](https://rdrr.io/r/base/match.html)` ``k``[``-``1``]``]`\
`      ``}`\
`      `[`writeLines`](https://rdrr.io/r/base/writeLines.html)`(``lines``, ``to``)`\
`    ``}`\
`    `[`close`](https://rdrr.io/r/base/connections.html)`(``con``)`\
`    `[`close`](https://rdrr.io/r/base/connections.html)`(``to``)`\
`  ``}`\
`  ``out`` ``<-`` `[`file.path`](https://rdrr.io/r/base/file.path.html)`(`[`normalizePath`](https://rdrr.io/r/base/normalizePath.html)`(`[`dirname`](https://rdrr.io/r/base/basename.html)`(``out``)``)``, `[`basename`](https://rdrr.io/r/base/basename.html)`(``out``)``)`\
`  `[`unlink`](https://rdrr.io/r/base/unlink.html)`(``out``)`\
`  ``old`` ``<-`` `[`setwd`](https://rdrr.io/r/base/getwd.html)`(``tmp``)`\
`  `[`on.exit`](https://rdrr.io/r/base/on.exit.html)`(`[`setwd`](https://rdrr.io/r/base/getwd.html)`(``old``)``)`\
`  `[`stopifnot`](https://rdrr.io/r/base/stopifnot.html)`(`[`zip`](https://rdrr.io/r/utils/zip.html)`(``out``, ``tables``, flags ``=`` ``"-q9X"``)`` ``==`` ``0``)`\
`  `[`c`](https://rdrr.io/r/base/c.html)`(``services ``=`` `[`length`](https://rdrr.io/r/base/length.html)`(``active``)``, trips ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``trips``)``)`\
`}`\
`gtfs_one_day``(``hsl``, ``"20261117"``, `[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_dir``, ``"network"``, ``"hsl.zip"``)``)`\
`#> services    trips `\
`#>      509    24083`\
\
[`get_gtfs_errors`](../reference/get_gtfs_errors.md)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_dir``, ``"network"``)``)`\
`#> # A tibble: 0 × 6`\
`#> # ℹ 6 variables: file <lgl>, line <lgl>, type <lgl>, field <lgl>, id <lgl>,`\
`#> #   priority <lgl>`\
`threads`` ``<-`` ``4``   ``# of this laptop's ten cores`\
`build_time`` ``<-`` `[`system.time`](https://rdrr.io/r/base/system.time.html)`(`\
`  ``network`` ``<-`` `[`build_network`](../reference/build_network.md)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_dir``, ``"network"``)``, n_threads ``=`` ``threads``)`\
`)``[[``"elapsed"``]``]`\
`network`\
`#> <peabiru_network>`\
`#>   data path: example-data/helsinki/network`\
`#>   streets:   641,401 vertices, 1,662,710 edges`\
`#>   transit:   1 feed(s) (hsl), 8,404 stops, 911 patterns, 509 services`\
`#>   elevation: NONE`\
`build_time`\
`#> [1] 13.649`

The cut feed passes peabiru’s GTFS checks without a single error (the
whole feed was not checked). Building the network took 14 seconds here,
with four threads on a laptop with other work running (the timings on
this page are indications); the R process that knitted this page, from
the cut of the feed to the last table, peaked at 3.9 GB of memory.

## Residents and jobs

Each 250 m cell is placed at its centre; the residents are the origins,
the jobs the destinations.

\
`centres`` ``<-`` ``function``(``x``)`` ``{`\
`  `[`st_coordinates`](https://r-spatial.github.io/sf/reference/st_coordinates.html)`(`[`st_transform`](https://r-spatial.github.io/sf/reference/st_transform.html)`(`[`st_centroid`](https://r-spatial.github.io/sf/reference/geos_unary.html)`(`[`st_geometry`](https://r-spatial.github.io/sf/reference/st_geometry.html)`(`[`st_transform`](https://r-spatial.github.io/sf/reference/st_transform.html)`(``x``, ``3067``)``)``)``, ``4326``)``)`\
`}`\
`residents`` ``<-`` `[`st_read`](https://r-spatial.github.io/sf/reference/st_read.html)`(``residents_file``, quiet ``=`` ``TRUE``)`\
`xy`` ``<-`` ``centres``(``residents``)`\
`cells`` ``<-`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` `[`as.character`](https://rdrr.io/r/base/character.html)`(``residents``$``index``)``, lon ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``xy``[``, ``1``]``, ``6``)``,`\
`                    lat ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``xy``[``, ``2``]``, ``6``)``, population ``=`` ``residents``$``asukkaita``)`\
`# the cell in the sea with the residents without a building`\
`sea`` ``<-`` `[`which.max`](https://rdrr.io/r/base/which.min.html)`(``cells``$``population``)`\
[`stopifnot`](https://rdrr.io/r/base/stopifnot.html)`(``cells``$``lat``[``sea``]`` ``==`` `[`min`](https://rdrr.io/r/base/Extremes.html)`(``cells``$``lat``)``, ``cells``$``lon``[``sea``]`` ``==`` `[`max`](https://rdrr.io/r/base/Extremes.html)`(``cells``$``lon``)``)`\
`cells``[``sea``, ``]`\
`#>         id      lon      lat population`\
`#> 5826 27699 25.25355 60.05407      33575`\
`cells`` ``<-`` ``cells``[``-``sea``, ``]`\
\
`jobs`` ``<-`` `[`st_read`](https://r-spatial.github.io/sf/reference/st_read.html)`(``jobs_file``, quiet ``=`` ``TRUE``)`\
`xy`` ``<-`` ``centres``(``jobs``)`\
`jobs`` ``<-`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` ``jobs``$``id``, lon ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``xy``[``, ``1``]``, ``6``)``, lat ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``xy``[``, ``2``]``, ``6``)``,`\
`                   jobs ``=`` `[`pmax`](https://rdrr.io/r/base/Extremes.html)`(``jobs``$``tphklkm``, ``0``)``, suppressed ``=`` ``jobs``$``tphklkm`` ``<`` ``0``)`\
[`c`](https://rdrr.io/r/base/c.html)`(``jobs_in_the_grid ``=`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``jobs``$``jobs``)``, without_a_location ``=`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``jobs``$``jobs``[``jobs``$``lat`` ``<`` ``60``]``)``)`\
`#>   jobs_in_the_grid without_a_location `\
`#>             572915              42516`\
`jobs`` ``<-`` ``jobs``[``jobs``$``lon`` ``>`` ``24.45`` ``&`` ``jobs``$``lon`` ``<`` ``25.30`` ``&`` ``jobs``$``lat`` ``>`` ``60.03`` ``&`` ``jobs``$``lat`` ``<`` ``60.42``, ``]`\
[`c`](https://rdrr.io/r/base/c.html)`(``cells ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``cells``)``, residents ``=`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``cells``$``population``)``,`\
`  job_cells ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``jobs``)``, jobs ``=`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``jobs``$``jobs``)``, suppressed_job_cells ``=`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``jobs``$``suppressed``)``)`\
`#>                cells            residents            job_cells `\
`#>                 5825              1253793                 8578 `\
`#>                 jobs suppressed_job_cells `\
`#>               486976                 4094`

## Jobs within reach at the morning peak

Walking and public transport, departures every minute from 07:30 to
08:30 on Tuesday 17 November 2026, the median travel time of each pair:

\
`peak`` ``<-`` `[`as.POSIXct`](https://rdrr.io/r/base/as.POSIXlt.html)`(``"2026-11-17 07:30:00"``, tz ``=`` ``"Europe/Helsinki"``)`\
`t`` ``<-`` `[`system.time`](https://rdrr.io/r/base/system.time.html)`(`\
`  ``access`` ``<-`` `[`accessibility`](../reference/accessibility.md)`(``network``, ``cells``, ``jobs``, opportunities_colnames ``=`` ``"jobs"``,`\
`                          mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``, departure_datetime ``=`` ``peak``,`\
`                          time_window ``=`` ``60``, cutoffs ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``30``, ``45``)``, n_threads ``=`` ``threads``)`\
`)`\
`t``[[``"elapsed"``]``]`\
`#> [1] 13.087`\
`access``$``population`` ``<-`` ``cells``$``population``[`[`match`](https://rdrr.io/r/base/match.html)`(``access``$``id``, ``cells``$``id``)``]`\
`total`` ``<-`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``jobs``$``jobs``)`\
[`sapply`](https://rdrr.io/r/base/lapply.html)`(`[`split`](https://rdrr.io/r/base/split.html)`(``access``, ``access``$``cutoff``)``, ``function``(``a``)`` `[`c`](https://rdrr.io/r/base/c.html)`(`\
`  average_jobs ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``a``$``accessibility``, ``a``$``population``)``)``,`\
`  average_share ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``a``$``accessibility``, ``a``$``population``)`` ``/`` ``total``, ``3``)``,`\
`  residents_reaching_half ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`sum`](https://rdrr.io/r/base/sum.html)`(``a``$``population``[``a``$``accessibility`` ``>=`` ``total`` ``/`` ``2``]``)`` ``/`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``a``$``population``)``, ``3``)`\
`)``)`\
`#>                                30         45`\
`#> average_jobs            41169.000 151537.000`\
`#> average_share               0.085      0.311`\
`#> residents_reaching_half     0.000      0.223`

5,825 origins, 8,578 destinations and 60 departure minutes take about 13
seconds.

![A map of the Helsinki capital region covered by 250 m cells coloured
by the number of jobs reachable within 45 minutes by public transport at
the morning peak: the centre of Helsinki and the cells along the metro
and railway lines reach the most.](figures/helsinki-jobs.svg)

## Isochrones from the Central Station

\
`station`` ``<-`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` ``"Helsinki Central Station"``, lon ``=`` ``24.94124``, lat ``=`` ``60.17128``)`\
`s2`` ``<-`` `[`sf_use_s2`](https://r-spatial.github.io/sf/reference/s2.html)`(``FALSE``)``   ``# see the note below`\
`iso`` ``<-`` `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`list`](https://rdrr.io/r/base/list.html)`(``"WALK"``, ``"BICYCLE"``, ``"CAR"``, `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``)``, ``function``(``m``)`` ``{`\
`  ``i`` ``<-`` `[`isochrone`](../reference/isochrone.md)`(``network``, ``station``, mode ``=`` ``m``, cutoffs ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``15``, ``30``, ``45``)``, zoom ``=`` ``10``,`\
`                 departure_datetime ``=`` ``peak``, time_window ``=`` ``60``, n_threads ``=`` ``threads``)`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``mode ``=`` `[`paste`](https://rdrr.io/r/base/paste.html)`(``m``, collapse ``=`` ``"+"``)``, minutes ``=`` ``i``$``isochrone``,`\
`             km2 ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`as.numeric`](https://rdrr.io/r/base/numeric.html)`(`[`st_area`](https://r-spatial.github.io/sf/reference/geos_measures.html)`(`[`st_transform`](https://r-spatial.github.io/sf/reference/st_transform.html)`(``i``, ``3067``)``)``)`` ``/`` ``1e6``, ``1``)``)`\
`}``)``)`\
[`sf_use_s2`](https://r-spatial.github.io/sf/reference/s2.html)`(``s2``)`\
[`xtabs`](https://rdrr.io/r/stats/xtabs.html)`(``km2`` ``~`` ``mode`` ``+`` ``minutes``, data ``=`` ``iso``)`\
`#>               minutes`\
`#> mode               15     30     45`\
`#>   BICYCLE        10.2   38.0   84.9`\
`#>   CAR            26.0  478.3 1312.9`\
`#>   WALK            1.6    6.7   14.3`\
`#>   WALK+TRANSIT    3.7   54.0  238.5`

Driving is free flow: no congestion, so its isochrones are an upper
bound at the morning peak. The 45-minute areas by car and by public
transport run past the box cut from OpenStreetMap, where the network
ends.

Why `sf_use_s2(FALSE)`: [`isochrone()`](../reference/isochrone.md)
passes each polygon through
[`sf::st_make_valid()`](https://r-spatial.github.io/sf/reference/valid.html),
and that repair depends on whether sf is using spherical geometry (s2,
its default). For eleven of these twelve polygons the area is the same
either way; the 30-minute driving polygon, which has 114 parts and 217
holes, comes out of the s2 repair at 523.8 km² instead of 478.3, more
than its outer rings enclose. With s2 off, the areas are those of the
bands as contoured, and those of the Python example. With s2 on,
[`isochrone()`](../reference/isochrone.md) notices that the repair
changed the area, repairs that origin’s polygons on the plane and warns
(see [`?isochrone`](../reference/isochrone.md)), so here switching s2
off only avoids the warning.

![Four maps of the Helsinki capital region with the areas reached from
Helsinki Central Station within 15, 30 and 45 minutes walking, cycling,
driving and by public transport.](figures/helsinki-isochrones.svg)

## The options for one trip

From Itäkeskus, in the east, to the Aalto University campus at Otaniemi,
in Espoo: the trips worth considering by travel time, number of
transfers and walking time, leaving between 07:30 and 07:40.

\
`pair`` ``<-`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"Itäkeskus"``, ``"Otaniemi"``)``, lon ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``25.0815``, ``24.8268``)``,`\
`                   lat ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``60.2102``, ``60.1847``)``)`\
`opts`` ``<-`` `[`trip_options`](../reference/trip_options.md)`(``network``, ``pair``[``1``, ``]``, ``pair``[``2``, ``]``, departure_datetime ``=`` ``peak``,`\
`                     criteria ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"TIME"``, ``"TRANSFERS"``, ``"WALK"``)``, itineraries ``=`` ``FALSE``)`\
[`as.data.frame`](https://rdrr.io/r/base/as.data.frame.html)`(``opts``$``options``[``, `[`c`](https://rdrr.io/r/base/c.html)`(``"option"``, ``"total_time"``, ``"transfers"``, ``"walk_time"``,`\
`                               ``"departure_time"``, ``"description"``)``]``)`\
`#>   option total_time transfers walk_time departure_time`\
`#> 1      1       39.3         0      10.7       07:30:00`\
`#> 2      2       39.3         0      10.7       07:33:00`\
`#> 3      3       70.3         2      10.3       07:39:00`\
`#> 4      4       72.0         1       9.3       07:31:00`\
`#>                                  description`\
`#> 1                    WALK > SUBWAY M2 > WALK`\
`#> 2                    WALK > SUBWAY M1 > WALK`\
`#> 3 WALK > BUS 97V > BUS 82 > SUBWAY M2 > WALK`\
`#> 4            WALK > BUS 500 > BUS 510 > WALK`

The metro takes 39 minutes without a transfer, with a departure on each
of its two lines in those ten minutes; the two options that start by bus
take half an hour longer and are listed only because they walk a little
less.

## Variations

- **The whole feed.** On a machine with memory to spare, put HSL’s feed
  in the network directory as it is: the routing on the chosen day
  should be the same (not run here).
- **Another region.** Any region with a valid GTFS feed and an
  OpenStreetMap extract works the same way (check the feed’s licence);
  the HSY grids cover the Helsinki region only.
- **Scenarios.** The what-if arguments of the [Porto Alegre
  example](scenarios-porto-alegre.md) work here too, for instance
  `new_lts` on a planned cycle route.

Map data © OpenStreetMap contributors, under the ODbL; timetables © HSL
2026, CC BY 4.0; residents and jobs © HSY, CC BY 4.0 (jobs: from
Statistics Finland’s Business Register).
