# peabiru: multimodal routing on OpenStreetMap and GTFS networks

peabiru builds a network from an 'OpenStreetMap' extract, 'GTFS' feeds
and an optional elevation raster with
[`build_network()`](build_network.md), and computes, by walking,
cycling, driving and public transport: travel-time matrices
([`travel_time_matrix()`](travel_time_matrix.md),
[`expanded_travel_time_matrix()`](expanded_travel_time_matrix.md),
[`arrival_travel_time_matrix()`](arrival_travel_time_matrix.md)),
accessibility ([`accessibility()`](accessibility.md)), isochrones
([`isochrone()`](isochrone.md)) and detailed itineraries
([`detailed_itineraries()`](detailed_itineraries.md)), with or without
fares ([`setup_fare_structure()`](setup_fare_structure.md),
[`read_fare_structure()`](read_fare_structure.md),
[`write_fare_structure()`](write_fare_structure.md),
[`pareto_frontier()`](pareto_frontier.md)) and scenarios of car speeds
and traffic stress. It inspects a network with
[`find_snap()`](find_snap.md),
[`street_network_bbox()`](street_network_bbox.md),
[`street_network_to_sf()`](street_network_to_sf.md),
[`transit_network_to_sf()`](transit_network_to_sf.md),
[`check_transit_availability()`](check_transit_availability.md) and
[`get_gtfs_errors()`](get_gtfs_errors.md). The routing engine is written
in 'Rust' and runs inside the R session; no 'Java' is needed. What the
package does not support is an error that says so, never silently
ignored.

## Details

The routing engine is a 'Rust' port of Conveyal's 'R5' routing engine,
and the functions are modelled on those of the 'r5r' package (Ipea),
from whose 'Java' code the fare calculator and parts of the trip planner
are translated; see the references, and the article "Compared with r5r"
on the package website. peabiru is an independent project, not
affiliated with or endorsed by Conveyal, Ipea or the r5r team.

## Class of the results

The tables the functions return are tibbles by default. The option
`peabiru.output`, read at each call, chooses another class:

- `options(peabiru.output = "tibble")` (the default; 'tibble' is the
  package's one dependency): tibbles, and `sf` tibbles (class
  `c("sf", "tbl_df", "tbl", "data.frame")`) for the results with a
  geometry, as
  [`sf::st_as_sf()`](https://r-spatial.github.io/sf/reference/st_as_sf.html)
  makes of a tibble;

- `options(peabiru.output = "data.frame")`: base `data.frame`s, and
  plain `sf` data frames;

- `options(peabiru.output = "data.table")` (needs 'data.table'):
  `data.table`s: `sf` `data.table`s for isochrone polygons, itineraries,
  the street edges and the transit routes, plain `sf` data frames for
  isochrone lines, street vertices and transit stops, and an index on
  the column `found` of [`find_snap()`](find_snap.md). A good choice for
  large matrices handled with 'data.table'.

Any other value is an error. The option applies to every table: the
matrices, accessibility, Pareto frontiers,
[`find_snap()`](find_snap.md),
[`check_transit_availability()`](check_transit_availability.md),
[`get_gtfs_errors()`](get_gtfs_errors.md), the tables of a fare
structure, the tables of [`trip_options()`](trip_options.md) and the
`sf` results (whose geometries are always in EPSG:4326). The rows, the
columns, their order and types, and the row names are the same whatever
the class.

## Sample data

`system.file("extdata/spo", package = "peabiru")` holds a small network
for examples and tests, cut from the Sao Paulo sample data of the r5r
project: `spo_osm.pbf`, the streets around Avenida Paulista (data (c)
OpenStreetMap contributors, available under the Open Database License,
<https://www.openstreetmap.org/copyright>); `spo_gtfs.zip`, the SPTrans
frequency-based 'GTFS' feed of that sample, without `shapes.txt`; and
`spo_points.csv`, 27 points of its hexagonal grid with their population
and opportunity counts.

## References

Conway, M. W., Byrd, A. and van der Linden, M. (2017). Evidence-based
transit and land use sketch planning using interactive accessibility
methods on combined schedule and headway-based networks. *Transportation
Research Record*, 2653(1), 45-53.
[doi:10.3141/2653-06](https://doi.org/10.3141/2653-06)

Conway, M. W., Byrd, A. and van Eggermond, M. (2018). Accounting for
uncertainty and variation in accessibility metrics for public transport
sketch planning. *Journal of Transport and Land Use*, 11(1).
[doi:10.5198/jtlu.2018.1074](https://doi.org/10.5198/jtlu.2018.1074)

Conway, M. W. and Stewart, A. F. (2019). Getting Charlie off the MTA: a
multiobjective optimization method to account for cost constraints in
public transit accessibility metrics. *International Journal of
Geographical Information Science*, 33(9), 1759-1787.
[doi:10.1080/13658816.2019.1605075](https://doi.org/10.1080/13658816.2019.1605075)

Delling, D., Pajor, T. and Werneck, R. F. (2015). Round-based public
transit routing. *Transportation Science*, 49(3), 591-604.
[doi:10.1287/trsc.2014.0534](https://doi.org/10.1287/trsc.2014.0534)

Geisberger, R., Sanders, P., Schultes, D. and Vetter, C. (2012). Exact
routing in large road networks using contraction hierarchies.
*Transportation Science*, 46(3), 388-404.
[doi:10.1287/trsc.1110.0401](https://doi.org/10.1287/trsc.1110.0401)

Pereira, R. H. M., Saraiva, M., Herszenhut, D., Braga, C. K. V. and
Conway, M. W. (2021). r5r: Rapid Realistic Routing on Multimodal
Transport Networks with R5 in R. *Findings*.
[doi:10.32866/001c.21262](https://doi.org/10.32866/001c.21262)

## See also

Useful links:

- <https://strategicprojects.github.io/peabiru-routing/r/>

- <https://github.com/StrategicProjects/peabiru-routing>

- Report bugs at
  <https://github.com/StrategicProjects/peabiru-routing/issues>

## Author

**Maintainer**: André Leite <leite@castlab.org>
([ORCID](https://orcid.org/0000-0002-4718-9766))

Authors:

- André Leite <leite@castlab.org>
  ([ORCID](https://orcid.org/0000-0002-4718-9766))

- Marcos Wasiliew <marcos.wasiliew@gmail.com>
  ([ORCID](https://orcid.org/0009-0004-4694-3159))

- Hugo Vasconcelos <hugo.vasconcelos@ufpe.br>
  ([ORCID](https://orcid.org/0000-0001-6249-0920))

- Carlos Amorim <carlos.agaf@ufpe.br>
  ([ORCID](https://orcid.org/0000-0001-6315-8305))

- Diogo Bezerra <diogo.bezerra@ufpe.br>
  ([ORCID](https://orcid.org/0000-0002-1216-8674))

- Júlia Nascimento Barreto <juliabarreto@gd.seplag.pe.gov.br>
  ([ORCID](https://orcid.org/0009-0004-2851-7770))

Other contributors:

- Conveyal LLC ('R5' routing engine, from which the routing logic is
  ported) \[copyright holder\]

- The Apache Software Foundation ('Apache Commons Math', parts of which
  are translated in the 'Rust' core) \[copyright holder\]

- Ipea - Institute for Applied Economic Research ('r5r' package: 'Java'
  code translated in the 'Rust' core (fare calculator, parts of the trip
  planner), the function names, arguments and result layouts, and the
  sample data) \[copyright holder\]

- r5r contributors (Copyright holders named in the license of the 'r5r'
  package) \[copyright holder\]

- Makoto Matsumoto (Mersenne Twister generator, translated in the 'Rust'
  core) \[copyright holder\]

- Takuji Nishimura (Mersenne Twister generator, translated in the 'Rust'
  core) \[copyright holder\]

- CERN - European Organization for Nuclear Research (Table of prime
  numbers from the 'Colt' library, in the 'Rust' core) \[copyright
  holder\]

- isoband authors ('isoband' package, whose contouring algorithm is
  translated in the 'Rust' core) \[copyright holder\]

- CIESIN - Center for International Earth Science Information Network,
  Columbia University (Population grid (GPWv3) by which the 'Rust' core
  checks GTFS stop locations) \[copyright holder\]

- OpenStreetMap contributors (Map data of the sample network,
  inst/extdata/spo/spo_osm.pbf) \[copyright holder\]

- Authors of the dependency Rust crates (Rust crates vendored in
  src/rust/vendor.tar.xz; see inst/AUTHORS) \[copyright holder\]
