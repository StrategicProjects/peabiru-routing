# The data of the Pernambuco examples (site/figures/pernambuco.R, and the
# terrain example of site/figures/terrain.R) and of the worked examples of the
# R and Python sites: street networks cut from OpenStreetMap, a hexagonal grid
# with the population of the Censo 2022, public schools and primary health
# care units with their official coordinates and, for the terrain example, an
# elevation raster cut from the Copernicus DEM.
#
#   Rscript site/figures/pernambuco-data.R            # Recife and Caruaru
#   Rscript site/figures/pernambuco-data.R rmr olinda # the ones named
#
# Everything is downloaded once into a cache directory (PEABIRU_PE_CACHE,
# default: tools::R_user_dir("peabiru", "cache")/pernambuco), about 640 MB
# of raw files for Recife and Caruaru (Salvador adds the census tracts of
# Bahia); the prepared data of each city is written to <cache>/<city>/:
#
#   network/<city>.osm.pbf  the streets of the municipality plus a buffer
#   cut.geojson             that area (the municipality buffered)
#   boundary.geojson        the municipality (IBGE); every one of them, with
#                           its code, for a region of several
#   cells.gpkg, cells.csv   hexagons 500 m apart over the municipality, with
#                           their population (Censo 2022, by census tract)
#   cells_muni.csv          the population of each cell by municipality
#   schools.csv             public schools in operation in the cut area (INEP)
#   health.csv              primary health care units in the cut area (CNES)
#
# and, by prepare_dem():
#
#   network-dem/            the same extract and <city>_dem.tif, the
#                           Copernicus DEM GLO-30 over its streets
#   network-dem90/          the same with GLO-90 (prepare_dem(arcsec = 3))
#
# Needs osmium (osmium-tool), sf, and network access the first time. The same
# steps are written out in the worked examples, in R and in Python.

suppressPackageStartupMessages(library(sf))

# The sources: fixed, dated files where the publisher offers them.
pe_sources <- list(
  # Geofabrik's extract of the Nordeste region of Brazil, of 2026-11-01 (ODbL)
  osm = "https://download.geofabrik.de/south-america/brazil/nordeste-261101.osm.pbf",
  osm_md5 = "93318fcddd85a53e8386eaa93588847c",
  # IBGE, Censo Demográfico 2022: census tracts of a state with their
  # population (v0001), "malha com atributos" of 2024-11-13 (%s: PE, BA)
  tracts = "https://ftp.ibge.gov.br/Censos/Censo_Demografico_2022/Agregados_por_Setores_Censitarios/malha_com_atributos/setores/gpkg/UF/%s/%s_setores_CD2022.gpkg",
  # INEP, Catálogo de Escolas 2023, as published by Ipea's geobr (schools_2023)
  schools = "https://www.ipea.gov.br/geobr/data_gpkg/schools/2023/schools_2023.gpkg",
  # Ministério da Saúde, CNES: every establishment, updated daily (not dated)
  cnes = "https://s3.sa-east-1.amazonaws.com/ckan.saude.gov.br/CNES/cnes_estabelecimentos_csv.zip",
  # IBGE's API of municipal boundaries (malha municipal)
  boundary = "https://servicodados.ibge.gov.br/api/v3/malhas/municipios/%s?formato=application/vnd.geo+json&qualidade=maxima",
  # Copernicus DEM (a surface model), GLO-30 at 1 arc second and GLO-90 at 3,
  # one tile per degree, as Cloud Optimised GeoTIFFs on the AWS open data
  # registry; read in place (%d: 30 and 10, or 90 and 30; %s: S09, W035)
  dem = "https://copernicus-dem-%dm.s3.amazonaws.com/Copernicus_DSM_COG_%d_%s_00_%s_00_DEM/Copernicus_DSM_COG_%d_%s_00_%s_00_DEM.tif"
)

pe_cities <- list(
  recife = list(code = "2611606", name = "Recife", buffer = 10000,
                origin = c(lon = -34.8709722, lat = -8.063374), origin_name = "Marco Zero"),
  caruaru = list(code = "2604106", name = "Caruaru", buffer = 5000,
                 origin = c(lon = -35.9713866, lat = -8.2897512), origin_name = "Feira de Caruaru"),
  # the terrain example (site/figures/terrain.R)
  olinda = list(code = "2609600", name = "Olinda", buffer = 3000, spacing = 250,
                origin = c(lon = -34.8408, lat = -8.0136), origin_name = "Alto da Sé"),
  jaboatao = list(code = "2607901", name = "Jaboatão dos Guararapes", buffer = 5000),
  # on the São Francisco, facing Juazeiro (Bahia): CNES rows of both states
  petrolina = list(code = "2611101", name = "Petrolina", buffer = 5000, utm = 31984,
                   cnes_uf = c("26", "29")),
  garanhuns = list(code = "2606002", name = "Garanhuns", buffer = 5000, utm = 31984),
  # the Região Metropolitana do Recife: its 14 municipalities (Lei
  # Complementar estadual 10/1994 as amended; Goiana joined in 2018 and left
  # in 2020, LC 426/2020) as one network
  rmr = list(code = c("2611606", "2609600", "2607901", "2610707", "2603454", "2613701",
                      "2602902", "2607208", "2609402", "2606804", "2600054", "2607752",
                      "2607604", "2601052"),
             name = "Região Metropolitana do Recife", buffer = 3000),
  salvador = list(code = "2927408", name = "Salvador", uf = "BA", uf_code = "29", buffer = 3000,
                  utm = 31984,
                  origin = c(lon = -38.5128, lat = -12.9740), origin_name = "Praça Municipal")
)

utm <- 31985     # SIRGAS 2000 / UTM zone 25S: metres, for both cities
spacing <- 500   # distance between the centres of neighbouring cells (m)
# the fields a city may leave out: its UTM zone (25S), state (PE), cell
# spacing (500 m) and the states whose CNES rows are read (its own)
city_get <- function(city, field) {
  default <- list(utm = utm, uf = "PE", uf_code = "26", spacing = spacing)
  if (field == "cnes_uf") {
    return(if (is.null(city$cnes_uf)) city_get(city, "uf_code") else city$cnes_uf)
  }
  if (is.null(city[[field]])) default[[field]] else city[[field]]
}

pe_cache <- function() {
  d <- Sys.getenv("PEABIRU_PE_CACHE",
                  file.path(tools::R_user_dir("peabiru", "cache"), "pernambuco"))
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

osmium <- function(...) {
  status <- system2("osmium", c(...))
  if (status != 0) stop("osmium ", paste(c(...)[1], collapse = " "), " failed")
}

# Regular hexagons, pointy side up, centres `spacing` apart in rows offset by
# half a cell, over the bounding box of `area` (projected); the ones that
# overlap it, numbered row by row from the south-west.
hex_grid <- function(area, spacing) {
  b <- st_bbox(area)
  h <- spacing * sqrt(3) / 2
  rows <- -1:(ceiling((b[["ymax"]] - b[["ymin"]]) / h) + 1)
  cols <- -1:(ceiling((b[["xmax"]] - b[["xmin"]]) / spacing) + 1)
  centres <- expand.grid(i = cols, j = rows)
  centres$x <- b[["xmin"]] + centres$i * spacing + (centres$j %% 2) * spacing / 2
  centres$y <- b[["ymin"]] + centres$j * h
  r <- spacing / sqrt(3)
  k <- (30 + 60 * 0:6) * pi / 180
  cells <- st_sfc(lapply(seq_len(nrow(centres)), function(n) {
    st_polygon(list(cbind(centres$x[n] + r * cos(k), centres$y[n] + r * sin(k))))
  }), crs = st_crs(area))
  keep <- lengths(st_intersects(cells, area)) > 0
  st_sf(id = seq_len(sum(keep)), geometry = cells[keep])
}

prepare_city <- function(key, cache = pe_cache()) {
  city <- pe_cities[[key]]
  utm <- city_get(city, "utm")
  uf <- city_get(city, "uf")
  raw <- file.path(cache, "raw")
  dir <- file.path(cache, key)
  dir.create(file.path(dir, "network"), recursive = TRUE, showWarnings = FALSE)
  done <- file.path(dir, "health.csv")
  if (file.exists(done)) return(invisible(dir))

  # the municipality, and the area cut from OpenStreetMap: the municipality
  # and a buffer, so that trips near its border can reach streets, schools
  # and health units in the neighbouring municipalities. A region of several
  # municipalities: their union, and boundary.geojson has each of them.
  munis <- lapply(city$code, function(code) {
    b <- st_read(fetch(sprintf(pe_sources$boundary, code),
                       file.path(raw, paste0(code, ".geojson"))), quiet = TRUE)
    st_transform(st_make_valid(b), utm)
  })
  if (length(munis) == 1) {
    boundary <- munis[[1]]
    st_write(st_transform(boundary, 4326), file.path(dir, "boundary.geojson"),
             delete_dsn = TRUE, quiet = TRUE)
  } else {
    # neighbouring boundaries do not share their vertices exactly: snapped
    # to a 1 cm grid, they union without topology errors
    each <- st_sf(code = city$code, geometry = do.call(c, lapply(munis, st_geometry)))
    each <- st_make_valid(st_set_precision(each, 100))
    boundary <- st_sf(geometry = st_union(each))
    st_write(st_transform(each, 4326), file.path(dir, "boundary.geojson"),
             delete_dsn = TRUE, quiet = TRUE)
  }
  cut <- st_transform(st_buffer(boundary, city$buffer), 4326)
  st_write(cut, file.path(dir, "cut.geojson"), delete_dsn = TRUE, quiet = TRUE)

  nordeste <- fetch(pe_sources$osm, file.path(raw, basename(pe_sources$osm)))
  osmium("extract", "--overwrite", "-p", file.path(dir, "cut.geojson"), nordeste,
         "-o", file.path(dir, "network", paste0(key, ".osm.pbf")))

  # hexagonal cells with their population, from the census tracts by the
  # share of each tract's area that falls in each cell
  tracts_url <- sprintf(pe_sources$tracts, uf, uf)
  tracts <- st_read(fetch(tracts_url, file.path(raw, basename(tracts_url))),
                    query = sprintf("SELECT CD_SETOR, CD_MUN, v0001, geom FROM %s_setores_CD2022 WHERE CD_MUN IN (%s)",
                                    uf, paste0("'", city$code, "'", collapse = ", ")), quiet = TRUE)
  tracts <- st_transform(st_make_valid(tracts), utm)
  tracts$tract_area <- as.numeric(st_area(tracts))
  cells <- hex_grid(boundary, city_get(city, "spacing"))
  parts <- suppressWarnings(st_intersection(cells, tracts[, c("CD_MUN", "v0001", "tract_area")]))
  parts$people <- parts$v0001 * as.numeric(st_area(parts)) / parts$tract_area
  pop <- tapply(parts$people, parts$id, sum)
  cells$population <- 0
  cells$population[match(as.integer(names(pop)), cells$id)] <- as.numeric(pop)
  xy <- st_coordinates(st_transform(st_centroid(st_geometry(cells)), 4326))
  cells$lon <- round(xy[, 1], 6)
  cells$lat <- round(xy[, 2], 6)
  st_write(st_transform(cells, 4326), file.path(dir, "cells.gpkg"), delete_dsn = TRUE, quiet = TRUE)
  write.csv(st_drop_geometry(cells)[, c("id", "lon", "lat", "population")],
            file.path(dir, "cells.csv"), row.names = FALSE)
  # the residents of each cell by the municipality of their tract
  by_muni <- aggregate(people ~ id + CD_MUN, data = st_drop_geometry(parts), FUN = sum)
  names(by_muni) <- c("id", "code", "population")
  write.csv(by_muni[order(by_muni$id, by_muni$code), ], file.path(dir, "cells_muni.csv"),
            row.names = FALSE)

  # public schools in operation (INEP's catalogue); the ones without
  # coordinates cannot be placed and are left out
  cut_4674 <- st_transform(cut, 4674)
  schools <- st_read(fetch(pe_sources$schools, file.path(raw, basename(pe_sources$schools))),
                     query = paste("SELECT code_school, name_school, name_muni, geom FROM schools_2023",
                                   "WHERE admin_category = 'Pública' AND service_restriction =",
                                   "'ESCOLA EM FUNCIONAMENTO E SEM RESTRIÇÃO DE ATENDIMENTO'"),
                     wkt_filter = st_as_text(st_as_sfc(st_bbox(cut_4674))), quiet = TRUE)
  schools <- schools[!st_is_empty(schools), ]
  schools <- schools[lengths(st_intersects(schools, cut_4674)) > 0, ]
  xy <- st_coordinates(schools)
  write.csv(data.frame(id = schools$code_school, name = schools$name_school,
                       municipality = schools$name_muni, lon = xy[, 1], lat = xy[, 2]),
            file.path(dir, "schools.csv"), row.names = FALSE)

  # primary health care units (CNES types 1, posto de saúde, and 2, centro de
  # saúde / unidade básica), active, with coordinates, in the cut area
  zip <- fetch(pe_sources$cnes, file.path(raw, basename(pe_sources$cnes)))
  cnes <- do.call(rbind, lapply(city_get(city, "cnes_uf"), function(u) read_cnes_uf(zip, raw, u)))
  cnes <- cnes[cnes$TP_UNIDADE %in% c("1", "2") & cnes$CO_MOTIVO_DESAB == "" &
                 cnes$NU_LATITUDE != "" & cnes$NU_LONGITUDE != "", ]
  units <- st_as_sf(data.frame(id = cnes$CO_CNES, name = cnes$NO_FANTASIA,
                               municipality_code = cnes$CO_IBGE,
                               lon = as.numeric(cnes$NU_LONGITUDE),
                               lat = as.numeric(cnes$NU_LATITUDE)),
                    coords = c("lon", "lat"), crs = 4674, remove = FALSE)
  units <- units[lengths(st_intersects(units, st_transform(cut, 4674))) > 0, ]
  write.csv(st_drop_geometry(units), done, row.names = FALSE)
  invisible(dir)
}

# The establishments of one state (CO_UF: 26 Pernambuco, 29 Bahia): the lines
# of the national file whose third field is the state's code, copied byte for
# byte into the cache (cnes_<code>.csv), and read from there. (Parsing the
# national file and writing it again lost rows of Bahia: some of its names
# have characters that do not convert to every locale's encoding.)
read_cnes_uf <- function(zip, raw, uf = "26") {
  one <- file.path(raw, paste0("cnes_", uf, ".csv"))
  if (!file.exists(one)) {
    con <- unz(zip, "cnes_estabelecimentos.csv", open = "rb")
    l <- readLines(con, warn = FALSE)
    close(con)
    keep <- c(1L, which(grepl(sprintf('^[^;]*;[^;]*;"?%s"?;', uf), l[-1], useBytes = TRUE)) + 1L)
    writeLines(l[keep], paste0(one, ".part"), useBytes = TRUE)
    file.rename(paste0(one, ".part"), one)
  }
  out <- utils::read.csv2(one, colClasses = "character", fileEncoding = "CP1252",
                          na.strings = character())
  lines <- length(readLines(one, warn = FALSE)) - 1L
  if (nrow(out) != lines || any(out$CO_UF != uf)) {
    stop(basename(one), ": read ", nrow(out), " rows of ", lines, " lines")
  }
  out
}

# The elevation raster of a city: the Copernicus DEM over the box of its
# streets, read in place from the tiles on AWS (the ocean has no tiles: 0 m,
# as their documentation says) and written on the tiles' own grid (a crop, no
# resampling) next to a link to the street extract, in network-dem/; network/
# stays the network without elevation. `arcsec`: 1 for GLO-30, the default,
# or 3 for GLO-90 (the same model averaged to 3 arc seconds by its
# publisher), which goes in network-dem90/. GDAL through sf.
prepare_dem <- function(key, cache = pe_cache(), arcsec = 1) {
  dir <- prepare_city(key, cache)
  tag <- if (arcsec == 1) "" else as.character(30 * arcsec)
  out_dir <- file.path(dir, paste0("network-dem", tag))
  tif <- file.path(out_dir, paste0(key, "_dem", tag, ".tif"))
  if (file.exists(tif)) return(invisible(out_dir))
  dir.create(out_dir, showWarnings = FALSE)
  pbf <- file.path(dir, "network", paste0(key, ".osm.pbf"))
  link <- file.path(out_dir, basename(pbf))
  if (!file.exists(link) && !suppressWarnings(file.link(pbf, link))) file.copy(pbf, link)
  # the box of the extract's streets, not of the cut: osmium keeps whole the
  # ways that cross the cut, and peabiru takes points off the raster as 0 m
  streets <- file.path(tempdir(), paste0(key, "-streets.osm.pbf"))
  osmium("tags-filter", "--overwrite", pbf, "w/highway", "-o", streets)
  box <- system2("osmium", c("fileinfo", "-e", "-g", "data.bbox", streets), stdout = TRUE)
  unlink(streets)
  b <- setNames(as.numeric(strsplit(gsub("[()]", "", box), ",")[[1]]),
                c("xmin", "ymin", "xmax", "ymax"))
  # pixels are centred on the grid of arc seconds: their edges are half a
  # pixel off
  px <- arcsec / 3600
  edge <- function(v, f) (f(v / px - 0.5) + 0.5) * px
  te <- c(edge(b[["xmin"]], floor), edge(b[["ymin"]], floor),
          edge(b[["xmax"]], ceiling), edge(b[["ymax"]], ceiling))
  tiles <- character()
  for (lat in floor(te[2]):floor(te[4])) for (lon in floor(te[1]):floor(te[3])) {
    ns <- sprintf("%s%02d", if (lat < 0) "S" else "N", abs(lat))
    ew <- sprintf("%s%03d", if (lon < 0) "W" else "E", abs(lon))
    url <- sprintf(pe_sources$dem, 30 * arcsec, 10 * arcsec, ns, ew, 10 * arcsec, ns, ew)
    status <- tryCatch(attr(curlGetHeaders(url), "status"), error = function(e) NA)
    if (identical(status, 200L)) tiles <- c(tiles, paste0("/vsicurl/", url))
  }
  if (!length(tiles)) stop("no Copernicus DEM tile over ", key)
  old <- Sys.getenv("GDAL_HTTP_USERAGENT")
  Sys.setenv(GDAL_HTTP_USERAGENT = "peabiru-docs-examples")
  on.exit(Sys.setenv(GDAL_HTTP_USERAGENT = old))
  part <- file.path(out_dir, "part.tif.tmp")
  sf::gdal_utils("warp", tiles, part, options = c(
    "-of", "GTiff", "-te", sprintf("%.10f", te), "-tr", sprintf("%.15f", px), sprintf("%.15f", px),
    "-r", "near", "-wo", "INIT_DEST=0", "-ot", "Float32",
    "-co", "COMPRESS=DEFLATE", "-co", "PREDICTOR=3", "-co", "TILED=YES"))
  file.rename(part, tif)
  message(sprintf("%-22s %d tiles, %6.1f MB", basename(tif), length(tiles), file.size(tif) / 1e6))
  invisible(out_dir)
}

# The extracts of the browser demo (site/demo/data/pe/): Recife's
# municipality, and Caruaru's urban area (14 x 12 km around the centre: the
# municipality is mostly rural, and on a whole-municipality map the city
# would be a small patch), without buildings and without the metadata of each object (version,
# timestamp, author), which peabiru does not read. Ways that are not
# buildings are kept, since their nodes split the streets as in the full
# extract: the network is the same as one cut from the full file.
demo_extract <- function(key, out_dir, cache = pe_cache()) {
  city <- pe_cities[[key]]
  dir <- file.path(cache, key)
  tmp <- file.path(tempdir(), paste0("demo-", key))
  dir.create(tmp, showWarnings = FALSE)
  dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)
  muni <- file.path(tmp, "area.osm.pbf")
  if (key == "caruaru") {
    o <- st_transform(st_sfc(st_point(city$origin), crs = 4326), utm)[[1]]
    box <- st_bbox(st_transform(st_sfc(st_polygon(list(cbind(
      o[1] + c(-7000, 7000, 7000, -7000, -7000), o[2] + c(-6000, -6000, 6000, 6000, -6000)))),
      crs = utm), 4326))
    area <- c("-b", paste(round(box[c("xmin", "ymin", "xmax", "ymax")], 5), collapse = ","))
  } else {
    area <- c("-p", file.path(dir, "boundary.geojson"))
  }
  osmium("extract", "--overwrite", area, file.path(dir, "network", paste0(key, ".osm.pbf")),
         "-o", muni)
  osmium("tags-filter", "--overwrite", "-i", muni, "w/building", "-o", file.path(tmp, "nb.osm.pbf"))
  ways <- system2("osmium", c("cat", "-t", "way", "-f", "opl", file.path(tmp, "nb.osm.pbf")), stdout = TRUE)
  osmium("tags-filter", "--overwrite", "-R", muni, "r/type=restriction", "r/park_ride",
         "-o", file.path(tmp, "rel.osm.pbf"))
  rels <- system2("osmium", c("cat", "-t", "relation", "-f", "opl", file.path(tmp, "rel.osm.pbf")), stdout = TRUE)
  ids <- file.path(tmp, "ids.txt")
  writeLines(sub(" .*", "", c(ways, rels)), ids)
  out <- file.path(out_dir, paste0(key, ".osm.pbf"))
  # getid exits with 1 when some referenced object is outside the extract
  system2("osmium", c("getid", "--overwrite", "-r", muni, "-i", ids,
                      "-f", "pbf,add_metadata=false", "-o", out))
  if (!file.exists(out)) stop("osmium getid did not write ", out)
  message(sprintf("%-22s %6.2f MB", basename(out), file.size(out) / 1e6))
  out
}

if (sys.nframe() == 0L) {
  keys <- commandArgs(TRUE)
  if (length(keys) == 0) keys <- c("recife", "caruaru")
  for (k in keys) message("prepared ", prepare_city(k))
}
