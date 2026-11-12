# The data of the "Brazilian capitals with open GTFS" example
# (site/figures/capitals.R and the worked examples of the R and Python
# sites): for São Paulo, Rio de Janeiro and Belo Horizonte, the street network
# cut from OpenStreetMap, the official GTFS feeds of the city, and the
# hexagonal grid of Ipea's Access to Opportunities Project (population and
# income of the Censo 2010, jobs, schools and health facilities of 2019).
#
#   Rscript site/figures/capitals-data.R          # prepare the three cities
#
# Everything is downloaded once into a cache directory (PEABIRU_CAPITALS_CACHE,
# default: tools::R_user_dir("peabiru", "cache")/capitals), about 1 GB of raw
# files (the Geofabrik extract of the Sudeste is 0.86 GB of it); the prepared
# data of each city is written to <cache>/<city>/:
#
#   network/<city>.osm.pbf  the streets of the area served by the feeds
#   network/*.zip           the GTFS feeds, as published
#   cut.geojson             that area: the municipality and the stops of the
#                           feeds, buffered
#   boundary.geojson        the municipality (IBGE)
#   cells.gpkg, cells.csv   the H3 cells (resolution 9) of the municipality
#                           with population, income and opportunities
#
# and, for the fares, r5r's sample of Porto Alegre in <cache>/poa/ (network/,
# fares_poa.zip, cells.csv with the income of the same project).
#
# Needs osmium (osmium-tool), sf, and network access the first time.
#
# The feeds are fetched from their publishers (São Paulo's from the dated copy
# the Mobility Database keeps: SPTrans serves the file only to registered
# developers). Two of them are replaced in place when the city publishes a new
# version (Rio de Janeiro's is of 2026-06-10; Belo Horizonte's is rewritten
# every day): the script checks every download against the SHA-256 of the
# file the published figures were computed from, and stops if it differs,
# unless PEABIRU_CAPITALS_ACCEPT_NEW=true (the figures will then describe
# another version of the network). The Geofabrik extract is checked against
# the MD5 Geofabrik publishes for it.

suppressPackageStartupMessages(library(sf))

cap_sources <- list(
  # Geofabrik's extract of the Sudeste region of Brazil, of 2026-11-01 (ODbL)
  osm = "https://download.geofabrik.de/south-america/brazil/sudeste-261101.osm.pbf",
  osm_md5 = "15c2834145c0273c96b5e7dee21c1109",
  # IBGE's API of municipal boundaries (malha municipal)
  boundary = "https://servicodados.ibge.gov.br/api/v3/malhas/municipios/%s?formato=application/vnd.geo+json&qualidade=maxima",
  # Ipea, Access to Opportunities Project, aopdata v1.0.0 data release
  aop = "https://github.com/ipeaGIT/aopdata/releases/download/v1.0.0/%s"
)

cap_cities <- list(
  spo = list(
    code = "3550308", name = "São Paulo", aop = "spo",
    feeds = list(
      # SPTrans's feed (buses, Metrô and CPTM), as the Mobility Database
      # fetched it from SPTrans on 2026-11-12 (SPTrans serves it only to
      # registered developers)
      list(file = "sptrans.zip",
           url = "https://files.mobilitydatabase.org/mdb-8/mdb-8-202611120148/mdb-8-202611120148.zip",
           sha256 = "70b6a2fccc1206086060bd1e310d0d89f1950cb5e1ece4f006094f82ad00e940"))),
  rio = list(
    code = "3304557", name = "Rio de Janeiro", aop = "rio",
    feeds = list(
      # Secretaria Municipal de Transportes (SMTR): buses and BRT, CC BY 4.0
      list(file = "smtr.zip", url = "https://dados.mobilidade.rio/gtfs/schedule",
           sha256 = "a99f925460e7628b6eeecb9952430542c06b3e2800afa8ba7f9765fd2e6f26f1"))),
  bho = list(
    code = "3106200", name = "Belo Horizonte", aop = "bho",
    feeds = list(
      # Superintendência de Mobilidade (SUMOB): the conventional and the
      # supplementary bus systems in one feed, CC BY, version 202611111921
      list(file = "bhtrans.zip",
           url = "https://s3.amazonaws.com/mobilibus-uploads/gtfs/GTFSBHTRANS.zip",
           sha256 = "15d3b184fe8e1e3a343d613134464468e5746a6d2ab9f67b5598154ba087a8b2")))
)

utm <- 31983     # SIRGAS 2000 / UTM zone 23S: metres, for the three cities
buffer <- 3000   # around the municipality and the stops of its feeds (m)

cap_cache <- function() {
  d <- Sys.getenv("PEABIRU_CAPITALS_CACHE",
                  file.path(tools::R_user_dir("peabiru", "cache"), "capitals"))
  dir.create(file.path(d, "raw"), recursive = TRUE, showWarnings = FALSE)
  d
}

fetch <- function(url, dest) {
  if (!file.exists(dest)) {
    message("downloading ", basename(dest))
    old <- options(timeout = 3600, HTTPUserAgent = "peabiru-docs-examples")
    on.exit(options(old))
    utils::download.file(url, paste0(dest, ".part"), mode = "wb", quiet = TRUE)
    file.rename(paste0(dest, ".part"), dest)
  }
  dest
}

sha256 <- function(path) {
  out <- system2("shasum", c("-a", "256", shQuote(path)), stdout = TRUE)
  sub(" .*", "", out)
}

fetch_feed <- function(feed, raw) {
  dest <- fetch(feed$url, file.path(raw, feed$file))
  got <- sha256(dest)
  if (got != feed$sha256) {
    msg <- sprintf("%s: SHA-256 %s, not the %s the figures were computed from (the publisher has a new version)",
                   feed$file, got, feed$sha256)
    if (!identical(tolower(Sys.getenv("PEABIRU_CAPITALS_ACCEPT_NEW")), "true")) stop(msg)
    warning(msg)
  }
  dest
}

osmium <- function(...) {
  status <- system2("osmium", c(...))
  if (status != 0) stop("osmium ", paste(c(...)[1], collapse = " "), " failed")
}

# The stops of a GTFS feed, as points.
feed_stops <- function(zip) {
  s <- utils::read.csv(unz(zip, "stops.txt"), colClasses = "character", encoding = "UTF-8")
  s <- s[s$stop_lat != "" & s$stop_lon != "", ]
  st_as_sf(data.frame(lon = as.numeric(s$stop_lon), lat = as.numeric(s$stop_lat)),
           coords = c("lon", "lat"), crs = 4326)
}

prepare_capital <- function(key, cache = cap_cache()) {
  city <- cap_cities[[key]]
  raw <- file.path(cache, "raw")
  dir <- file.path(cache, key)
  done <- file.path(dir, "cells.csv")
  if (file.exists(done)) return(invisible(dir))
  dir.create(file.path(dir, "network"), recursive = TRUE, showWarnings = FALSE)

  # the feeds, copied next to the street network
  zips <- vapply(city$feeds, fetch_feed, "", raw = raw)
  file.copy(zips, file.path(dir, "network", basename(zips)), overwrite = TRUE)

  # the municipality, and the area cut from OpenStreetMap: the municipality
  # and the convex hull of the stops of its feeds (rail lines and a few bus
  # lines leave the municipality), buffered
  boundary <- st_read(fetch(sprintf(cap_sources$boundary, city$code),
                            file.path(raw, paste0(city$code, ".geojson"))), quiet = TRUE)
  boundary <- st_transform(st_make_valid(boundary), utm)
  stops <- st_transform(do.call(rbind, lapply(zips, feed_stops)), utm)
  hull <- st_convex_hull(st_union(stops))
  cut <- st_transform(st_buffer(st_union(st_geometry(boundary), hull), buffer), 4326)
  st_write(st_transform(boundary, 4326), file.path(dir, "boundary.geojson"),
           delete_dsn = TRUE, quiet = TRUE)
  st_write(st_sf(geometry = cut), file.path(dir, "cut.geojson"), delete_dsn = TRUE, quiet = TRUE)

  sudeste <- fetch(cap_sources$osm, file.path(raw, basename(cap_sources$osm)))
  if (unname(tools::md5sum(sudeste)) != cap_sources$osm_md5) {
    stop(basename(sudeste), ": MD5 differs from the one Geofabrik publishes; delete it and run again")
  }
  osmium("extract", "--overwrite", "-p", file.path(dir, "cut.geojson"), sudeste,
         "-o", file.path(dir, "network", paste0(key, ".osm.pbf")))

  # the cells of the Access to Opportunities Project: H3 resolution 9
  # (about 0.1 km2, centres 357 m apart), with the population and the income
  # per capita of the Censo 2010 (P001, R001; R003 the income decile, by
  # population), and the formal jobs (RAIS 2019, T001), public schools
  # (Censo Escolar 2019, E001) and public health facilities (CNES 2019, S001)
  aop <- function(f) fetch(sprintf(cap_sources$aop, f), file.path(raw, f))
  grid <- st_read(aop(sprintf("hex_grid_%s.gpkg", city$aop)), quiet = TRUE)
  pop <- utils::read.csv(aop(sprintf("population_2010_%s.csv", city$aop)))
  use <- utils::read.csv(aop(sprintf("landuse_2019_%s.csv", city$aop)))
  cells <- merge(grid[, "id_hex"], pop[, c("id_hex", "P001", "R001", "R003")], by = "id_hex")
  cells <- merge(cells, use[, c("id_hex", "T001", "E001", "S001")], by = "id_hex")
  names(cells)[match(c("id_hex", "P001", "R001", "R003", "T001", "E001", "S001"), names(cells))] <-
    c("id", "population", "income", "decile", "jobs", "schools", "health")
  xy <- st_coordinates(st_centroid(st_transform(st_geometry(cells), utm)))
  xy <- st_coordinates(st_transform(st_sfc(lapply(seq_len(nrow(xy)), function(i) st_point(xy[i, ])),
                                           crs = utm), 4326))
  cells$lon <- round(xy[, 1], 6)
  cells$lat <- round(xy[, 2], 6)
  st_write(cells, file.path(dir, "cells.gpkg"), delete_dsn = TRUE, quiet = TRUE)
  utils::write.csv(st_drop_geometry(cells)[, c("id", "lon", "lat", "population", "income", "decile",
                                               "jobs", "schools", "health")],
                   done, row.names = FALSE)
  invisible(dir)
}

# Porto Alegre, for the fares: the sample data of r5r 2.5.0 (an OpenStreetMap
# extract of the centre and the neighbourhoods around it, the bus feed of EPTC
# and the train feed of Trensurb of May 2019, a grid of the same H3 cells
# with population and jobs, and a fare structure), with the income of each
# cell from the Access to Opportunities Project.
poa_url <- "https://raw.githubusercontent.com/ipea/r5r/v2.5.0/r-package/inst/extdata/poa/"

prepare_poa <- function(cache = cap_cache()) {
  dir <- file.path(cache, "poa")
  done <- file.path(dir, "cells.csv")
  if (file.exists(done)) return(invisible(dir))
  dir.create(file.path(dir, "network"), recursive = TRUE, showWarnings = FALSE)
  for (f in c("poa_osm.pbf", "poa_eptc.zip", "poa_trensurb.zip")) {
    fetch(paste0(poa_url, f), file.path(dir, "network", f))
  }
  fetch(paste0(poa_url, "fares/fares_poa.zip"), file.path(dir, "fares_poa.zip"))
  grid <- utils::read.csv(fetch(paste0(poa_url, "poa_hexgrid.csv"), file.path(dir, "poa_hexgrid.csv")))
  pop <- utils::read.csv(fetch(sprintf(cap_sources$aop, "population_2010_poa.csv"),
                               file.path(cache, "raw", "population_2010_poa.csv")))
  cells <- merge(grid[, c("id", "lon", "lat", "population", "jobs")],
                 pop[, c("id_hex", "R001", "R002")], by.x = "id", by.y = "id_hex", all.x = TRUE)
  names(cells)[6:7] <- c("income", "quintile")
  cells$jobs[is.na(cells$jobs)] <- 0   # five cells of the sample have no count
  utils::write.csv(cells, done, row.names = FALSE)
  invisible(dir)
}

if (sys.nframe() == 0L) {
  for (k in names(cap_cities)) message("prepared ", prepare_capital(k))
  message("prepared ", prepare_poa())
}
