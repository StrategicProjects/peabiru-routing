# The data of the Recife and Caruaru examples (site/figures/pernambuco.R and
# the worked examples of the R and Python sites): street networks cut from
# OpenStreetMap, a hexagonal grid with the population of the Censo 2022, public
# schools and primary health care units with their official coordinates.
#
#   Rscript site/figures/pernambuco-data.R        # prepare both cities
#
# Everything is downloaded once into a cache directory (PEABIRU_PE_CACHE,
# default: tools::R_user_dir("peabiru", "cache")/pernambuco), about 640 MB
# of raw files; the prepared data of each city is written to <cache>/<city>/:
#
#   network/<city>.osm.pbf  the streets of the municipality plus a buffer
#   cut.geojson             that area (the municipality buffered)
#   boundary.geojson        the municipality (IBGE)
#   cells.gpkg, cells.csv   hexagons 500 m apart over the municipality, with
#                           their population (Censo 2022, by census tract)
#   schools.csv             public schools in operation in the cut area (INEP)
#   health.csv              primary health care units in the cut area (CNES)
#
# Needs osmium (osmium-tool), sf, and network access the first time. The same
# steps are written out in the worked examples, in R and in Python.

suppressPackageStartupMessages(library(sf))

# The sources: fixed, dated files where the publisher offers them.
pe_sources <- list(
  # Geofabrik's extract of the Nordeste region of Brazil, of 2026-11-01 (ODbL)
  osm = "https://download.geofabrik.de/south-america/brazil/nordeste-261101.osm.pbf",
  osm_md5 = "93318fcddd85a53e8386eaa93588847c",
  # IBGE, Censo Demográfico 2022: census tracts of Pernambuco with their
  # population (v0001), "malha com atributos" of 2024-11-13
  tracts = "https://ftp.ibge.gov.br/Censos/Censo_Demografico_2022/Agregados_por_Setores_Censitarios/malha_com_atributos/setores/gpkg/UF/PE/PE_setores_CD2022.gpkg",
  # INEP, Catálogo de Escolas 2023, as published by Ipea's geobr (schools_2023)
  schools = "https://www.ipea.gov.br/geobr/data_gpkg/schools/2023/schools_2023.gpkg",
  # Ministério da Saúde, CNES: every establishment, updated daily (not dated)
  cnes = "https://s3.sa-east-1.amazonaws.com/ckan.saude.gov.br/CNES/cnes_estabelecimentos_csv.zip",
  # IBGE's API of municipal boundaries (malha municipal)
  boundary = "https://servicodados.ibge.gov.br/api/v3/malhas/municipios/%s?formato=application/vnd.geo+json&qualidade=maxima"
)

pe_cities <- list(
  recife = list(code = "2611606", name = "Recife", buffer = 10000,
                origin = c(lon = -34.8709722, lat = -8.063374), origin_name = "Marco Zero"),
  caruaru = list(code = "2604106", name = "Caruaru", buffer = 5000,
                 origin = c(lon = -35.9713866, lat = -8.2897512), origin_name = "Feira de Caruaru")
)

utm <- 31985     # SIRGAS 2000 / UTM zone 25S: metres, for both cities
spacing <- 500   # distance between the centres of neighbouring cells (m)

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
  raw <- file.path(cache, "raw")
  dir <- file.path(cache, key)
  dir.create(file.path(dir, "network"), recursive = TRUE, showWarnings = FALSE)
  done <- file.path(dir, "health.csv")
  if (file.exists(done)) return(invisible(dir))

  # the municipality, and the area cut from OpenStreetMap: the municipality
  # and a buffer, so that trips near its border can reach streets, schools
  # and health units in the neighbouring municipalities
  boundary <- st_read(fetch(sprintf(pe_sources$boundary, city$code),
                            file.path(raw, paste0(city$code, ".geojson"))), quiet = TRUE)
  boundary <- st_transform(st_make_valid(boundary), utm)
  cut <- st_transform(st_buffer(boundary, city$buffer), 4326)
  st_write(st_transform(boundary, 4326), file.path(dir, "boundary.geojson"),
           delete_dsn = TRUE, quiet = TRUE)
  st_write(cut, file.path(dir, "cut.geojson"), delete_dsn = TRUE, quiet = TRUE)

  nordeste <- fetch(pe_sources$osm, file.path(raw, basename(pe_sources$osm)))
  osmium("extract", "--overwrite", "-p", file.path(dir, "cut.geojson"), nordeste,
         "-o", file.path(dir, "network", paste0(key, ".osm.pbf")))

  # hexagonal cells with their population, from the census tracts by the
  # share of each tract's area that falls in each cell
  tracts <- st_read(fetch(pe_sources$tracts, file.path(raw, basename(pe_sources$tracts))),
                    query = sprintf("SELECT CD_SETOR, v0001, geom FROM PE_setores_CD2022 WHERE CD_MUN = '%s'",
                                    city$code), quiet = TRUE)
  tracts <- st_transform(st_make_valid(tracts), utm)
  tracts$tract_area <- as.numeric(st_area(tracts))
  cells <- hex_grid(boundary, spacing)
  parts <- suppressWarnings(st_intersection(cells, tracts[, c("v0001", "tract_area")]))
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
  cnes <- read_cnes_pe(zip, raw)
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

# The establishments of Pernambuco (CO_UF 26), read once from the national
# file and kept in the cache.
read_cnes_pe <- function(zip, raw) {
  pe <- file.path(raw, "cnes_pe.csv")
  if (!file.exists(pe)) {
    all <- utils::read.csv2(unz(zip, "cnes_estabelecimentos.csv"), colClasses = "character",
                            fileEncoding = "latin1", na.strings = character())
    utils::write.csv(all[all$CO_UF == "26", ], pe, row.names = FALSE, fileEncoding = "UTF-8")
  }
  utils::read.csv(pe, colClasses = "character", na.strings = character(), encoding = "UTF-8")
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
  for (k in names(pe_cities)) message("prepared ", prepare_city(k))
}
