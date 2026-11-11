# The Recife and Caruaru figures of the documentation sites, computed with the
# peabiru R package on the data prepared by site/figures/pernambuco-data.R
# (OpenStreetMap streets, Censo 2022 population, INEP schools, CNES health
# units).
#
#   Rscript site/figures/pernambuco.R             # all of them
#   Rscript site/figures/pernambuco.R pe_share    # only the ones named
#
# or `bash site/figures/make.sh --pernambuco`. The first run downloads about
# 640 MB into the cache of pernambuco-data.R and needs osmium.

root_dir <- local({
  f <- sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE))
  normalizePath(file.path(dirname(f), "..", ".."))
})
source(file.path(root_dir, "site", "figures", "common.R"))
source(file.path(root_dir, "site", "figures", "pernambuco-data.R"))

threads <- 4
cache <- pe_cache()

city_data <- function(key) {
  dir <- prepare_city(key, cache)
  cells <- st_read(file.path(dir, "cells.gpkg"), quiet = TRUE)
  cells <- st_transform(cells[cells$population > 0, ], 3857)
  list(
    key = key, city = pe_cities[[key]],
    network = build_network(file.path(dir, "network"), n_threads = threads),
    cells = cells,
    points = st_drop_geometry(cells)[, c("id", "lon", "lat", "population")],
    schools = read.csv(file.path(dir, "schools.csv"), colClasses = c(id = "character")),
    health = read.csv(file.path(dir, "health.csv"), colClasses = c(id = "character")),
    boundary = st_transform(st_read(file.path(dir, "boundary.geojson"), quiet = TRUE), 3857),
    pbf = file.path(dir, "network", paste0(key, ".osm.pbf"))
  )
}
pe <- list()
get_city <- function(key) {
  if (is.null(pe[[key]])) pe[[key]] <<- city_data(key)
  pe[[key]]
}
bg_cache <- list()
get_bg <- function(key) {
  if (is.null(bg_cache[[key]])) bg_cache[[key]] <<- background(get_city(key)$pbf)
  bg_cache[[key]]
}

origin_of <- function(d) {
  o <- d$city$origin
  data.frame(id = d$city$origin_name, lon = o[["lon"]], lat = o[["lat"]])
}
origin_3857_of <- function(d) {
  st_transform(st_as_sf(origin_of(d), coords = c("lon", "lat"), crs = 4326), 3857)
}

# Map extents (EPSG:3857). The isochrones: a box around the origin, west of
# Marco Zero (on the coast) and around the Feira de Caruaru. The grids:
# Recife's municipality, and Caruaru's urban area (the municipality is mostly
# rural, 40 km from north to south).
box_around <- function(d, west, east, south, north) {
  xy <- st_coordinates(origin_3857_of(d))
  st_bbox(c(xmin = xy[1] - west, xmax = xy[1] + east, ymin = xy[2] - south,
            ymax = xy[2] + north), crs = st_crs(3857))
}
iso_extent <- function(d) {
  if (d$key == "recife") box_around(d, 7500, 2500, 7000, 6000)
  else box_around(d, 5000, 5000, 4000, 4000)
}
extent_of <- function(d) {
  if (d$key == "recife") {
    st_bbox(st_buffer(st_as_sfc(st_bbox(d$boundary)), 400))
  } else {
    box_around(d, 7000, 7000, 6000, 6000)
  }
}

# Only what falls in the map is drawn: everything else would still be written
# to the SVG, outside its clipping box.
crop <- function(x, extent) {
  box <- st_as_sfc(st_bbox(st_buffer(st_as_sfc(extent), 200)))
  x[lengths(st_intersects(x, box)) > 0]
}
pe_map <- function(d, extent) {
  bg <- get_bg(d$key)
  lines <- bg$lines[lengths(st_intersects(bg$lines, st_as_sfc(extent))) > 0, ]
  list(
    geom_sf(data = crop(bg$water, extent), fill = light("water"), colour = NA),
    geom_sf(data = lines, aes(linewidth = w), colour = light("street"), show.legend = FALSE),
    scale_linewidth_identity(),
    geom_sf(data = st_geometry(d$boundary), fill = NA, colour = light("soft"),
            linewidth = 0.3, linetype = "22")
  )
}
pe_coords <- function(extent) {
  coord_sf(xlim = extent[c("xmin", "xmax")], ylim = extent[c("ymin", "ymax")],
           expand = FALSE, crs = 3857, datum = NA)
}
pe_credit <- paste0(
  "Streets © OpenStreetMap contributors (ODbL), Geofabrik extract of 2026-11-01.\n",
  "Boundaries and population: IBGE, Censo 2022. Schools: INEP. Health units: CNES, Ministério da Saúde.")
legend_top <- theme(legend.position = "top", legend.justification = "left",
                    legend.margin = margin(0, 0, 0, 0), legend.key.size = unit(10, "pt"))

figures <- list()

# ------------------------------------------------------------------ isochrones by mode
iso_figure <- function(key, file, width, height, title, desc) {
  d <- get_city(key)
  modes <- c(WALK = "Walking", BICYCLE = "Cycling", CAR = "Driving (free flow)")
  cutoffs <- c(10, 20, 30)
  polys <- do.call(rbind, lapply(names(modes), function(m) {
    iso <- isochrone(d$network, origin_of(d), mode = m, cutoffs = cutoffs, zoom = 11,
                     n_threads = threads)
    iso$mode <- modes[[m]]
    iso[, c("mode", "isochrone")]
  }))
  polys <- st_transform(polys, 3857)
  polys$mode <- factor(polys$mode, levels = modes)
  polys$band <- factor(paste(polys$isochrone, "min"), levels = paste(cutoffs, "min"))
  polys <- polys[order(polys$mode, -polys$isochrone), ]
  ext <- iso_extent(d)
  polys <- suppressWarnings(st_intersection(st_make_valid(polys), st_as_sfc(st_bbox(st_buffer(st_as_sfc(ext), 200)))))
  p <- ggplot() +
    pe_map(d, ext) +
    geom_sf(data = polys, aes(fill = band), colour = NA, alpha = 0.82) +
    geom_sf(data = origin_3857_of(d), shape = 21, size = 2.4, stroke = 1.1,
            fill = light("ink"), colour = light("panel")) +
    facet_wrap(~mode, nrow = 1) +
    scale_fill_manual(values = ramp_time$light[c(1, 3, 4)], name = NULL) +
    pe_coords(ext) +
    labs(title = title$main, subtitle = title$sub, caption = pe_credit) +
    theme_peabiru(map = TRUE) + legend_top +
    theme(plot.caption = element_text(lineheight = 1.1))
  save_svg(p, file, width = width, height = height, title = title$alt, desc = desc)
}

figures$pe_iso_recife <- function() {
  iso_figure("recife", "isochrones-recife", width = 8, height = 5.3,
             title = list(
               main = "Isochrones from Marco Zero, Recife",
               sub = "Areas reached within 10, 20 and 30 minutes; the dashed line is the municipal boundary",
               alt = "Isochrones by mode from Marco Zero, Recife"),
             desc = paste(
               "Three maps of Recife, one per mode, each shading the areas reached from Marco Zero,",
               "in the old centre on the coast, within 10, 20 and 30 minutes. Walking reaches the",
               "islands of the centre and the neighbourhoods just across the rivers; cycling reaches",
               "Boa Viagem to the south and Olinda to the north; driving, with free-flowing traffic,",
               "reaches almost all of the municipality and beyond it within 30 minutes."))
}

figures$pe_iso_caruaru <- function() {
  iso_figure("caruaru", "isochrones-caruaru", width = 8, height = 4.2,
             title = list(
               main = "Isochrones from the Feira de Caruaru",
               sub = "Areas reached within 10, 20 and 30 minutes; urban area of Caruaru. Built without an elevation raster:\nwalking and cycling times are optimistic on its hills",
               alt = "Isochrones by mode from the Feira de Caruaru"),
             desc = paste(
               "Three maps of the urban area of Caruaru, one per mode, each shading the areas reached",
               "from the Feira de Caruaru, in the centre, within 10, 20 and 30 minutes. Walking reaches",
               "the centre and the nearby neighbourhoods; cycling most of the urban area; driving,",
               "with free-flowing traffic, all of it and the roads out of the city."))
}

# ------------------------------------------------------------------ nearest school and health unit
# The travel time from every cell to the nearest school and the nearest
# primary health care unit, by mode (cells with no unit within the limit: NA).
nearest_cache <- list()
nearest <- function(key, mode, limit) {
  id <- paste(key, mode, limit)
  if (!is.null(nearest_cache[[id]])) return(nearest_cache[[id]])
  d <- get_city(key)
  out <- d$points
  for (what in c("schools", "health")) {
    m <- travel_time_matrix(d$network, d$points, d[[what]], mode = mode,
                            max_trip_duration = limit, n_threads = threads)
    t <- tapply(m$travel_time_p50, m$from_id, min)
    out[[what]] <- unname(t[as.character(out$id)])
  }
  nearest_cache[[id]] <<- out
  out
}

nearest_figure <- function(key, file, width, height, title, desc) {
  d <- get_city(key)
  t <- nearest(key, "WALK", 60)
  cells <- merge(d$cells[, "id"], t, by = "id")
  long <- rbind(
    transform(cells, value = schools, panel = "Nearest public school"),
    transform(cells, value = health, panel = "Nearest primary health care unit")
  )
  long <- long[!is.na(long$value) & long$value <= 30, ]   # blank: over 30 minutes
  ext <- extent_of(d)
  long <- long[lengths(st_intersects(long, st_as_sfc(ext))) > 0, ]
  long$panel <- factor(long$panel, levels = c("Nearest public school",
                                              "Nearest primary health care unit"))
  labels <- c("under 5 min", "5–10", "10–15", "15–20", "20–30")
  p <- ggplot() +
    pe_map(d, ext) +
    hex_layer(long, c(0, 5, 10, 15, 20, 31), labels, ramp_time$light) +
    guides(fill = guide_legend(nrow = 1)) +
    facet_wrap(~panel, nrow = 1) +
    pe_coords(ext) +
    labs(title = title$main, subtitle = title$sub, caption = pe_credit) +
    theme_peabiru(map = TRUE) + legend_top
  save_svg(p, file, width = width, height = height, title = title$alt, desc = desc)
}

figures$pe_nearest_recife <- function() {
  d <- get_city("recife")
  nearest_figure("recife", "nearest-recife", width = 8, height = 6.6,
                 title = list(
                   main = "Walking time to the nearest school and health unit, Recife",
                   sub = sprintf("%s hexagonal cells 500 m apart with their population (Censo 2022); %s public schools\n(INEP) and %s primary health care units (CNES) in Recife and up to 10 km around it.\nBlank: no residents, or more than 30 minutes",
                                 format(nrow(d$cells), big.mark = ","), format(nrow(d$schools), big.mark = ","),
                                 nrow(d$health)),
                   alt = "Walking time to the nearest public school and primary health care unit, Recife"),
                 desc = paste(
                   "Two maps of Recife covered by hexagons coloured by the walking time from each cell",
                   "to the nearest public school, on the left, and to the nearest primary health care",
                   "unit, on the right. Almost every inhabited cell is within 15 minutes of a public",
                   "school; health units are farther apart, and a scattering of cells, mostly in the",
                   "south and the west, are 15 to 30 minutes from the nearest one."))
}

figures$pe_nearest_caruaru <- function() {
  d <- get_city("caruaru")
  nearest_figure("caruaru", "nearest-caruaru", width = 8, height = 5.1,
                 title = list(
                   main = "Walking time to the nearest school and health unit, Caruaru",
                   sub = sprintf("Urban area; %s public schools (INEP) and %s primary health care units (CNES) in Caruaru and up\nto 5 km around it. Blank: more than 30 minutes. Elevation is not modelled yet: on the hills, these\ntimes are optimistic",
                                 nrow(d$schools), nrow(d$health)),
                   alt = "Walking time to the nearest public school and primary health care unit, Caruaru"),
                 desc = paste(
                   "Two maps of the urban area of Caruaru covered by hexagons coloured by the walking",
                   "time from each cell to the nearest public school, on the left, and to the nearest",
                   "primary health care unit, on the right. The centre and most neighbourhoods are",
                   "within 10 minutes of both; cells at the edges of the city, and the rural ones",
                   "around it, are 20 minutes or more from the nearest health unit."))
}

# ------------------------------------------------------------------ share of the population, by mode
share_table <- function() {
  modes <- c(WALK = "Walking", BICYCLE = "Cycling", CAR = "Driving (free flow)")
  do.call(rbind, lapply(names(pe_cities), function(key) {
    do.call(rbind, lapply(names(modes), function(m) {
      t <- nearest(key, m, 60)
      pop <- sum(t$population)
      rbind(
        data.frame(city = pe_cities[[key]]$name, mode = modes[[m]], what = "A public school",
                   within15 = sum(t$population[!is.na(t$schools) & t$schools <= 15]) / pop,
                   within30 = sum(t$population[!is.na(t$schools) & t$schools <= 30]) / pop),
        data.frame(city = pe_cities[[key]]$name, mode = modes[[m]], what = "A primary health care unit",
                   within15 = sum(t$population[!is.na(t$health) & t$health <= 15]) / pop,
                   within30 = sum(t$population[!is.na(t$health) & t$health <= 30]) / pop))
    }))
  }))
}

figures$pe_share <- function() {
  s <- share_table()
  print(transform(s, within15 = round(100 * within15, 1), within30 = round(100 * within30, 1)))
  s$row <- factor(paste(s$mode, s$city, sep = "\n"),
                  levels = rev(as.vector(t(outer(c("Walking", "Cycling", "Driving (free flow)"),
                                                 c("Recife", "Caruaru"), paste, sep = "\n")))))
  s$what <- factor(s$what, levels = c("A public school", "A primary health care unit"))
  s$label <- sprintf("%d%% · %d%%", round(100 * s$within15), round(100 * s$within30))
  cols15 <- c(Recife = light("route"), Caruaru = light("forest"))
  cols30 <- c(Recife = "#E8A57B", Caruaru = light("forest2"))
  p <- ggplot(s, aes(y = row)) +
    geom_col(aes(x = within30, fill = paste(city, "30")), width = 0.68) +
    geom_col(aes(x = within15, fill = paste(city, "15")), width = 0.68) +
    geom_text(aes(x = 1.02, label = label), hjust = 0, size = 3, colour = light("ink"),
              family = "Helvetica Neue") +
    scale_fill_manual(values = c("Recife 15" = cols15[["Recife"]], "Recife 30" = cols30[["Recife"]],
                                 "Caruaru 15" = cols15[["Caruaru"]], "Caruaru 30" = cols30[["Caruaru"]]),
                      breaks = c("Recife 15", "Recife 30", "Caruaru 15", "Caruaru 30"),
                      labels = c("Recife, 15 min", "Recife, 30 min", "Caruaru, 15 min", "Caruaru, 30 min"),
                      name = NULL) +
    scale_x_continuous(labels = function(x) paste0(round(100 * x), "%"), breaks = c(0, 0.5, 1),
                       limits = c(0, 1.42), expand = expansion(mult = c(0, 0))) +
    facet_wrap(~what, nrow = 1) +
    labs(title = "Share of the population within 15 and 30 minutes of the nearest unit",
         subtitle = "Recife and Caruaru, whole municipalities, weighted by the population of each cell (Censo 2022).\nInner bar: within 15 minutes; its extension: within 30. Driving is free flow; walking and cycling ignore\nthe hills of Caruaru",
         x = NULL, y = NULL, caption = pe_credit) +
    theme_peabiru(base_size = 10) +
    theme(panel.grid.major.y = element_blank(), legend.position = "top",
          legend.justification = "left", legend.margin = margin(0, 0, 0, 0),
          legend.key.size = unit(9, "pt"), axis.text.y = element_text(size = 8, hjust = 0,
                                                                       lineheight = 0.95),
          strip.text = element_text(size = 10))
  save_svg(p, "access-recife-caruaru", width = 8, height = 5.2,
           title = "Share of the population of Recife and Caruaru within 15 and 30 minutes of a school and a health unit",
           desc = paste(
             "Horizontal bars, one pair per mode and city, of the share of the population within 15",
             "and within 30 minutes of the nearest public school, on the left, and of the nearest",
             "primary health care unit, on the right, walking, cycling and driving. In Recife nearly",
             "everyone is within 15 minutes' walk of a public school and three in four of a health",
             "unit; in Caruaru, whose municipality has a large rural area, fewer are, and the gap is",
             "largest for health units on foot. By car almost everyone is within 15 minutes of both."))
}

# ------------------------------------------------------------------ the browser demo
figures$pe_demo_data <- function() {
  for (k in names(pe_cities)) {
    prepare_city(k, cache)
    demo_extract(k, file.path(root_dir, "site", "demo", "data", "pe"), cache)
  }
}

# ------------------------------------------------------------------ run
wanted <- commandArgs(TRUE)
if (length(wanted) == 0) wanted <- names(figures)
unknown <- setdiff(wanted, names(figures))
if (length(unknown)) stop("no figure named ", paste(unknown, collapse = ", "),
                          " (have: ", paste(names(figures), collapse = ", "), ")")
for (f in wanted) figures[[f]]()
