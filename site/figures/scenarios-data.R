# The data of the "before and after" examples (site/figures/scenarios.R and
# the worked examples of the R and Python sites):
#
# - Porto Alegre: the sample data of r5r 2.5.0 (streets, the EPTC and
#   Trensurb feeds, the hexagonal grid with population, jobs, schools and
#   health care, the fare structure and the scenario tables);
# - Helsinki: the streets of the capital region cut from OpenStreetMap, HSL's
#   GTFS feed cut to one weekday, and HSY's 250 m grids of residents and jobs.
#
#   Rscript site/figures/scenarios-data.R        # prepare both
#
# Everything is downloaded once into a cache directory (PEABIRU_SCENARIOS_CACHE,
# default: tools::R_user_dir("peabiru", "cache")/scenarios), about 880 MB of
# raw files; the prepared data is written to <cache>/poa/ and
# <cache>/helsinki/:
#
#   poa/network/          poa_osm.pbf, poa_eptc.zip, poa_trensurb.zip
#   poa/*                 the other sample files of r5r, as published
#   helsinki/network/     helsinki.osm.pbf, hsl.zip (one day of service)
#   helsinki/cells.csv    250 m cells with residents (HSY, 2025)
#   helsinki/jobs.csv     250 m cells with jobs (HSY, 2024)
#   helsinki/cells.gpkg   the cells with residents, as polygons
#
# Needs osmium (osmium-tool), sf, and network access the first time.

suppressPackageStartupMessages(library(sf))

sc_sources <- list(
  # r5r 2.5.0's sample data of Porto Alegre, at the tag of the release
  poa = "https://raw.githubusercontent.com/ipea/r5r/v2.5.0/r-package/inst/extdata/poa/",
  poa_files = c("poa_osm.pbf", "poa_eptc.zip", "poa_trensurb.zip", "poa_hexgrid.csv",
                "poa_points_of_interest.csv", "poa_osm_congestion.csv",
                "poa_poly_congestion.rds", "poa_osm_lts.csv", "poa_ls_lts.rds",
                "fares/fares_poa.zip"),
  # Geofabrik's extract of Finland, of 2026-11-11 (ODbL), and its MD5
  osm = "https://download.geofabrik.de/europe/finland-261111.osm.pbf",
  osm_md5 = "ff7c5c52ce53880611989d15378b13af",
  # HSL's GTFS feed (CC BY 4.0): replaced every day; the copy used here was
  # published on 2026-11-12 (feed_version 2026-11-12 05:42:11)
  gtfs = "https://infopalvelut.storage.hsldev.com/gtfs/hsl.zip",
  # HSY's open WFS: residents (2025) and jobs (2024) on a 250 m grid (CC BY 4.0)
  hsy = "https://kartta.hsy.fi/geoserver/wfs?service=WFS&version=2.0.0&request=GetFeature&typeNames=asuminen_ja_maankaytto:%s&outputFormat=application/json&srsName=EPSG:4326"
)

# The day and the area of the Helsinki example: a Tuesday, and a box around
# the capital region (Helsinki, Espoo, Vantaa, Kauniainen), which HSY's grid
# of residents covers, with a margin.
hel_date <- "20261117"
hel_box <- c(xmin = 24.45, ymin = 60.03, xmax = 25.30, ymax = 60.42)

sc_cache <- function() {
  d <- Sys.getenv("PEABIRU_SCENARIOS_CACHE",
                  file.path(tools::R_user_dir("peabiru", "cache"), "scenarios"))
  dir.create(file.path(d, "raw"), recursive = TRUE, showWarnings = FALSE)
  d
}

sc_fetch <- function(url, dest) {
  if (!file.exists(dest)) {
    message("downloading ", basename(dest))
    old <- options(timeout = 3600, HTTPUserAgent = "peabiru-docs-examples")
    on.exit(options(old))
    utils::download.file(url, paste0(dest, ".part"), mode = "wb", quiet = TRUE)
    file.rename(paste0(dest, ".part"), dest)
  }
  dest
}

# ------------------------------------------------------------------ Porto Alegre
prepare_poa <- function(cache = sc_cache()) {
  dir <- file.path(cache, "poa")
  dir.create(file.path(dir, "network"), recursive = TRUE, showWarnings = FALSE)
  for (f in sc_sources$poa_files) {
    dir.create(dirname(file.path(dir, f)), showWarnings = FALSE)
    sc_fetch(paste0(sc_sources$poa, f), file.path(dir, f))
  }
  # the network: streets and the two feeds (the sample's elevation raster is
  # left out: a scenario routes without slope costs, see the article)
  for (f in c("poa_osm.pbf", "poa_eptc.zip", "poa_trensurb.zip")) {
    if (!file.exists(file.path(dir, "network", f))) file.copy(file.path(dir, f), file.path(dir, "network"))
  }
  invisible(dir)
}

# ------------------------------------------------------------------ Helsinki

# The trips of a GTFS feed that run on `date` (YYYYMMDD), and every row of the
# other tables that belongs to them, copied line by line (no value changes);
# the tables peabiru does not read are left out.
gtfs_one_day <- function(src, date, out) {
  day <- c("sunday", "monday", "tuesday", "wednesday", "thursday", "friday",
           "saturday")[as.POSIXlt(as.Date(date, "%Y%m%d"))$wday + 1]
  files <- utils::unzip(src, list = TRUE)$Name
  table <- function(name) utils::read.csv(unz(src, name), colClasses = "character",
                                          na.strings = character(), check.names = FALSE,
                                          fileEncoding = "UTF-8-BOM")
  cal <- table("calendar.txt")
  active <- cal$service_id[cal$start_date <= date & cal$end_date >= date & cal[[day]] == "1"]
  if ("calendar_dates.txt" %in% files) {
    cd <- table("calendar_dates.txt")
    cd <- cd[cd$date == date, ]
    active <- union(setdiff(active, cd$service_id[cd$exception_type == "2"]),
                    cd$service_id[cd$exception_type == "1"])
  }
  trips <- table("trips.txt")
  trips <- trips[trips$service_id %in% active, ]
  keep <- list(trips.txt = c("trip_id", trips$trip_id), stop_times.txt = c("trip_id", trips$trip_id),
               frequencies.txt = c("trip_id", trips$trip_id),
               routes.txt = c("route_id", unique(trips$route_id)),
               shapes.txt = c("shape_id", unique(trips$shape_id)),
               calendar.txt = c("service_id", active), calendar_dates.txt = c("service_id", active))
  tables <- c("agency.txt", "stops.txt", "routes.txt", "trips.txt", "stop_times.txt",
              "calendar.txt", "calendar_dates.txt", "frequencies.txt", "transfers.txt",
              "shapes.txt", "feed_info.txt", "fare_attributes.txt", "fare_rules.txt")
  tmp <- file.path(tempdir(), "gtfs-one-day")
  unlink(tmp, recursive = TRUE)
  dir.create(tmp)
  for (name in intersect(tables, files)) {
    con <- unz(src, name, open = "rb")
    outcon <- file(file.path(tmp, name), "wb")
    header <- readLines(con, n = 1, warn = FALSE)
    writeLines(header, outcon, sep = "\n")
    k <- keep[[name]]
    col <- if (is.null(k)) NA else match(k[1], strsplit(sub("^﻿", "", sub("\r$", "", header)), ",")[[1]])
    repeat {
      lines <- readLines(con, n = 500000, warn = FALSE)
      if (length(lines) == 0) break
      if (!is.na(col)) {
        values <- utils::read.csv(text = lines, header = FALSE, colClasses = "character",
                                  na.strings = character(), quote = "\"")[[col]]
        stopifnot(length(values) == length(lines))   # no line breaks inside fields
        lines <- lines[values %in% k[-1]]
      }
      writeLines(lines, outcon, sep = "\n")
    }
    close(con)
    close(outcon)
  }
  out <- file.path(normalizePath(dirname(out)), basename(out))
  unlink(out)
  old <- setwd(tmp)
  on.exit(setwd(old))
  stopifnot(utils::zip(out, intersect(tables, files), flags = "-q9X") == 0)
  message(sprintf("%s (%s): %d services, %d trips", date, day, length(active), nrow(trips)))
  invisible(out)
}

prepare_helsinki <- function(cache = sc_cache()) {
  raw <- file.path(cache, "raw")
  dir <- file.path(cache, "helsinki")
  dir.create(file.path(dir, "network"), recursive = TRUE, showWarnings = FALSE)

  # streets: the box, cut from Geofabrik's extract of Finland
  streets <- file.path(dir, "network", "helsinki.osm.pbf")
  if (!file.exists(streets)) {
    pbf <- sc_fetch(sc_sources$osm, file.path(raw, basename(sc_sources$osm)))
    stopifnot(unname(tools::md5sum(pbf)) == sc_sources$osm_md5)
    status <- system2("osmium", c("extract", "--overwrite", "-s", "smart", "-b",
                                  paste(hel_box, collapse = ","), pbf, "-o", streets))
    stopifnot(status == 0)
  }

  # public transport: HSL's feed, cut to the trips of one Tuesday
  if (!file.exists(file.path(dir, "network", "hsl.zip"))) {
    gtfs <- sc_fetch(sc_sources$gtfs, file.path(raw, "hsl.zip"))
    gtfs_one_day(gtfs, hel_date, file.path(dir, "network", "hsl.zip"))
  }

  # residents and jobs on HSY's 250 m grid; each cell is placed at its centre
  done <- file.path(dir, "jobs.csv")
  if (file.exists(done)) return(invisible(dir))
  grid <- function(layer) {
    x <- st_read(sc_fetch(sprintf(sc_sources$hsy, layer), file.path(raw, paste0(layer, ".geojson"))),
                 quiet = TRUE)
    xy <- st_coordinates(st_transform(st_centroid(st_geometry(st_transform(x, 3067))), 4326))
    list(sf = x, lon = round(xy[, 1], 6), lat = round(xy[, 2], 6))
  }
  pop <- grid("Vaestotietoruudukko_2025")
  cells <- data.frame(id = as.character(pop$sf$index), lon = pop$lon, lat = pop$lat,
                      population = pop$sf$asukkaita)
  # HSY puts the residents it cannot place in a building in one cell in the
  # sea, at the south-east corner of the grid (33,575 people in 2025): it is
  # not a place, and is left out
  sea <- which.max(cells$population)
  stopifnot(cells$lat[sea] == min(cells$lat), cells$lon[sea] == max(cells$lon))
  message(sprintf("left out: cell %s in the sea, %d residents without a building",
                  cells$id[sea], cells$population[sea]))
  st_write(st_sf(cells, geometry = st_geometry(pop$sf))[-sea, ], file.path(dir, "cells.gpkg"),
           delete_dsn = TRUE, quiet = TRUE)
  write.csv(cells[-sea, ], file.path(dir, "cells.csv"), row.names = FALSE)
  # jobs: tphklkm is the number of jobs in the cell, -1 where there are 1 to 3
  # (suppressed for privacy), counted here as 0. The grid covers the
  # wider Helsinki region; the box keeps the capital region,
  # and leaves out the cells in the sea (near 59.94 N, 25.23 E) where HSY puts
  # the jobs of establishments without a location
  jb <- grid("Tyopaikkaruudukko_2024")
  jobs <- data.frame(id = jb$sf$id, lon = jb$lon, lat = jb$lat,
                     jobs = pmax(jb$sf$tphklkm, 0), suppressed = jb$sf$tphklkm < 0)
  inside <- jobs$lon > hel_box[["xmin"]] & jobs$lon < hel_box[["xmax"]] &
    jobs$lat > hel_box[["ymin"]] & jobs$lat < hel_box[["ymax"]]
  message(sprintf("jobs: %d in the grid, %d in the box, %d in the sea cells", sum(jobs$jobs),
                  sum(jobs$jobs[inside]), sum(jobs$jobs[jobs$lat < 60])))
  write.csv(jobs[inside, ], done, row.names = FALSE)
  invisible(dir)
}

if (sys.nframe() == 0L) {
  message("prepared ", prepare_poa())
  message("prepared ", prepare_helsinki())
}
