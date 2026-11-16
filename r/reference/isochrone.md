# Estimate isochrones from a given location

The areas (`polygon_output = TRUE`) or the street segments
(`polygon_output = FALSE`) reached from each origin within each cutoff,
by one street mode (`"WALK"`, `"BICYCLE"` or `"CAR"`) or by public
transport (see [`travel_time_matrix()`](travel_time_matrix.md) for the
modes). Package 'sf' is required.

## Usage

``` r
isochrone(
  network,
  origins,
  mode = "transit",
  mode_egress = "walk",
  cutoffs = c(0, 15, 30),
  zoom = 10,
  departure_datetime = NULL,
  polygon_output = TRUE,
  time_window = 10L,
  max_walk_time = Inf,
  max_bike_time = Inf,
  max_car_time = Inf,
  max_trip_duration = 120L,
  walk_speed = 3.6,
  bike_speed = 12,
  max_rides = 3,
  max_lts = 2,
  draws_per_minute = 5L,
  seed = NULL,
  percentiles = NULL,
  n_threads = Inf,
  verbose = FALSE,
  progress = TRUE
)
```

## Arguments

- network:

  A network built with [`build_network()`](build_network.md).

- origins:

  A `data.frame` with columns `id`, `lon` and `lat` (WGS 84), or a
  `POINT sf` with an `id` column, in any coordinate reference system
  (reprojected to WGS 84, see
  [`travel_time_matrix()`](travel_time_matrix.md)).

- mode:

  A character vector, the transport modes (default `"transit"`): one of
  `"WALK"`, `"BICYCLE"` or `"CAR"`, and for public transport `"TRANSIT"`
  or some of its modes (see
  [`travel_time_matrix()`](travel_time_matrix.md)).

- mode_egress:

  A string, the mode from the last stop to the destination: `"WALK"`
  (the default), `"BICYCLE"` or `"CAR"`. Ignored without public
  transport.

- cutoffs:

  A numeric vector, the travel times in minutes that delimit the
  isochrones. Values are sorted and duplicates removed; 0 is added and
  at least one value must be greater than 0. Defaults to `c(0, 15, 30)`.

- zoom:

  A number from 9 to 12: the Web Mercator zoom level of the travel-time
  grid polygons are interpolated from (only used with
  `polygon_output = TRUE`). Defaults to 10 (cells of about 153 metres at
  the Equator).

- departure_datetime:

  A POSIXct, the departure date and time (see the section "Date and
  time"). Required with public transport.

- polygon_output:

  A logical. `TRUE` (the default) for polygon isochrones, `FALSE` for
  the street segments reached.

- time_window:

  An integer, the number of departure minutes, from `departure_datetime`
  on, over which travel times are computed. Defaults to 10.

- max_walk_time, max_bike_time, max_car_time:

  The maximum time in minutes on foot, by bicycle and by car. Each limit
  applies to each leg on its own (15 minutes to reach the first stop and
  another 15 from the last), and to the whole trip when that mode is the
  only one. `Inf`, the default, leaves only `max_trip_duration`.

- max_trip_duration:

  Not used: the trip limit comes from `max(cutoffs)`.

- walk_speed, bike_speed:

  Average speeds in km/h, 3.6 and 12 by default.

- max_rides:

  An integer, the maximum number of public transport rides in a trip.
  Defaults to 3.

- max_lts:

  An integer from 1 to 4, the highest level of traffic stress a cyclist
  rides on (see the section "Level of traffic stress"). Defaults to 2.

- draws_per_minute:

  An integer, the Monte Carlo draws per departure minute (see the
  section "Departures and Monte Carlo draws"). Defaults to 5.

- seed:

  An integer seed for the Monte Carlo draws, or `NULL` (the default: the
  draws differ from call to call). With a seed, the same inputs give the
  same results whatever `n_threads`.

- percentiles:

  An integer vector of up to 5 values from 1 to 99, the travel time
  percentiles to make polygons for (one set per percentile). `NULL` (the
  default) means 50. Must be `NULL` with `polygon_output = FALSE`.

- n_threads:

  The number of threads; `Inf` (the default) uses all cores (two under
  `R CMD check` when it limits the cores).

- verbose, progress:

  Logicals, checked but without effect: no progress counter or routing
  log is printed.

## Value

An `sf` table in WGS 84 (EPSG:4326). With `polygon_output = TRUE`, one
`POLYGON` or `MULTIPOLYGON` per origin, percentile and cutoff, with
columns `id`, `isochrone` (the cutoff), `percentile` (such as `"p50"`)
and `polygons`; rows by origin, percentile and cutoff (descending). Each
polygon covers the whole area reached from 0 up to its cutoff. With
`polygon_output = FALSE`, one `LINESTRING` per street segment reached,
with columns `id`, `edge_index`, `osm_id`, `isochrone` (the smallest
cutoff at or above the segment's travel time), `travel_time_p50`, the
segment's attributes from
[`street_network_to_sf()`](street_network_to_sf.md) (`from_vertex`,
`to_vertex`, `street_class`, `length`, `walk`, `car`, `car_speed`,
`bicycle`, `bicycle_lts`) and `geometry`; rows by origin, then by band
and travel time, both descending.

Tables are tibbles by default; `options(peabiru.output = "data.frame")`
gives base `data.frame`s and `options(peabiru.output = "data.table")`
gives `data.table`s (see [peabiru-package](peabiru-package.md)). An `sf`
result keeps its geometry column and EPSG:4326 and takes the same data
frame class: an `sf` tibble, a plain `sf` data frame, or, with
"data.table", an `sf` `data.table` where the section "Class of the
results" of [peabiru-package](peabiru-package.md) says so. With
"data.table", the polygons are an `sf` `data.table` and the lines a
plain `sf` data frame.

## Details

Polygon isochrones are traced from the travel times from each origin to
the centres of a regular grid of Web Mercator pixels covering the
network (see `zoom`): the bands from 0 to each cutoff are traced over
that grid by marching squares (the algorithm of
[`isoband::isobands()`](http://isoband.r-lib.org/reference/isobands.md)),
grouped into polygons with holes (as
[`isoband::iso_to_sfg()`](http://isoband.r-lib.org/reference/iso_to_sfg.md)
groups them) and passed to
[`sf::st_make_valid()`](https://r-spatial.github.io/sf/reference/valid.html)
(see the section "Repair of the polygons" for the case in which
spherical geometry changes their area). The order of the rings and the
vertex each one starts at are those 'isoband' gives on each platform,
which follow the C++ library it is built with (libc++ on macOS,
libstdc++ on Linux and Windows); between platforms the polygons differ
in that order and, by the system's
[`sinh()`](https://rdrr.io/r/base/Hyperbolic.html) and
[`atan()`](https://rdrr.io/r/base/Trig.html), in the last bits of some
latitudes.

Line isochrones are the travel times from each origin to the centroid of
every street segment
([`sf::st_centroid()`](https://r-spatial.github.io/sf/reference/geos_unary.html)
of the edges of [`street_network_to_sf()`](street_network_to_sf.md)),
each segment in the band of the smallest cutoff at or above its travel
time.

`max_trip_duration` is not used: the trip limit is the largest cutoff
(plus 10 minutes for polygons, so that the outer band is interpolated
like the inner ones).

## Repair of the polygons

The traced polygons go through
[`sf::st_make_valid()`](https://r-spatial.github.io/sf/reference/valid.html).
With spherical geometry on
([`sf::sf_use_s2()`](https://r-spatial.github.io/sf/reference/s2.html),
sf's default), that repair snaps the vertices and, in a polygon of many
parts and holes, can return an area that is not the traced one: of 438
isochrones of Porto Alegre and Helsinki, 17 came out between 0.1% and
9.5% larger or smaller. When the spherical repair changes the area of a
polygon by more than one part in 10,000, the polygons of that origin are
repaired on the plane instead (which leaves the area as traced) and a
warning of class `peabiru_isochrone_repair` names them.
`options(peabiru.isochrone_repair = "spherical")` keeps the spherical
repair in every case, without the warning.

## See also

Other routing functions: [`accessibility()`](accessibility.md),
[`arrival_travel_time_matrix()`](arrival_travel_time_matrix.md),
[`detailed_itineraries()`](detailed_itineraries.md),
[`expanded_travel_time_matrix()`](expanded_travel_time_matrix.md),
[`travel_time_matrix()`](travel_time_matrix.md)

## Examples

``` r
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
points <- read.csv(file.path(data_path, "spo_points.csv"))
departure <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")

# polygons: the areas reached on foot within 10 and 20 minutes
iso <- isochrone(network, points[1, ], mode = "WALK",
                 cutoffs = c(0, 10, 20), departure_datetime = departure,
                 n_threads = 2)
iso
#> Simple feature collection with 2 features and 3 fields
#> Geometry type: POLYGON
#> Dimension:     XY
#> Bounding box:  xmin: -46.63902 ymin: -23.56588 xmax: -46.6198 ymax: -23.54762
#> Geodetic CRS:  WGS 84
#> # A tibble: 2 × 4
#>   id              isochrone percentile                                  polygons
#>   <chr>               <dbl> <chr>                                  <POLYGON [°]>
#> 1 89a8100c00fffff        20 p50        ((-46.62941 -23.56462, -46.62804 -23.564…
#> 2 89a8100c00fffff        10 p50        ((-46.62804 -23.55958, -46.62666 -23.559…

# lines: the street segments reached by public transport
iso_lines <- isochrone(network, points[1, ], mode = "TRANSIT",
                       cutoffs = c(0, 10, 20), polygon_output = FALSE,
                       departure_datetime = departure, seed = 42,
                       n_threads = 2)
head(iso_lines)
#> Simple feature collection with 6 features and 14 fields
#> Geometry type: LINESTRING
#> Dimension:     XY
#> Bounding box:  xmin: -46.63879 ymin: -23.55799 xmax: -46.63449 ymax: -23.55056
#> Geodetic CRS:  WGS 84
#> # A tibble: 6 × 15
#>   id           edge_index osm_id isochrone travel_time_p50 from_vertex to_vertex
#>   <chr>        <chr>       <dbl>     <dbl>           <int>       <int>     <int>
#> 1 89a8100c00f… 10         8.10e6        20              20           8         9
#> 2 89a8100c00f… 11         8.10e6        20              20           9         8
#> 3 89a8100c00f… 464        2.30e7        20              20         291       285
#> 4 89a8100c00f… 465        2.30e7        20              20         285       291
#> 5 89a8100c00f… 698        3.87e7        20              20         487       488
#> 6 89a8100c00f… 699        3.87e7        20              20         488       487
#> # ℹ 8 more variables: street_class <chr>, length <dbl>, walk <lgl>, car <lgl>,
#> #   car_speed <dbl>, bicycle <lgl>, bicycle_lts <int>,
#> #   geometry <LINESTRING [°]>
```
