# Example: terrain, in Pernambuco and in Salvador

The [Recife and Caruaru example](recife-caruaru.md) routes on streets
without elevation. This one adds an elevation raster and asks what it
changes, on foot and by bicycle:

- in **Pernambuco**: Recife, Olinda, Jaboatão dos Guararapes, Caruaru,
  Garanhuns and Petrolina, each on its own network, and the **Região
  Metropolitana do Recife** (14 municipalities, 3.7 million people) as
  one network, where the question is also how many residents have their
  nearest school or health unit across a municipal border;
- in **Salvador**, in Bahia, the one city from outside Pernambuco, taken
  as the steepest case: an escarpment splits its upper and lower towns.
  It is drawn beside Olinda, the hill town of the Recife metropolitan
  region, whose historic centre sits above a flat coastal strip.

The short version: slopes matter, less than a first run suggests. The
open elevation model at 30 m reads the tops of buildings, and on a flat
street between towers that alone costs as much as the hills do
elsewhere. The page measures that before anything else, and then uses
the 90 m version of the model, whose results are on the low side.

The data steps are those of the Recife and Caruaru example, kept as
functions in
[`site/figures/pernambuco-data.R`](https://github.com/StrategicProjects/peabiru-routing/blob/main/site/figures/pernambuco-data.R),
which this page sources. The output shown is what the code prints; it
was computed on 2026-11-12.

**Streets only.** No open GTFS feed was found for Recife or for
Salvador: neither Pernambuco nor Bahia has a feed in the Mobility
Database catalogue (read on 2026-11-12), and a search for “gtfs” on the
open-data portal of the city of Recife returns nothing. So there is no
public transport here. (Grande Recife publishes a feed on its own site,
found after this page was written: the [Recife metropolitan region
example](recife-metropolitan.md) routes on it.)

## The data

| Data | Source | Licence |
|----|----|----|
| Streets | [OpenStreetMap](https://www.openstreetmap.org/copyright), extract of the Nordeste region by [Geofabrik](https://download.geofabrik.de/south-america/brazil/nordeste.html), dated 2026-11-01 (445 MB, MD5 `93318fcddd85a53e8386eaa93588847c`) | ODbL 1.0 |
| Elevation | [Copernicus DEM](https://registry.opendata.aws/copernicus-dem/), GLO-90 (3 arc seconds, about 90 m) and, for comparison, GLO-30 (1 arc second): a surface model, 2021 release, files of 2022-05-09, read from the tiles on the AWS open data registry on 2026-11-12 | [Copernicus DEM licence](https://dataspace.copernicus.eu/explore-data/data-collections/copernicus-contributing-missions/collections-description/COP-DEM): free of charge, with the attribution at the end of this page |
| Municipal boundaries | [IBGE, malhas municipais](https://servicodados.ibge.gov.br/api/docs/malhas?versao=3) (API, version 3) | public data, IBGE |
| Population | [IBGE, Censo Demográfico 2022, census tracts with their population](https://ftp.ibge.gov.br/Censos/Censo_Demografico_2022/Agregados_por_Setores_Censitarios/malha_com_atributos/setores/gpkg/UF/), variable `v0001`, files of Pernambuco (44 MB) and Bahia (118 MB) | public data, IBGE |
| Schools | INEP, Catálogo de Escolas 2023, with the coordinates of each school, as published by Ipea’s [geobr](https://ipeagit.github.io/geobr/) (`schools_2023`, 101 MB for Brazil) | public data, INEP; geobr is MIT |
| Health units | [CNES](https://cnes.datasus.gov.br/), Ministério da Saúde, through [OpenDataSUS](https://opendatasus.saude.gov.br/) (`cnes_estabelecimentos_csv.zip`, 57 MB for Brazil, updated daily; downloaded on 2026-11-12) | open data, Ministério da Saúde |

`prepare_city()` cuts the streets of a municipality (or of the 14 of the
metropolitan region) with a buffer around it, puts the census population
on a hexagonal grid (cells 500 m apart; 250 m in the small municipality
of Olinda) and lists the public schools in operation and the primary
health care units (CNES types 1 and 2) in the cut area. `prepare_dem()`
crops the elevation model over the box of those streets, on the model’s
own grid. Both need [osmium](https://osmcode.org/osmium-tool/); the crop
uses GDAL through sf, and the checks below use terra.

\
[`library`](https://rdrr.io/r/base/library.html)`(`[`peabiru`](https://strategicprojects.github.io/peabiru-routing/r/)`)`\
[`library`](https://rdrr.io/r/base/library.html)`(`[`sf`](https://r-spatial.github.io/sf/)`)`\
[`options`](https://rdrr.io/r/base/options.html)`(``timeout ``=`` ``3600``)`\
\
`# the data steps of the Recife and Caruaru example, as functions:`\
`# prepare_city() and prepare_dem()`\
`pipeline`` ``<-`` `[`Sys.getenv`](https://rdrr.io/r/base/Sys.getenv.html)`(``"PEABIRU_PE_SCRIPT"``, `[`paste0`](https://rdrr.io/r/base/paste.html)`(`\
`  ``"https://raw.githubusercontent.com/StrategicProjects/peabiru-routing/main/"``,`\
`  ``"site/figures/pernambuco-data.R"``)``)`\
[`Sys.setenv`](https://rdrr.io/r/base/Sys.setenv.html)`(``PEABIRU_PE_CACHE ``=`` `[`Sys.getenv`](https://rdrr.io/r/base/Sys.getenv.html)`(``"PEABIRU_EXAMPLE_DATA"``, ``"pe-data"``)``)`\
[`source`](https://rdrr.io/r/base/source.html)`(``pipeline``)`\
`data_dir`` ``<-`` ``pe_cache``(``)`\
\
`keys`` ``<-`` `[`c`](https://rdrr.io/r/base/c.html)`(``"recife"``, ``"olinda"``, ``"jaboatao"``, ``"caruaru"``, ``"garanhuns"``, ``"petrolina"``,`\
`          ``"rmr"``, ``"salvador"``)`\
`summary_of`` ``<-`` ``function``(``k``)`` ``{`\
`  ``dir`` ``<-`` ``prepare_city``(``k``)`\
`  ``prepare_dem``(``k``, arcsec ``=`` ``3``)``       ``# GLO-90, into <city>/network-dem90/`\
`  ``cells`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``dir``, ``"cells.csv"``)``)`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``key ``=`` ``k``,`\
`             extract_MB ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`file.size`](https://rdrr.io/r/base/file.info.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``dir``, ``"network"``, `[`paste0`](https://rdrr.io/r/base/paste.html)`(``k``, ``".osm.pbf"``)``)``)`` ``/`` ``1e6``, ``1``)``,`\
`             dem_MB ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`file.size`](https://rdrr.io/r/base/file.info.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``dir``, ``"network-dem90"``, `[`paste0`](https://rdrr.io/r/base/paste.html)`(``k``, ``"_dem90.tif"``)``)``)`` ``/`` ``1e6``, ``1``)``,`\
`             cells ``=`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``cells``$``population`` ``>`` ``0``)``,`\
`             people ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`sum`](https://rdrr.io/r/base/sum.html)`(``cells``$``population``)``)``,`\
`             schools ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(`[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``dir``, ``"schools.csv"``)``)``)``,`\
`             health_units ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(`[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``dir``, ``"health.csv"``)``)``)``)`\
`}`\
[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(``keys``, ``summary_of``)``)`\
`#>         key extract_MB dem_MB cells  people schools health_units`\
`#> 1    recife       16.0    0.6  1107 1488797     937          511`\
`#> 2    olinda        3.4    0.1   857  349970     260          160`\
`#> 3  jaboatao        8.2    0.6  1286  644037     416          238`\
`#> 4   caruaru        1.9    1.1  4489  378048     216          102`\
`#> 5 garanhuns        1.0    0.6  2294  142506     133           90`\
`#> 6 petrolina        3.1   10.7 21363  409608     239          120`\
`#> 7       rmr       21.2    1.9 13096 3730664    1203          691`\
`#> 8  salvador        9.0    0.3  1567 2418518     652          202`

`people` is the population of the Censo 2022 in the cells; `cells`, the
cells with residents. Schools and health units include those in the
buffer around each area (10 km around Recife, 3 km around the
metropolitan region, Olinda and Salvador, 5 km around the others), so a
trip can end across the border. Petrolina faces Juazeiro, in Bahia,
across the São Francisco: its health units are read from the CNES rows
of both states. In Salvador’s buffer there is mostly sea.

## The elevation model, checked against the ground

The Copernicus DEM is a *surface* model: radar measured the top of what
it saw, roofs and tree crowns included. It comes at 1 arc second
(GLO-30, about 30 m) and, averaged by its publisher, at 3 (GLO-90).
peabiru samples the raster every 10 m along each street, at the nearest
pixel, so a step between two pixels becomes a slope on one 10 m stretch:
a real hillside is read as a staircase, and a roof next to a street as a
wall. Three checks, before any routing.

**Voids, the sea.** Neither raster has a void over these cities. The sea
has no tiles and is taken as 0 m, as the registry’s documentation says;
a few pixels (water, mangrove) are below zero, down to -17 m.

\
`dem_of`` ``<-`` ``function``(``k``, ``arcsec`` ``=`` ``3``)`` ``{`\
`  ``prepare_dem``(``k``, arcsec ``=`` ``arcsec``)`\
`  ``tag`` ``<-`` ``if`` ``(``arcsec`` ``==`` ``1``)`` ``""`` ``else`` ``"90"`\
`  ``terra``::`[`rast`](https://rspatial.github.io/terra/reference/rast.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_dir``, ``k``, `[`paste0`](https://rdrr.io/r/base/paste.html)`(``"network-dem"``, ``tag``)``, `[`paste0`](https://rdrr.io/r/base/paste.html)`(``k``, ``"_dem"``, ``tag``, ``".tif"``)``)``)`\
`}`\
`r`` ``<-`` ``dem_of``(``"rmr"``)`\
`v`` ``<-`` ``terra``::`[`values`](https://rspatial.github.io/terra/reference/values.html)`(``r``, mat ``=`` ``FALSE``)`\
[`c`](https://rdrr.io/r/base/c.html)`(``res_arcsec ``=`` ``terra``::`[`res`](https://rspatial.github.io/terra/reference/dimensions.html)`(``r``)``[``1``]`` ``*`` ``3600``, voids ``=`` `[`sum`](https://rdrr.io/r/base/sum.html)`(`[`is.na`](https://rdrr.io/r/base/NA.html)`(``v``)`` ``|`` ``v`` ``<`` ``-``1000``)``,`\
`  sea_share ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`mean`](https://rdrr.io/r/base/mean.html)`(``v`` ``==`` ``0``)``, ``3``)``, below_0_share ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`mean`](https://rdrr.io/r/base/mean.html)`(``v`` ``<`` ``0``)``, ``4``)``,`\
`  min_m ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`min`](https://rdrr.io/r/base/Extremes.html)`(``v``)``, ``1``)``, max_m ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`max`](https://rdrr.io/r/base/Extremes.html)`(``v``)``, ``1``)``)`\
`#>    res_arcsec         voids     sea_share below_0_share         min_m `\
`#>        3.0000        0.0000        0.1950        0.0014      -17.1000 `\
`#>         max_m `\
`#>      446.4000`

**A flat district.** Boa Viagem, in Recife, is a coastal plain a few
metres above the sea, lined by residential towers. Its streets, sampled
as peabiru samples them:

\
`# street samples every 10 m at the nearest pixel`\
`streets_z`` ``<-`` ``function``(``k``, ``arcsec`` ``=`` ``3``, ``bridges`` ``=`` ``FALSE``)`` ``{`\
`  ``lines`` ``<-`` `[`suppressWarnings`](https://rdrr.io/r/base/warning.html)`(`[`st_read`](https://r-spatial.github.io/sf/reference/st_read.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_dir``, ``k``, ``"network"``, `[`paste0`](https://rdrr.io/r/base/paste.html)`(``k``, ``".osm.pbf"``)``)``,`\
`                                    ``"lines"``, quiet ``=`` ``TRUE``)``)`\
`  ``lines`` ``<-`` ``lines``[``!`[`is.na`](https://rdrr.io/r/base/NA.html)`(``lines``$``highway``)``, ``]`\
`  ``is_bridge`` ``<-`` `[`grepl`](https://rdrr.io/r/base/grep.html)`(``'"bridge"=>"(yes|viaduct)"'``, ``lines``$``other_tags``)`\
`  ``lines`` ``<-`` `[`st_transform`](https://r-spatial.github.io/sf/reference/st_transform.html)`(``lines``[``if`` ``(``bridges``)`` ``is_bridge`` ``else`` ``!``is_bridge``, ``]``, ``31985``)`\
`  ``lines`` ``<-`` ``lines``[`[`as.numeric`](https://rdrr.io/r/base/numeric.html)`(`[`st_length`](https://r-spatial.github.io/sf/reference/geos_measures.html)`(``lines``)``)`` ``>=`` ``20``, ``]`\
`  ``pts`` ``<-`` `[`st_line_sample`](https://r-spatial.github.io/sf/reference/st_line_sample.html)`(``lines``, density ``=`` ``1`` ``/`` ``10``, type ``=`` ``"regular"``)`\
`  ``n`` ``<-`` `[`vapply`](https://rdrr.io/r/base/lapply.html)`(``pts``, ``function``(``p``)`` ``if`` ``(`[`st_is_empty`](https://r-spatial.github.io/sf/reference/geos_query.html)`(``p``)``)`` ``0L`` ``else`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(`[`st_coordinates`](https://r-spatial.github.io/sf/reference/st_coordinates.html)`(``p``)``)``, ``1L``)`\
`  ``ll`` ``<-`` `[`sf_project`](https://r-spatial.github.io/sf/reference/sf_project.html)`(`[`st_crs`](https://r-spatial.github.io/sf/reference/st_crs.html)`(``31985``)``, `[`st_crs`](https://r-spatial.github.io/sf/reference/st_crs.html)`(``4326``)``, `[`st_coordinates`](https://r-spatial.github.io/sf/reference/st_coordinates.html)`(``pts``)``[``, ``1``:``2``]``)`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``way ``=`` `[`rep`](https://rdrr.io/r/base/rep.html)`(``lines``$``osm_id``, ``n``)``, lon ``=`` ``ll``[``, ``1``]``, lat ``=`` ``ll``[``, ``2``]``,`\
`             z ``=`` ``terra``::`[`extract`](https://rspatial.github.io/terra/reference/extract.html)`(``dem_of``(``k``, ``arcsec``)``, ``ll``, method ``=`` ``"simple"``)``[``, ``1``]``)`\
`}`\
`in_boa_viagem`` ``<-`` ``function``(``lon``, ``lat``)`` ``lon`` ``>`` ``-``34.925`` ``&`` ``lon`` ``<`` ``-``34.885`` ``&`` ``lat`` ``>`` ``-``8.145`` ``&`` ``lat`` ``<`` ``-``8.09`\
\
[`sapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"GLO-30"`` ``=`` ``1``, ``"GLO-90"`` ``=`` ``3``)``, ``function``(``arcsec``)`` ``{`\
`  ``s`` ``<-`` ``streets_z``(``"recife"``, ``arcsec``)`\
`  ``bv`` ``<-`` ``s``[``in_boa_viagem``(``s``$``lon``, ``s``$``lat``)``, ``]`\
`  ``same_way`` ``<-`` ``bv``$``way``[``-``1``]`` ``==`` ``bv``$``way``[``-`[`nrow`](https://rdrr.io/r/base/nrow.html)`(``bv``)``]`\
`  `[`c`](https://rdrr.io/r/base/c.html)`(``samples ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``bv``)``, `[`round`](https://rdrr.io/r/base/Round.html)`(`[`quantile`](https://rdrr.io/r/stats/quantile.html)`(``bv``$``z``, `[`c`](https://rdrr.io/r/base/c.html)`(``0.1``, ``0.5``, ``0.9``, ``0.99``)``)``, ``1``)``,`\
`    steps_over_2m ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`mean`](https://rdrr.io/r/base/mean.html)`(`[`abs`](https://rdrr.io/r/base/MathFun.html)`(`[`diff`](https://rdrr.io/r/base/diff.html)`(``bv``$``z``)``)``[``same_way``]`` ``>`` ``2``)``, ``3``)``)`\
`}``)`\
`#>                  GLO-30    GLO-90`\
`#> samples       23528.000 23528.000`\
`#> 10%               2.500     3.100`\
`#> 50%               4.500     4.700`\
`#> 90%               9.200     9.100`\
`#> 99%              13.400    12.900`\
`#> steps_over_2m     0.075     0.025`

Half of the samples read 4.5 m or more and one in ten more than 9 m, on
a plain by the sea: the higher readings are roofs and trees. On GLO-30,
7.5% of the 10 m steps along a street change by more than 2 m, a slope
of 20% that is not there; on GLO-90, 2.5%. What that does to travel
times is measured [below](#which-raster-a-flat-district-as-control).

**Bridges.** A surface model should see the deck of a bridge; a 90 m
pixel mostly sees what is under it. For each bridge or viaduct of the
metropolitan region, how far the lowest sample falls below the average
of its two ends:

\
`b`` ``<-`` ``streets_z``(``"rmr"``, bridges ``=`` ``TRUE``)`\
`sag`` ``<-`` `[`sapply`](https://rdrr.io/r/base/lapply.html)`(`[`split`](https://rdrr.io/r/base/split.html)`(``b``$``z``, ``b``$``way``)``, ``function``(``z``)`` ``{`\
`  ``if`` ``(`[`length`](https://rdrr.io/r/base/length.html)`(``z``)`` ``<`` ``3``)`` ``NA`` ``else`` `[`mean`](https://rdrr.io/r/base/mean.html)`(``z``[`[`c`](https://rdrr.io/r/base/c.html)`(``1``, `[`length`](https://rdrr.io/r/base/length.html)`(``z``)``)``]``)`` ``-`` `[`min`](https://rdrr.io/r/base/Extremes.html)`(``z``[``-`[`c`](https://rdrr.io/r/base/c.html)`(``1``, `[`length`](https://rdrr.io/r/base/length.html)`(``z``)``)``]``)`\
`}``)`\
`sag`` ``<-`` ``sag``[``!`[`is.na`](https://rdrr.io/r/base/NA.html)`(``sag``)``]`\
[`c`](https://rdrr.io/r/base/c.html)`(``bridges ``=`` `[`length`](https://rdrr.io/r/base/length.html)`(``sag``)``, dip_over_2m ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`mean`](https://rdrr.io/r/base/mean.html)`(``sag`` ``>`` ``2``)``, ``3``)``,`\
`  dip_over_5m ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`mean`](https://rdrr.io/r/base/mean.html)`(``sag`` ``>`` ``5``)``, ``3``)``, worst_m ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`max`](https://rdrr.io/r/base/Extremes.html)`(``sag``)``, ``1``)``)`\
`#>     bridges dip_over_2m dip_over_5m     worst_m `\
`#>     358.000       0.209       0.036       8.700`

One bridge in five dips by more than 2 m in the raster, and a few by
more than 5 m (the viaducts of Avenida Caxangá, in Recife, by almost 9
m): those cross a valley that the deck does not go down into, and
walking or cycling over them is costed as if it did. They are few among
369 thousand street edges, but an analysis of one corridor should look
at its bridges.

## Networks with and without elevation

The same extract is read from `network/`, with no raster, and from
`network-dem90/`, where the raster turns slope costs on (`"TOBLER"`, the
default, or `"MINETTI"`); `"TOBLER30"` below is Tobler on GLO-30, from
`network-dem/`. Car times do not change with elevation.

\
`networks`` ``<-`` `[`list`](https://rdrr.io/r/base/list.html)`(``)`\
`network`` ``<-`` ``function``(``k``, ``elevation`` ``=`` ``"NONE"``)`` ``{`\
`  ``id`` ``<-`` `[`paste`](https://rdrr.io/r/base/paste.html)`(``k``, ``elevation``)`\
`  ``if`` ``(`[`is.null`](https://rdrr.io/r/base/NULL.html)`(``networks``[[``id``]``]``)``)`` ``{`\
`    ``if`` ``(``elevation`` ``==`` ``"TOBLER30"``)`` ``prepare_dem``(``k``)``        ``# GLO-30, into network-dem/`\
`    ``dir`` ``<-`` `[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_dir``, ``k``, `[`switch`](https://rdrr.io/r/base/switch.html)`(``elevation``, NONE ``=`` ``"network"``, TOBLER30 ``=`` ``"network-dem"``,`\
`                                         ``"network-dem90"``)``)`\
`    ``networks``[[``id``]``]`` ``<<-`` `[`build_network`](../reference/build_network.md)`(``dir``, elevation ``=`` `[`sub`](https://rdrr.io/r/base/grep.html)`(``"30$"``, ``""``, ``elevation``)``)`\
`  ``}`\
`  ``networks``[[``id``]``]`\
`}`\
[`system.time`](https://rdrr.io/r/base/system.time.html)`(``network``(``"rmr"``)``)``[[``"elapsed"``]``]`\
`#> [1] 0.588`\
[`system.time`](https://rdrr.io/r/base/system.time.html)`(``network``(``"rmr"``, ``"TOBLER"``)``)``[[``"elapsed"``]``]`\
`#> [1] 0.638`\
`network``(``"rmr"``, ``"TOBLER"``)`\
`#> <peabiru_network>`\
`#>   data path: pe-data/rmr/network-dem90`\
`#>   streets:   140,267 vertices, 368,800 edges`\
`#>   transit:   none (no GTFS feed)`\
`#>   elevation: TOBLER`

The metropolitan region is one extract of 21 MB and builds in about a
second, with or without the raster (the times above are of a busy
laptop, with the default number of threads).

**For the browser demo, it is too large.** The demo of the documentation
site takes extracts under 3 MB (Recife’s municipality is 2.8 MB).
Stripped the same way (no buildings, no object metadata), the 14
municipalities are 8.9 MB; with nothing but the streets and the turn
restrictions, 5.3 MB. So the region is not in the demo; a dashboard
would load it on a server, or by municipality.

## Which raster: a flat district as control

The time from every cell to the nearest school and health unit is the
measure of the Recife and Caruaru example. In Boa Viagem slopes should
cost nothing, so whatever a raster adds there is the raster’s own doing:

\
`grid_of`` ``<-`` ``function``(``k``)`` ``{`\
`  ``g`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_dir``, ``k``, ``"cells.csv"``)``, colClasses ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``id ``=`` ``"character"``)``)`\
`  ``g``[``g``$``population`` ``>`` ``0``, ``]`\
`}`\
`units_of`` ``<-`` ``function``(``k``, ``what``)`` ``{`\
`  `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_dir``, ``k``, `[`paste0`](https://rdrr.io/r/base/paste.html)`(``what``, ``".csv"``)``)``, colClasses ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``id ``=`` ``"character"``)``)`\
`}`\
`# the time from each cell to the nearest school and health unit, and which one`\
`nearest`` ``<-`` ``function``(``k``, ``elevation``, ``mode``, ``max_lts`` ``=`` ``2``)`` ``{`\
`  ``out`` ``<-`` ``grid_of``(``k``)`\
`  ``for`` ``(``what`` ``in`` `[`c`](https://rdrr.io/r/base/c.html)`(``"schools"``, ``"health"``)``)`` ``{`\
`    ``m`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``network``(``k``, ``elevation``)``, ``out``, ``units_of``(``k``, ``what``)``, mode ``=`` ``mode``,`\
`                            max_lts ``=`` ``max_lts``, max_trip_duration ``=`` ``60``)`\
`    ``m`` ``<-`` ``m``[`[`order`](https://rdrr.io/r/base/order.html)`(``m``$``from_id``, ``m``$``travel_time_p50``, ``m``$``to_id``)``, ``]`\
`    ``m`` ``<-`` ``m``[``!`[`duplicated`](https://rdrr.io/r/base/duplicated.html)`(``m``$``from_id``)``, ``]`\
`    ``i`` ``<-`` `[`match`](https://rdrr.io/r/base/match.html)`(``out``$``id``, ``m``$``from_id``)`\
`    ``out``[[``what``]``]`` ``<-`` ``m``$``travel_time_p50``[``i``]``        ``# NA: none within the hour`\
`    ``out``[[`[`paste0`](https://rdrr.io/r/base/paste.html)`(``what``, ``"_id"``)``]``]`` ``<-`` ``m``$``to_id``[``i``]`\
`  ``}`\
`  ``out`\
`}`\
`share`` ``<-`` ``function``(``t``, ``what``, ``minutes`` ``=`` ``15``)`` ``{`\
`  `[`round`](https://rdrr.io/r/base/Round.html)`(``100`` ``*`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``t``$``population``[``!`[`is.na`](https://rdrr.io/r/base/NA.html)`(``t``[[``what``]``]``)`` ``&`` ``t``[[``what``]``]`` ``<=`` ``minutes``]``)`` ``/`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``t``$``population``)``, ``1``)`\
`}`\
\
`# what a raster adds to the walk to the nearest health unit, in minutes`\
`# averaged over the residents, and the share of them who lose 2 or more`\
`added`` ``<-`` ``function``(``k``, ``elevation``, ``inside`` ``=`` ``function``(``lon``, ``lat``)`` ``TRUE``)`` ``{`\
`  ``flat`` ``<-`` ``nearest``(``k``, ``"NONE"``, ``"WALK"``)`\
`  ``hill`` ``<-`` ``nearest``(``k``, ``elevation``, ``"WALK"``)`\
`  ``d`` ``<-`` ``hill``$``health`` ``-`` ``flat``$``health`\
`  ``ok`` ``<-`` ``!`[`is.na`](https://rdrr.io/r/base/NA.html)`(``d``)`` ``&`` ``inside``(``flat``$``lon``, ``flat``$``lat``)`\
`  `[`c`](https://rdrr.io/r/base/c.html)`(``residents ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`sum`](https://rdrr.io/r/base/sum.html)`(``flat``$``population``[``inside``(``flat``$``lon``, ``flat``$``lat``)``]``)``)``,`\
`    minutes_added ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``d``[``ok``]``, ``flat``$``population``[``ok``]``)``, ``2``)``,`\
`    lose_2_or_more ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``100`` ``*`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``flat``$``population``[``ok`` ``&`` ``d`` ``>=`` ``1.5``]``)`` ``/`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``flat``$``population``[``ok``]``)``, ``1``)``)`\
`}`\
[`rbind`](https://rdrr.io/r/base/cbind.html)`(``"Boa Viagem, GLO-30"`` ``=`` ``added``(``"recife"``, ``"TOBLER30"``, ``in_boa_viagem``)``,`\
`      ``"Boa Viagem, GLO-90"`` ``=`` ``added``(``"recife"``, ``"TOBLER"``, ``in_boa_viagem``)``,`\
`      ``"Recife, GLO-30"`` ``=`` ``added``(``"recife"``, ``"TOBLER30"``)``,`\
`      ``"Recife, GLO-90"`` ``=`` ``added``(``"recife"``, ``"TOBLER"``)``,`\
`      ``"Salvador, GLO-30"`` ``=`` ``added``(``"salvador"``, ``"TOBLER30"``)``,`\
`      ``"Salvador, GLO-90"`` ``=`` ``added``(``"salvador"``, ``"TOBLER"``)``,`\
`      ``"Petrolina, GLO-30"`` ``=`` ``added``(``"petrolina"``, ``"TOBLER30"``)``,`\
`      ``"Petrolina, GLO-90"`` ``=`` ``added``(``"petrolina"``, ``"TOBLER"``)``)`\
`#>                    residents minutes_added lose_2_or_more`\
`#> Boa Viagem, GLO-30    206103          1.04           26.1`\
`#> Boa Viagem, GLO-90    206103          0.15            1.0`\
`#> Recife, GLO-30       1488797          1.05           26.2`\
`#> Recife, GLO-90       1488797          0.42            5.5`\
`#> Salvador, GLO-30     2418518          2.28           60.2`\
`#> Salvador, GLO-90     2418518          1.26           31.8`\
`#> Petrolina, GLO-30     409608          0.30            2.2`\
`#> Petrolina, GLO-90     409608          0.16            0.2`

On GLO-30 the flat district loses as much as the whole of Recife, hills
included: a minute on average, and a quarter of its 206 thousand
residents lose two minutes or more. That is the roofs, not the ground.
On GLO-90 the same district loses 0.15 of a minute and 1% of its
residents lose two minutes: the control is close to zero, and what
remains elsewhere is mostly terrain. Petrolina, flat and low-built,
hardly changes on either.

So **everything below uses GLO-90**. It errs the other way: a pixel of
90 m averages short, steep streets (a 30 m pixel already does), so the
slope costs here are on the low side, and the 30 m figures are a ceiling
that includes the roofs: in Salvador, 1.3 and 2.3 minutes. A bare-earth
terrain model at a finer resolution would settle it. Pernambuco has a
state-wide LiDAR survey (Pernambuco Tridimensional, PE3D); its terms of
use were not checked for this page, and it is not used here.

## Six municipalities, flat and with slopes

\
`pe`` ``<-`` `[`c`](https://rdrr.io/r/base/c.html)`(``"recife"``, ``"olinda"``, ``"jaboatao"``, ``"caruaru"``, ``"garanhuns"``, ``"petrolina"``)`\
`shares`` ``<-`` `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(``pe``, ``function``(``k``)`` ``{`\
`  `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"BICYCLE"``)``, ``function``(``mode``)`` ``{`\
`    ``flat`` ``<-`` ``nearest``(``k``, ``"NONE"``, ``mode``)`\
`    ``hill`` ``<-`` ``nearest``(``k``, ``"TOBLER"``, ``mode``)`\
`    `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``city ``=`` ``k``, mode ``=`` ``mode``,`\
`               school_flat ``=`` ``share``(``flat``, ``"schools"``)``, school_slopes ``=`` ``share``(``hill``, ``"schools"``)``,`\
`               health_flat ``=`` ``share``(``flat``, ``"health"``)``, health_slopes ``=`` ``share``(``hill``, ``"health"``)``)`\
`  ``}``)``)`\
`}``)``)`\
`shares`\
`#>         city    mode school_flat school_slopes health_flat health_slopes`\
`#> 1     recife    WALK        95.6          95.0        75.8          73.7`\
`#> 2     recife BICYCLE        99.5          99.5        96.0          95.5`\
`#> 3     olinda    WALK        90.0          88.2        75.8          74.6`\
`#> 4     olinda BICYCLE       100.0          99.9        99.0          98.9`\
`#> 5   jaboatao    WALK        89.2          87.0        81.1          77.8`\
`#> 6   jaboatao BICYCLE        98.3          97.8        95.2          94.5`\
`#> 7    caruaru    WALK        72.5          70.6        68.0          65.8`\
`#> 8    caruaru BICYCLE        92.7          91.4        84.9          84.3`\
`#> 9  garanhuns    WALK        78.7          74.9        73.3          70.7`\
`#> 10 garanhuns BICYCLE        90.1          89.2        88.1          86.8`\
`#> 11 petrolina    WALK        71.2          70.9        44.1          43.3`\
`#> 12 petrolina BICYCLE        84.3          84.3        70.5          70.1`

The share of the residents (in %) within 15 minutes of the nearest
public school and primary health care unit, on flat ground and with
Tobler’s slope costs. On foot every municipality loses something, up to
4 points: Jaboatão dos Guararapes the most to a health unit (81.1% to
77.8%) and Garanhuns, on the Borborema plateau, the most to a school
(78.7% to 74.9%); Petrolina, on the plain of the São Francisco, less
than one. By bicycle the largest change is 1.3 points: almost everyone
who is within 15 minutes on flat ground stays within it.

![Dot plots of the share of residents within 15 minutes of the nearest
public school and primary health care unit, walking and cycling, on flat
ground and with slopes, for six municipalities of Pernambuco: on foot
each loses up to 4 percentage points, by bicycle about one at
most.](figures/terrain-pe-shares.svg)

### Recife and Caruaru, as published

The [Recife and Caruaru example](recife-caruaru.md) is computed without
elevation, and stays that way (its figures and its Python page are the
same analysis). Here are its headline numbers, the share of the
population within 15 and 30 minutes, recomputed with the slopes:

\
[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"recife"``, ``"caruaru"``)``, ``function``(``k``)`` ``{`\
`  `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"BICYCLE"``)``, ``function``(``mode``)`` ``{`\
`    ``flat`` ``<-`` ``nearest``(``k``, ``"NONE"``, ``mode``)`\
`    ``hill`` ``<-`` ``nearest``(``k``, ``"TOBLER"``, ``mode``)`\
`    `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``city ``=`` ``k``, mode ``=`` ``mode``,`\
`               school_15 ``=`` ``share``(``hill``, ``"schools"``)``, health_15 ``=`` ``share``(``hill``, ``"health"``)``,`\
`               school_30 ``=`` ``share``(``hill``, ``"schools"``, ``30``)``, health_30 ``=`` ``share``(``hill``, ``"health"``, ``30``)``,`\
`               flat_school_30 ``=`` ``share``(``flat``, ``"schools"``, ``30``)``, flat_health_30 ``=`` ``share``(``flat``, ``"health"``, ``30``)``)`\
`  ``}``)``)`\
`}``)``)`\
`#>      city    mode school_15 health_15 school_30 health_30 flat_school_30`\
`#> 1  recife    WALK      95.0      73.7      99.6      97.3           99.6`\
`#> 2  recife BICYCLE      99.5      95.5      99.8      99.8           99.8`\
`#> 3 caruaru    WALK      70.6      65.8      92.0      84.6           92.7`\
`#> 4 caruaru BICYCLE      91.4      84.3      97.7      90.7           97.9`\
`#>   flat_health_30`\
`#> 1           97.5`\
`#> 2           99.8`\
`#> 3           85.7`\
`#> 4           91.1`

With the slopes, on foot, Recife goes from 95.6% to 95.0% of its
residents within 15 minutes of a school and from 75.8% to 73.7% of a
health unit; Caruaru from 72.5% to 70.6% and from 68.0% to 65.8%. Within
30 minutes almost nothing changes (Recife’s health units: 97.5% to
97.3%; Caruaru’s: 85.7% to 84.6%), and by bicycle the largest change is
1.3 points (Caruaru’s schools within 15 minutes). The published page
said that Recife is flat and Caruaru hilly, so that Caruaru’s times were
the optimistic ones. Measured, the two lose about the same: Recife’s
plain is flat, but the hills of the north and the west of the city are
not, and people live on them, as the map of the next section shows. The
Recife and Caruaru pages now say so.

## The metropolitan region as one network

The 14 municipalities of the Região Metropolitana do Recife as a single
network: Recife, Olinda, Jaboatão dos Guararapes, Paulista, Camaragibe,
São Lourenço da Mata, Cabo de Santo Agostinho, Ipojuca, Moreno,
Igarassu, Abreu e Lima, Itapissuma, Ilha de Itamaracá and Araçoiaba,
with 3 km around them.

**The municipal border.** A municipal network stops at the border, or
needs a buffer chosen by hand; one network of the region does not. How
many residents have a school or a health unit across the border nearer
than any in their own municipality? Each cell’s residents are split by
the municipality of their census tract, each unit belongs to the
municipality it stands in, and a resident counts when a unit elsewhere
is strictly nearer on foot (times are whole minutes; ties do not count)
or when none in their own municipality is within the hour:

\
`munis`` ``<-`` `[`st_read`](https://r-spatial.github.io/sf/reference/st_read.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_dir``, ``"rmr"``, ``"boundary.geojson"``)``, quiet ``=`` ``TRUE``)`\
`unit_muni`` ``<-`` ``function``(``units``)`` ``{`\
`  ``p`` ``<-`` `[`st_as_sf`](https://r-spatial.github.io/sf/reference/st_as_sf.html)`(``units``, coords ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"lon"``, ``"lat"``)``, crs ``=`` ``4674``)`\
`  ``hit`` ``<-`` `[`st_intersects`](https://r-spatial.github.io/sf/reference/geos_binary_pred.html)`(`[`st_transform`](https://r-spatial.github.io/sf/reference/st_transform.html)`(``p``, `[`st_crs`](https://r-spatial.github.io/sf/reference/st_crs.html)`(``munis``)``)``, ``munis``)`\
`  `[`setNames`](https://rdrr.io/r/stats/setNames.html)`(`[`vapply`](https://rdrr.io/r/base/lapply.html)`(``hit``, ``function``(``i``)`` ``if`` ``(`[`length`](https://rdrr.io/r/base/length.html)`(``i``)``)`` ``munis``$``code``[``i``[``1``]``]`` ``else`` ``"outside"``, ``""``)``, ``units``$``id``)`\
`}`\
`# the residents of each cell, by the municipality of their census tract`\
`by_muni`` ``<-`` `[`read.csv`](https://rdrr.io/r/utils/read.table.html)`(`[`file.path`](https://rdrr.io/r/base/file.path.html)`(``data_dir``, ``"rmr"``, ``"cells_muni.csv"``)``,`\
`                    colClasses ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``id ``=`` ``"character"``, code ``=`` ``"character"``)``)`\
`by_muni`` ``<-`` ``by_muni``[``by_muni``$``id`` `[`%in%`](https://rdrr.io/r/base/match.html)` ``grid_of``(``"rmr"``)``$``id``, ``]`\
`across`` ``<-`` ``function``(``what``)`` ``{`\
`  ``units`` ``<-`` ``units_of``(``"rmr"``, ``what``)`\
`  ``m`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``network``(``"rmr"``, ``"TOBLER"``)``, ``grid_of``(``"rmr"``)``, ``units``, mode ``=`` ``"WALK"``,`\
`                          max_trip_duration ``=`` ``60``)`\
`  ``# the time from each cell to the nearest unit of each municipality`\
`  ``best`` ``<-`` `[`tapply`](https://rdrr.io/r/base/tapply.html)`(``m``$``travel_time_p50``, `[`list`](https://rdrr.io/r/base/list.html)`(``m``$``from_id``, ``unit_muni``(``units``)``[``m``$``to_id``]``)``, ``min``)`\
`  ``rows`` ``<-`` `[`match`](https://rdrr.io/r/base/match.html)`(``by_muni``$``id``, `[`rownames`](https://rdrr.io/r/base/colnames.html)`(``best``)``)`\
`  ``own`` ``<-`` ``best``[`[`cbind`](https://rdrr.io/r/base/cbind.html)`(``rows``, `[`match`](https://rdrr.io/r/base/match.html)`(``by_muni``$``code``, `[`colnames`](https://rdrr.io/r/base/colnames.html)`(``best``)``)``)``]`\
`  ``other`` ``<-`` `[`vapply`](https://rdrr.io/r/base/lapply.html)`(`[`seq_along`](https://rdrr.io/r/base/seq.html)`(``rows``)``, ``function``(``i``)`` ``{`\
`    ``if`` ``(`[`is.na`](https://rdrr.io/r/base/NA.html)`(``rows``[``i``]``)``)`` `[`return`](https://rdrr.io/r/base/function.html)`(``NA_real_``)`\
`    ``v`` ``<-`` ``best``[``rows``[``i``]``, `[`colnames`](https://rdrr.io/r/base/colnames.html)`(``best``)`` ``!=`` ``by_muni``$``code``[``i``]``]`\
`    ``if`` ``(`[`all`](https://rdrr.io/r/base/all.html)`(`[`is.na`](https://rdrr.io/r/base/NA.html)`(``v``)``)``)`` ``NA_real_`` ``else`` `[`min`](https://rdrr.io/r/base/Extremes.html)`(``v``, na.rm ``=`` ``TRUE``)`\
`  ``}``, ``0``)`\
`  ``nearer`` ``<-`` ``!`[`is.na`](https://rdrr.io/r/base/NA.html)`(``other``)`` ``&`` ``(`[`is.na`](https://rdrr.io/r/base/NA.html)`(``own``)`` ``|`` ``other`` ``<`` ``own``)`\
`  ``people`` ``<-`` `[`tapply`](https://rdrr.io/r/base/tapply.html)`(``by_muni``$``population`` ``*`` ``nearer``, ``by_muni``$``code``, ``sum``)`\
`  ``total`` ``<-`` `[`tapply`](https://rdrr.io/r/base/tapply.html)`(``by_muni``$``population``, ``by_muni``$``code``, ``sum``)`\
`  `[`list`](https://rdrr.io/r/base/list.html)`(``region ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``people ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`sum`](https://rdrr.io/r/base/sum.html)`(``people``)``)``, share ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``100`` ``*`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``people``)`` ``/`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``total``)``, ``1``)``,`\
`                  tied ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`sum`](https://rdrr.io/r/base/sum.html)`(``by_muni``$``population``[``!`[`is.na`](https://rdrr.io/r/base/NA.html)`(``other``)`` ``&`` ``!`[`is.na`](https://rdrr.io/r/base/NA.html)`(``own``)`` ``&`` ``other`` ``==`` ``own``]``)``)``)``,`\
`       top ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``100`` ``*`` `[`sort`](https://rdrr.io/r/base/sort.html)`(``people`` ``/`` ``total``, decreasing ``=`` ``TRUE``)``[``1``:``4``]``, ``1``)``)`\
`}`\
`codes`` ``<-`` `[`c`](https://rdrr.io/r/base/c.html)`(``"2610707"`` ``=`` ``"Paulista"``, ``"2609600"`` ``=`` ``"Olinda"``, ``"2603454"`` ``=`` ``"Camaragibe"``,`\
`           ``"2611606"`` ``=`` ``"Recife"``, ``"2607752"`` ``=`` ``"Itapissuma"``, ``"2606804"`` ``=`` ``"Igarassu"``,`\
`           ``"2607901"`` ``=`` ``"Jaboatão dos Guararapes"``, ``"2613701"`` ``=`` ``"São Lourenço da Mata"``)`\
`for`` ``(``what`` ``in`` `[`c`](https://rdrr.io/r/base/c.html)`(``"schools"``, ``"health"``)``)`` ``{`\
`  ``x`` ``<-`` `[`across`](https://dplyr.tidyverse.org/reference/across.html)`(``what``)`\
`  `[`names`](https://rdrr.io/r/base/names.html)`(``x``$``top``)`` ``<-`` ``codes``[`[`names`](https://rdrr.io/r/base/names.html)`(``x``$``top``)``]`\
`  `[`print`](https://rdrr.io/r/base/print.html)`(``x``)`\
`}`\
`#> $region`\
`#>   people    share     tied `\
`#> 108906.0      2.9  13487.0 `\
`#> `\
`#> $top`\
`#>   Paulista Itapissuma   Igarassu Camaragibe `\
`#>       11.6        4.5        3.3        2.9 `\
`#> `\
`#> $region`\
`#> people  share   tied `\
`#> 147740      4  14642 `\
`#> `\
`#> $top`\
`#>   Paulista     Olinda Camaragibe     Recife `\
`#>        9.3        8.7        5.1        3.7`

For 148 thousand residents of the region (4.0%) the nearest primary
health care unit on foot is in another municipality, and for 109
thousand (2.9%) the nearest public school; another 15 and 13 thousand
have one just as near on each side. It is a matter of a few borders:
Paulista (12% of its residents for schools, 9% for health units) and
Olinda (9% for health units), which run into each other and into Recife
without a gap, then Camaragibe. In absolute numbers Recife leads for
health units (55 thousand residents, 3.7%). An analysis by municipality
without a buffer would send these residents to a farther unit; the units
outside the region, within 3 km of it, count as “another municipality”
too. The registers say where a unit is, not whom it serves: health units
have catchment areas, and municipal schools enrol their own residents
first.

![Bars, one per municipality of the Recife metropolitan region, of the
share of its residents whose nearest public school and nearest primary
health care unit on foot is in another municipality: Paulista 12% and
9%, Olinda 3% and 9%, Recife 2% and 4%, and close to zero in the outer
municipalities.](figures/terrain-rmr-border.svg)

**Where the slopes cost.** The minutes that the slopes add to the walk
to the nearest health unit, cell by cell, in the core of the region:

\
`added``(``"rmr"``, ``"TOBLER"``)`\
`#>      residents  minutes_added lose_2_or_more `\
`#>     3730664.00           0.49           8.10`

![A map of Recife, Olinda, Camaragibe and the north of Jaboatão covered
by hexagons coloured by the minutes that slopes add to the walk to the
nearest health unit: nothing on the coastal plain, 2 to 6 minutes on the
hills of the north and the west.](figures/terrain-rmr-penalty.svg)

Across the region the slopes add half a minute on average, and 8% of the
residents lose two minutes or more. They are not spread evenly: the
plain along the coast and the rivers does not change, and the hills
north and west of Recife’s centre, Camaragibe and the west of Jaboatão
lose 2 to 6 minutes.

## Cities of hills: Salvador and Olinda

Two centres on high ground: the Praça Municipal of Salvador, at the top
of the escarpment (the Lacerda lift goes down to the lower town from it;
a lift is not a street, and is not in the network), and the Alto da Sé
of Olinda. The areas reached from each in 10, 20 and 30 minutes, on flat
ground, with Tobler’s hiking function and with Minetti’s energy cost:

\
`centres`` ``<-`` `[`list`](https://rdrr.io/r/base/list.html)`(`\
`  salvador ``=`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` ``"Praça Municipal"``, lon ``=`` ``-``38.5128``, lat ``=`` ``-``12.9740``)``,`\
`  olinda ``=`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``id ``=`` ``"Alto da Sé"``, lon ``=`` ``-``34.8408``, lat ``=`` ``-``8.0136``)`\
`)`\
`utm_of`` ``<-`` `[`c`](https://rdrr.io/r/base/c.html)`(``salvador ``=`` ``31984``, olinda ``=`` ``31985``)`\
`iso`` ``<-`` `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`names`](https://rdrr.io/r/base/names.html)`(``centres``)``, ``function``(``k``)`` ``{`\
`  `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"BICYCLE"``)``, ``function``(``mode``)`` ``{`\
`    `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"NONE"``, ``"TOBLER"``, ``"MINETTI"``)``, ``function``(``e``)`` ``{`\
`      ``i`` ``<-`` `[`isochrone`](../reference/isochrone.md)`(``network``(``k``, ``e``)``, ``centres``[[``k``]``]``, mode ``=`` ``mode``, cutoffs ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``10``, ``20``, ``30``)``, zoom ``=`` ``11``)`\
`      `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``city ``=`` ``k``, mode ``=`` ``mode``, elevation ``=`` ``e``, minutes ``=`` ``i``$``isochrone``,`\
`                 km2 ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`as.numeric`](https://rdrr.io/r/base/numeric.html)`(`[`st_area`](https://r-spatial.github.io/sf/reference/geos_measures.html)`(`[`st_transform`](https://r-spatial.github.io/sf/reference/st_transform.html)`(``i``, ``utm_of``[[``k``]``]``)``)``)`` ``/`` ``1e6``, ``2``)``)`\
`    ``}``)``)`\
`  ``}``)``)`\
`}``)``)`\
[`xtabs`](https://rdrr.io/r/stats/xtabs.html)`(``km2`` ``~`` ``elevation`` ``+`` ``mode`` ``+`` ``city``, data ``=`` ``iso``[``iso``$``minutes`` ``==`` ``30``, ``]``)`\
`#> , , city = olinda`\
`#> `\
`#>          mode`\
`#> elevation BICYCLE  WALK`\
`#>   MINETTI   12.43  3.95`\
`#>   NONE      15.05  5.10`\
`#>   TOBLER    13.95  4.65`\
`#> `\
`#> , , city = salvador`\
`#> `\
`#>          mode`\
`#> elevation BICYCLE  WALK`\
`#>   MINETTI    8.49  2.89`\
`#>   NONE      15.13  4.40`\
`#>   TOBLER    12.28  3.69`

With Tobler’s costs, the area reached in 30 minutes from the Praça
Municipal shrinks by 16% on foot (4.4 to 3.7 km²) and by 19% by bicycle
(15.1 to 12.3 km²); from the Alto da Sé, by 9% and 7%. Minetti’s cost
takes more: 34% and 44% in Salvador. On GLO-30 the losses with Tobler
would be 35% in Salvador and 14% in Olinda, on foot: half or more of
that is roofs, by the control above.

![Four maps of central Salvador with the areas reached from the Praça
Municipal within 10, 20 and 30 minutes: walking and cycling, on flat
ground on the left and with slopes on the right, where the 30-minute
area is 16% smaller on foot and 19% by
bicycle.](figures/terrain-iso-salvador.svg)![Four maps of Olinda and the
north of Recife with the areas reached from the Alto da Sé within 10, 20
and 30 minutes: walking and cycling, on flat ground on the left and with
slopes on the right, where the 30-minute area is 9% smaller on foot and
7% by bicycle.](figures/terrain-iso-olinda.svg)

### Minutes the slopes add, by cell

\
[`sapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``salvador ``=`` ``"salvador"``, olinda ``=`` ``"olinda"``, recife ``=`` ``"recife"``)``, ``added``, elevation ``=`` ``"TOBLER"``)`\
`#>                  salvador    olinda     recife`\
`#> residents      2418518.00 349970.00 1488797.00`\
`#> minutes_added        1.26      0.47       0.42`\
`#> lose_2_or_more      31.80      6.80       5.50`

The walk to the nearest health unit is 1.3 minutes longer on average in
Salvador, where a third of the residents lose two minutes or more, and
half a minute longer in Olinda and in Recife (7% and 6% lose two or
more). In Salvador the cost is spread over most of the city; in Olinda
it is the hills of the west and of the historic centre, and the coastal
strip does not change.

![Two maps of Salvador covered by hexagons coloured by the minutes that
slopes add to the walk to the nearest public school and primary health
care unit: one to three minutes over most of the city, more in the
valleys.](figures/terrain-penalty-salvador.svg)![Two maps of Olinda
covered by hexagons coloured by the minutes that slopes add to the walk
to the nearest public school and primary health care unit: nothing on
the coastal strip, up to six minutes on the hills of the
west.](figures/terrain-penalty-olinda.svg)

### Tobler against Minetti

`elevation = "MINETTI"` replaces Tobler’s hiking function by Minetti’s
energy cost of walking and cycling on a gradient. The two differ
visibly:

\
`models`` ``<-`` `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"salvador"``, ``"olinda"``, ``"garanhuns"``, ``"caruaru"``)``, ``function``(``k``)`` ``{`\
`  `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"BICYCLE"``)``, ``function``(``mode``)`` ``{`\
`    `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"NONE"``, ``"TOBLER"``, ``"MINETTI"``)``, ``function``(``e``)`` ``{`\
`      ``t`` ``<-`` ``nearest``(``k``, ``e``, ``mode``)`\
`      `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``city ``=`` ``k``, mode ``=`` ``mode``, elevation ``=`` ``e``, health_15 ``=`` ``share``(``t``, ``"health"``)``,`\
`                 mean_minutes ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``t``$``health``, ``t``$``population``, na.rm ``=`` ``TRUE``)``, ``2``)``)`\
`    ``}``)``)`\
`  ``}``)``)`\
`}``)``)`\
[`reshape`](https://rdrr.io/r/stats/reshape.html)`(``models``, direction ``=`` ``"wide"``, idvar ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"city"``, ``"mode"``)``, timevar ``=`` ``"elevation"``)`\
`#>         city    mode health_15.NONE mean_minutes.NONE health_15.TOBLER`\
`#> 1   salvador    WALK           60.7             15.09             56.4`\
`#> 4   salvador BICYCLE           88.9              7.81             86.8`\
`#> 7     olinda    WALK           75.8             11.20             74.6`\
`#> 10    olinda BICYCLE           99.0              4.14             98.9`\
`#> 13 garanhuns    WALK           73.3             11.51             70.7`\
`#> 16 garanhuns BICYCLE           88.1              6.18             86.8`\
`#> 19   caruaru    WALK           68.0             13.31             65.8`\
`#> 22   caruaru BICYCLE           84.9              7.07             84.3`\
`#>    mean_minutes.TOBLER health_15.MINETTI mean_minutes.MINETTI`\
`#> 1                16.18              49.2                17.84`\
`#> 4                 8.49              83.4                 9.66`\
`#> 7                11.67              71.8                12.53`\
`#> 10                4.29              98.8                 4.61`\
`#> 13               12.23              65.5                13.73`\
`#> 16                6.51              84.1                 7.20`\
`#> 19               13.85              60.9                14.41`\
`#> 22                7.34              82.3                 7.93`

Minetti’s cost is the larger in every city, on foot and by bicycle, by a
factor of two to three: in Salvador the average walk to a health unit
goes from 15.1 minutes on flat ground to 16.2 with Tobler and 17.8 with
Minetti, and the residents within 15 minutes from 60.7% to 56.4% and
49.2%. Each average is over the residents who reach a unit within the
hour with that model, so the three are not over exactly the same cells:
cell by cell, over those that reach one both ways, Tobler adds 1.26
minutes in Salvador (the table of the previous section), not the 1.09
that separates the two averages. The choice of function matters as much
as the choice of raster: a result should state both. `"TOBLER"` is the
default.

![Dot plots, one row per city, of the average time to the nearest health
unit, walking and cycling, on flat ground, with Tobler and with Minetti,
for Salvador, Olinda, Garanhuns and Caruaru: Minetti is the slowest
everywhere, Tobler between it and flat
ground.](figures/terrain-models.svg)

### Traffic stress and slope

`max_lts` sets the highest level of traffic stress a cyclist rides (1,
calm streets only, to 4, any street; 2 is the default); on a street
above it the bicycle is walked. Slopes and stress add up: the share of
the residents (in %) within 15 minutes of a health unit by bicycle, and
the average time to it:

\
`lts`` ``<-`` `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``"salvador"``, ``"olinda"``, ``"recife"``)``, ``function``(``k``)`` ``{`\
`  `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(``1``:``4``, ``function``(``l``)`` ``{`\
`    ``flat`` ``<-`` ``nearest``(``k``, ``"NONE"``, ``"BICYCLE"``, max_lts ``=`` ``l``)`\
`    ``hill`` ``<-`` ``nearest``(``k``, ``"TOBLER"``, ``"BICYCLE"``, max_lts ``=`` ``l``)`\
`    ``minutes`` ``<-`` ``function``(``t``)`` `[`round`](https://rdrr.io/r/base/Round.html)`(`[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``t``$``health``, ``t``$``population``, na.rm ``=`` ``TRUE``)``, ``2``)`\
`    `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``city ``=`` ``k``, max_lts ``=`` ``l``, flat_15 ``=`` ``share``(``flat``, ``"health"``)``,`\
`               slopes_15 ``=`` ``share``(``hill``, ``"health"``)``, flat_minutes ``=`` ``minutes``(``flat``)``,`\
`               slopes_minutes ``=`` ``minutes``(``hill``)``)`\
`  ``}``)``)`\
`}``)``)`\
`lts`\
`#>        city max_lts flat_15 slopes_15 flat_minutes slopes_minutes`\
`#> 1  salvador       1    85.5      83.4         8.75           9.51`\
`#> 2  salvador       2    88.9      86.8         7.81           8.49`\
`#> 3  salvador       3    90.1      88.0         7.53           8.20`\
`#> 4  salvador       4    96.7      95.7         5.34           5.81`\
`#> 5    olinda       1    98.0      97.9         4.98           5.19`\
`#> 6    olinda       2    99.0      98.9         4.14           4.29`\
`#> 7    olinda       3    99.0      98.9         4.12           4.27`\
`#> 8    olinda       4   100.0     100.0         3.45           3.58`\
`#> 9    recife       1    92.3      91.7         6.55           6.80`\
`#> 10   recife       2    96.0      95.5         5.58           5.78`\
`#> 11   recife       3    96.0      95.7         5.46           5.65`\
`#> 12   recife       4    99.8      99.7         3.98           4.08`

Stress costs more than slope. In Salvador a cyclist who rides only calm
streets takes 9.5 minutes on average to the nearest health unit, and one
who rides any street 5.8; the slopes add 0.5 to 0.8 of a minute at every
level. In Recife and Olinda the slopes add 0.1 to 0.25 of a minute.
Levels 2 and 3 give almost the same times in the three cities.

![Line charts, one per city, of the average cycling time to the nearest
health unit as the highest level of traffic stress ridden goes from 1 to
4, on flat ground and with slopes: the time falls by a third or more
from level 1 to level 4, and the slopes add under a minute in Salvador
and a quarter of a minute or less in Olinda and
Recife.](figures/terrain-lts.svg)

## What is not measured

- **The elevation model** is a surface model at about 90 m. It has no
  voids here, but it smooths short steep streets and stairs, reads some
  roofs and trees still, and dips under one bridge in five; the 30 m
  version reads roofs enough to cost a flat district a minute. The slope
  costs on this page are a low estimate, not a measurement of the
  ground.
- **Slope costs** apply to walking and cycling times on streets; steps,
  lifts and funiculars (Salvador’s Elevador Lacerda and its inclined
  planes) are not modelled, and they are how people cross the
  escarpment.
- **Cycling** assumes a rider on every street up to `max_lts`, with
  stress levels derived from OpenStreetMap tags; no surface, no
  lighting, no safety.
- **Nearest** is by travel time on the street network, from the centre
  of each cell (500 m apart; 250 m in Olinda), in whole minutes. It says
  where a unit is, not whether it has a place or serves that address.
- **The registers**: schools without coordinates in INEP’s catalogue are
  left out, CNES coordinates are those the establishments declared, and
  CNES changes daily (this is the file of 2026-11-12). The Censo is of
  2022, the schools of 2023, the streets of 2026-11-01.
- **No public transport and no congestion**: this page routes on foot
  and by bicycle only (for the region’s public transport, see the
  [Recife metropolitan region example](recife-metropolitan.md)).

Map data © OpenStreetMap contributors, under the ODbL; elevation:
Copernicus DEM GLO-90 and GLO-30 © DLR e.V. 2010-2014 and © Airbus
Defence and Space GmbH 2014-2018, provided under COPERNICUS by the
European Union and ESA; boundaries and population © IBGE; schools: INEP
(Catálogo de Escolas 2023, via geobr); health units: CNES, Ministério da
Saúde.
