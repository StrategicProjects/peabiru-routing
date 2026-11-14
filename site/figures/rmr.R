# The figures of the Recife metropolitan region example, the first block of
# the gallery: the 14 municipalities as one network, streets from
# OpenStreetMap and the buses, metro and VLT of the GTFS feed of Grande
# Recife Consórcio de Transporte Metropolitano. Computed with the peabiru R
# package on the data prepared by site/figures/rmr-data.R (which reuses the
# streets, the grid of the Censo 2022, the schools and the health units of
# site/figures/pernambuco-data.R).
#
#   Rscript site/figures/rmr.R                 # all of them
#   Rscript site/figures/rmr.R rmr_iso_modes   # only the ones named
#
# or `bash site/figures/make.sh --rmr`. The results of the routing are kept
# in <cache>/results/ (PEABIRU_RMR_CACHE) with a log of the time each step
# took, and reused; delete that directory to compute them again. Every
# number of the captions is printed.
#
# No figure uses fares: the feed has a price per route and no rule for the
# integration at the terminals, where a transfer is free.

root_dir <- local({
  f <- sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE))
  normalizePath(file.path(dirname(f), "..", ".."))
})
# the helpers of the Recife and Caruaru figures (maps, legends) and their data
source(file.path(root_dir, "site", "figures", "pernambuco.R"))
source(file.path(root_dir, "site", "figures", "rmr-data.R"))
suppressPackageStartupMessages(library(patchwork))

figures <- list()   # (pernambuco.R filled it with its own)
threads <- 4
rmr <- prepare_rmr()
results <- file.path(rmr$cache, "results")

# The analysis: Tuesday 10 November 2026, departures every minute from 07:00
# to 07:59, the median travel time over that hour. The feed has timetables
# only, so nothing is drawn at random.
departure <- as.POSIXct("2026-11-10 07:00:00", tz = "America/Recife")
window <- 60
when <- "Tuesday 10 November 2026"
# Praça do Derby (OpenStreetMap), where the bus corridors of the west and the
# north meet on their way to the centre
origin <- data.frame(id = "Praça do Derby", lon = -34.8990, lat = -8.0570)
transit <- c("WALK", "TRANSIT")

rss_mib <- function() {
  as.numeric(system2("ps", c("-o", "rss=", "-p", Sys.getpid()), stdout = TRUE)) / 1024
}
log_line <- function(...) {
  line <- sprintf(...)
  message(line)
  cat(format(Sys.time(), "%Y-%m-%d %H:%M  "), line, "\n", file = file.path(results, "log.txt"),
      append = TRUE, sep = "")
}
timed <- function(what, expr) {
  t <- system.time(out <- expr)
  log_line("%s: %.1f s, R process RSS now %.0f MiB", what, t[["elapsed"]], rss_mib())
  out
}

# ------------------------------------------------------------------ data
net_env <- new.env()
get_net <- function() {
  if (is.null(net_env$net)) {
    net_env$net <- timed("network built", build_network(rmr$network, n_threads = threads))
  }
  net_env$net
}
cells <- read.csv(file.path(rmr$streets, "cells.csv"), colClasses = c(id = "character"))
cells <- cells[cells$population > 0, ]
people <- sum(cells$population)
schools <- read.csv(file.path(rmr$streets, "schools.csv"), colClasses = c(id = "character"))
health <- read.csv(file.path(rmr$streets, "health.csv"), colClasses = c(id = "character"))
cell_polygons <- function() {
  g <- st_read(file.path(rmr$streets, "cells.gpkg"), quiet = TRUE)[, "id"]
  g$id <- as.character(g$id)
  st_transform(g[g$id %in% cells$id, ], 3857)
}
munis <- st_transform(st_read(file.path(rmr$streets, "boundary.geojson"), quiet = TRUE), 3857)
muni_names <- c("2611606" = "Recife", "2609600" = "Olinda", "2607901" = "Jaboatão dos Guararapes",
                "2610707" = "Paulista", "2603454" = "Camaragibe", "2613701" = "São Lourenço da Mata",
                "2602902" = "Cabo de Santo Agostinho", "2607208" = "Ipojuca", "2609402" = "Moreno",
                "2606804" = "Igarassu", "2600054" = "Abreu e Lima", "2607752" = "Itapissuma",
                "2607604" = "Ilha de Itamaracá", "2601052" = "Araçoiaba")

# The integrated terminals: the stations of the feed named "Ti ..." (26).
terminals <- local({
  s <- feed_table(rmr$feed, "stops")
  s <- s[s$location_type == "1" & grepl("^Ti ", s$stop_name), ]
  name <- sub(" - [0-9]+$", "", s$stop_name)
  name <- sub("^Ti ", "TI ", name)
  name <- sub("Tip$", "TIP", sub("Cdu$", "CDU", sub("Pe-15", "PE-15", name)))
  name <- gsub(" E ", " e ", gsub(" Da ", " da ", name))
  data.frame(id = name, lon = as.numeric(s$stop_lon), lat = as.numeric(s$stop_lat))
})
# The metro and VLT lines, and their stops, for the maps.
rail_env <- new.env()
get_rail <- function() {
  if (is.null(rail_env$lines)) {
    tn <- transit_network_to_sf(get_net())
    r <- tn$routes[tn$routes$mode %in% c("SUBWAY", "TRAM"), ]
    r <- st_simplify(st_transform(r, 3857), dTolerance = 15)
    r$kind <- ifelse(r$mode == "SUBWAY", "Metro", "VLT")
    rail_env$lines <- r[!duplicated(r$short_name), c("kind", "short_name")]   # one direction of each
  }
  rail_env$lines
}

# ------------------------------------------------------------------ results, cached
cached <- function(file, compute, ...) {
  f <- file.path(results, file)
  if (!file.exists(f)) write.csv(compute(), f, row.names = FALSE)
  read.csv(f, ...)
}

# Minutes from the origin to every inhabited cell, by mode (NA: more than
# two hours). "bus": public transport without the metro and the VLT.
from_origin <- function() {
  cached("from-origin.csv", function() {
    modes <- list(walk = "WALK", bicycle = "BICYCLE", car = "CAR", transit = transit,
                  bus = c("WALK", "BUS"))
    out <- cells[, c("id", "population")]
    for (m in names(modes)) {
      t <- timed(sprintf("from the origin to %d cells, %s", nrow(cells), m),
                 travel_time_matrix(get_net(), origin, cells, mode = modes[[m]],
                                    departure_datetime = departure, time_window = window,
                                    max_trip_duration = 120, n_threads = threads))
      out[[m]] <- t$travel_time_p50[match(out$id, t$to_id)]
    }
    out
  }, colClasses = c(id = "character"))
}

# Cumulative access from every inhabited cell: public schools, primary health
# care units and residents of the region within 30, 45 and 60 minutes, by
# mode. One row per cell, columns schools_30, ..., population_60.
access_modes <- list(walk = "WALK", transit = transit, bus = c("WALK", "BUS"))
access_of <- function(m) {
  cached(sprintf("access-%s.csv", m), function() {
    dest <- rbind(
      data.frame(id = paste0("s", schools$id), lon = schools$lon, lat = schools$lat,
                 schools = 1, health = 0, population = 0),
      data.frame(id = paste0("h", health$id), lon = health$lon, lat = health$lat,
                 schools = 0, health = 1, population = 0),
      data.frame(id = paste0("c", cells$id), lon = cells$lon, lat = cells$lat,
                 schools = 0, health = 0, population = cells$population))
    a <- timed(sprintf("accessibility, %s, %d origins x %d destinations", m, nrow(cells), nrow(dest)),
               accessibility(get_net(), cells, dest,
                             opportunities_colnames = c("schools", "health", "population"),
                             mode = access_modes[[m]], departure_datetime = departure,
                             time_window = window, percentiles = 50, decay_function = "step",
                             cutoffs = c(30, 45, 60), max_trip_duration = 60,
                             n_threads = threads, progress = FALSE))
    a$what <- paste(a$opportunity, a$cutoff, sep = "_")
    w <- reshape(a[, c("id", "what", "accessibility")], idvar = "id", timevar = "what",
                 direction = "wide")
    names(w) <- sub("^accessibility\\.", "", names(w))
    out <- merge(cells[, c("id", "population")], w, by = "id", all.x = TRUE)
    out[is.na(out)] <- 0
    out
  }, colClasses = c(id = "character"))
}

wmean <- function(x, w) sum(as.numeric(x) * w) / sum(as.numeric(w))
wmedian <- function(x, w) {
  o <- order(x)
  x[o][which(cumsum(as.numeric(w[o])) >= sum(as.numeric(w)) / 2)[1]]
}
pct <- function(x, digits = 0) paste0(formatC(100 * x, format = "f", digits = digits), "%")
num <- function(x) format(round(x), big.mark = ",")

# ------------------------------------------------------------------ maps
ll_box <- function(xmin, xmax, ymin, ymax) {
  st_bbox(st_transform(st_as_sfc(st_bbox(c(xmin = xmin, xmax = xmax, ymin = ymin, ymax = ymax),
                                         crs = st_crs(4326))), 3857))
}
# The conurbation, from Igarassu in the north to Cabo de Santo Agostinho in
# the south: the maps of the grid show it, and the numbers are of the whole
# region.
core_extent <- ll_box(-35.12, -34.80, -8.32, -7.80)

bg_env <- new.env()
rmr_bg <- function() {
  if (is.null(bg_env$bg)) {
    f <- file.path(results, "background.rds")
    if (!file.exists(f)) saveRDS(background(file.path(rmr$network, "rmr.osm.pbf")), f)
    bg_env$bg <- readRDS(f)
  }
  bg_env$bg
}
# Water, the main streets and the municipal boundaries inside a map.
map_layers <- function(extent, min_w = 0.5, tol = 60, min_water = 2e5) {
  bg <- rmr_bg()
  box <- st_as_sfc(st_bbox(st_buffer(st_as_sfc(extent), 200)))
  lines <- bg$lines[bg$lines$w >= min_w, ]
  lines <- st_simplify(suppressWarnings(st_intersection(lines, box)), dTolerance = tol)
  lines <- lines[!st_is_empty(lines), ]
  water <- bg$water[lengths(st_intersects(bg$water, box)) > 0]
  water <- water[as.numeric(st_area(water)) > min_water]
  water <- suppressWarnings(st_intersection(st_simplify(water, dTolerance = tol), box))
  edges <- st_cast(st_geometry(munis), "MULTILINESTRING")
  edges <- st_simplify(suppressWarnings(st_intersection(edges, box)), dTolerance = tol)
  list(geom_sf(data = water, fill = light("water"), colour = NA),
       geom_sf(data = lines, aes(linewidth = w), colour = light("street"), show.legend = FALSE),
       scale_linewidth_identity(),
       geom_sf(data = edges, colour = light("soft"), linewidth = 0.25, linetype = "22"))
}
# Cells of one class dissolved into one polygon and thinned: thousands of
# hexagons drawn one by one would weigh megabytes.
dissolve <- function(polys, class, panel = NULL, tol = 45) {
  parts <- lapply(levels(class), function(l) {
    g <- st_union(st_geometry(polys)[!is.na(class) & class == l])
    if (length(g) == 0 || st_is_empty(g)) return(NULL)
    st_sf(class = factor(l, levels = levels(class)),
          geometry = st_simplify(st_buffer(g, 1), dTolerance = tol))
  })
  out <- do.call(rbind, parts[!vapply(parts, is.null, TRUE)])
  if (!is.null(panel)) out$panel <- panel
  out
}
in_extent <- function(polys, extent) polys[lengths(st_intersects(polys, st_as_sfc(extent))) > 0, ]
origin_3857 <- st_transform(st_as_sf(origin, coords = c("lon", "lat"), crs = 4326), 3857)
origin_mark <- function(size = 2.4) {
  geom_sf(data = origin_3857, shape = 21, size = size, stroke = 1.1, fill = light("ink"),
          colour = light("panel"))
}
small_caption <- theme(plot.caption = element_text(size = 6.5, lineheight = 1.1))

credit_osm <- "Streets © OpenStreetMap contributors (ODbL), Geofabrik extract of 2026-11-01."
credit_gtfs <- "Timetables: Grande Recife Consórcio de Transporte Metropolitano, GTFS v2026.10.26."
credit_ibge <- "Boundaries and population: IBGE, Censo 2022."
rmr_credit <- function(...) paste(c(credit_osm, credit_gtfs, ...), collapse = " ")
wrap_credit <- function(x, width = 150) paste(strwrap(x, width), collapse = "\n")
# subtitles are written as one sentence and wrapped to the width of the figure
wrap <- function(x, width = 96) paste(strwrap(x, width), collapse = "\n")

# ------------------------------------------------------------------ isochrones by mode
iso_cutoffs <- c(15, 30, 45)
iso_modes <- list(walk = list(mode = "WALK", label = "Walking"),
                  bicycle = list(mode = "BICYCLE", label = "Cycling"),
                  car = list(mode = "CAR", label = "Driving (free flow)"),
                  transit = list(mode = transit, label = "Walking + public transport"))
iso_polys <- function() {
  f <- file.path(results, "isochrones.gpkg")
  if (!file.exists(f)) {
    polys <- do.call(rbind, lapply(names(iso_modes), function(m) {
      iso <- timed(sprintf("isochrone, %s", m),
                   suppressWarnings(isochrone(get_net(), origin, mode = iso_modes[[m]]$mode,
                                              cutoffs = iso_cutoffs, zoom = 11,
                                              departure_datetime = departure, time_window = window,
                                              n_threads = threads)))
      iso$mode <- m
      iso[, c("mode", "isochrone")]
    }))
    st_write(polys, f, quiet = TRUE)
  }
  st_read(f, quiet = TRUE)
}
# The isochrones are traced on a grid of points, and the points just off the
# coast are reached from the seafront: the polygons are cut at the coast (the
# municipal boundaries), and their areas are of land.
land <- st_union(st_make_valid(st_transform(munis, utm)))
iso_land <- function() {
  p <- st_make_valid(st_transform(iso_polys(), utm))
  suppressWarnings(st_intersection(p, land))
}
iso_areas <- function() {
  p <- iso_land()
  a <- as.numeric(st_area(p)) / 1e6
  w <- reshape(data.frame(mode = p$mode, minutes = p$isochrone, km2 = round(a, 1)),
               idvar = "mode", timevar = "minutes", direction = "wide")
  w[match(names(iso_modes), w$mode), ]
}

figures$rmr_iso_modes <- function() {
  areas <- iso_areas()
  print(areas, row.names = FALSE)
  polys <- st_transform(iso_land(), 3857)
  labels <- vapply(iso_modes, `[[`, "", "label")
  polys$panel <- factor(labels[polys$mode], levels = labels)
  polys$band <- factor(paste(polys$isochrone, "min"), levels = paste(iso_cutoffs, "min"))
  polys <- polys[order(polys$panel, -polys$isochrone), ]
  xy <- st_coordinates(origin_3857)
  ext <- st_bbox(c(xmin = xy[1] - 13500, xmax = xy[1] + 5500, ymin = xy[2] - 11000,
                   ymax = xy[2] + 11000), crs = st_crs(3857))
  box <- st_as_sfc(st_bbox(st_buffer(st_as_sfc(ext), 200)))
  polys <- suppressWarnings(st_intersection(st_make_valid(polys), box))
  polys <- st_simplify(polys, dTolerance = 25)
  a45 <- setNames(areas$km2.45, areas$mode)
  p <- ggplot() +
    map_layers(ext, min_w = 0.38, tol = 30, min_water = 5e4) +
    geom_sf(data = polys, aes(fill = band), colour = NA, alpha = 0.82) +
    origin_mark() +
    facet_wrap(~panel, nrow = 2) +
    scale_fill_manual(values = ramp_time$light[c(1, 3, 4)], name = NULL) +
    pe_coords(ext) +
    labs(title = "Isochrones from Praça do Derby, Recife, by mode",
         subtitle = wrap(sprintf(paste(
           "Land reached within 15, 30 and 45 minutes. Public transport: buses, metro and VLT, the median of",
           "the departures 07:00–07:59 on %s; within 45 minutes it reaches %.0f km², cycling %.0f,",
           "walking %.0f. Dashed: municipal boundaries and the coast"),
           when, a45[["transit"]], a45[["bicycle"]], a45[["walk"]])),
         caption = wrap_credit(rmr_credit("Boundaries: IBGE."), 118)) +
    theme_peabiru(map = TRUE) + legend_top + small_caption
  save_svg(p, "isochrones-rmr", width = 8, height = 10.2,
           title = "Isochrones by mode from Praça do Derby, Recife metropolitan region",
           desc = sprintf(paste(
             "Four maps of central Recife and its neighbours, one per mode, each shading the areas reached",
             "from Praça do Derby within 15, 30 and 45 minutes. Walking reaches the neighbourhoods around",
             "the square, %.0f square kilometres in 45 minutes; cycling %.0f; driving with free-flowing",
             "traffic fills the map; walking plus public transport, at the morning peak of %s,",
             "reaches %.0f square kilometres, in arms along the bus corridors and the metro lines."),
             a45[["walk"]], a45[["bicycle"]], when, a45[["transit"]]))
}

# ------------------------------------------------------------------ travel time from one point
time_breaks <- c(0, 15, 30, 45, 60, 91)
time_labels <- c("under 15 min", "15–30", "30–45", "45–60", "60–90")

figures$rmr_travel_time <- function() {
  t <- from_origin()
  share <- function(m, minutes) sum(t$population[!is.na(t[[m]]) & t[[m]] <= minutes]) / people
  s <- sapply(c(walk = "walk", transit = "transit"), function(m) sapply(c(`30` = 30, `60` = 60, `90` = 90), share, m = m))
  print(round(100 * s, 1))
  polys <- cell_polygons()
  panels <- c(walk = "Walking", transit = "Walking + public transport")
  layers <- do.call(rbind, lapply(names(panels), function(m) {
    x <- merge(polys, data.frame(id = t$id, value = t[[m]]), by = "id")
    x <- in_extent(x[!is.na(x$value) & x$value <= 90, ], core_extent)
    dissolve(x, cut(x$value, time_breaks, labels = time_labels, right = FALSE), panels[[m]])
  }))
  layers$panel <- factor(layers$panel, levels = panels)
  p <- ggplot() +
    map_layers(core_extent) +
    geom_sf(data = layers, aes(fill = class), colour = NA, alpha = 0.9) +
    origin_mark(2) +
    scale_fill_manual(values = setNames(ramp_time$light, time_labels), drop = FALSE, name = NULL) +
    guides(fill = guide_legend(nrow = 1)) +
    facet_wrap(~panel, nrow = 1) +
    pe_coords(core_extent) +
    labs(title = "Travel time from Praça do Derby to every inhabited cell",
         subtitle = wrap(sprintf(paste(
           "Recife metropolitan region, cells 500 m apart; departures 07:00–07:59 on %s (median).",
           "Within an hour: %s of the region's %s residents on foot, %s with buses, metro and VLT;",
           "within 90 minutes, %s and %s. Blank: over 90 minutes, or no residents"),
           when, pct(s["60", "walk"]), sprintf("%.1f million", people / 1e6),
           pct(s["60", "transit"]), pct(s["90", "walk"]), pct(s["90", "transit"]))),
         caption = wrap_credit(rmr_credit(credit_ibge), 118)) +
    theme_peabiru(map = TRUE) + legend_top + small_caption
  save_svg(p, "travel-time-rmr", width = 8, height = 8.3,
           title = "Travel time from Praça do Derby to a grid over the Recife metropolitan region, walking and by public transport",
           desc = sprintf(paste(
             "Two maps of the conurbation of Recife, from Igarassu to Cabo de Santo Agostinho, with the cells",
             "of a grid coloured by the median travel time from Praça do Derby at the morning peak: on the",
             "left walking, a patch of five kilometres around the square; on the right walking plus public",
             "transport, which within 90 minutes covers most of the built-up area. Within an hour, %s of the",
             "region's residents are reached on foot and %s by public transport."),
             pct(s["60", "walk"]), pct(s["60", "transit"])))
}

# ------------------------------------------------------------------ share of people reached, by mode
figures$rmr_reach <- function() {
  t <- from_origin()
  label <- c(walk = "Walking", bicycle = "Cycling", car = "Driving (free flow)",
             transit = "Walking + public transport")
  minutes <- 0:120
  curves <- do.call(rbind, lapply(names(label), function(m) {
    data.frame(mode = m, minutes = minutes,
               share = vapply(minutes, function(x) sum(t$population[!is.na(t[[m]]) & t[[m]] <= x]), 0) / people)
  }))
  at <- reshape(curves[curves$minutes %in% c(30, 60, 90, 120), ], idvar = "mode", timevar = "minutes",
                direction = "wide")
  print(cbind(mode = at$mode, round(100 * at[, -1], 1)), row.names = FALSE)
  curves$mode <- factor(curves$mode, levels = names(label))
  ends <- curves[curves$minutes == 120, ]
  ends$label <- sprintf("%s  %d%%", label[as.character(ends$mode)], round(100 * ends$share))
  ends <- ends[order(-ends$share), ]
  ends$y <- ends$share
  for (i in seq_len(nrow(ends))[-1]) ends$y[i] <- min(ends$y[i], ends$y[i - 1] - 0.075)
  cols <- vapply(mode_colour[names(label)], light, "")
  share_at <- function(m, x) curves$share[curves$mode == m & curves$minutes == x]
  p <- ggplot(curves, aes(minutes, share, colour = mode)) +
    geom_step(linewidth = 1.1, direction = "hv") +
    geom_text(data = ends, aes(y = y, label = label), hjust = 0, nudge_x = 2.4, size = 3.4,
              family = "Helvetica Neue") +
    scale_colour_manual(values = cols, guide = "none") +
    scale_x_continuous(breaks = seq(0, 120, 30), limits = c(0, 178), expand = expansion(mult = c(0, 0))) +
    scale_y_continuous(labels = function(x) paste0(round(100 * x), "%"), limits = c(0, 1),
                       expand = expansion(mult = c(0, 0.02))) +
    labs(title = "Share of the metropolitan region's population within reach, by mode",
         subtitle = wrap(sprintf(paste(
           "From Praça do Derby, Recife; %s residents (Censo 2022) on %s cells. Public transport: median",
           "of the departures 07:00–07:59 on %s; within an hour it reaches %s, cycling %s"),
           num(people), num(nrow(cells)), when, pct(share_at("transit", 60)), pct(share_at("bicycle", 60))), 90),
         x = "Travel time (minutes)", y = NULL,
         caption = wrap_credit(rmr_credit(credit_ibge), 130)) +
    theme_peabiru() + small_caption +
    theme(panel.grid.major.x = element_blank(),
          axis.title.x = element_text(colour = light("soft"), hjust = 0))
  save_svg(p, "reach-rmr", width = 7.6, height = 4.3,
           title = "Share of the population of the Recife metropolitan region within reach of Praça do Derby, by mode and travel time",
           desc = sprintf(paste(
             "Four step curves of the share of the region's population reached from Praça do Derby within",
             "each number of minutes, from 0 to 120. Driving with free-flowing traffic reaches %s within 30",
             "minutes; walking plus public transport %s within an hour and %s within two; cycling %s and %s;",
             "walking %s and %s."),
             pct(share_at("car", 30)), pct(share_at("transit", 60)), pct(share_at("transit", 120)),
             pct(share_at("bicycle", 60)), pct(share_at("bicycle", 120)),
             pct(share_at("walk", 60)), pct(share_at("walk", 120))))
}

# ------------------------------------------------------------------ access on the grid
access_summary <- function() {
  rows <- do.call(rbind, lapply(names(access_modes), function(m) {
    a <- access_of(m)
    do.call(rbind, lapply(c("schools", "health", "population"), function(what) {
      do.call(rbind, lapply(c(30, 45, 60), function(k) {
        x <- a[[paste(what, k, sep = "_")]]
        data.frame(mode = m, what = what, minutes = k, mean = wmean(x, a$population),
                   median = wmedian(x, a$population), none = wmean(x == 0, a$population))
      }))
    }))
  }))
  rows
}

access_breaks <- c(1, 5, 10, 20, 40, 80, Inf)
access_labels <- c("1–4", "5–9", "10–19", "20–39", "40–79", "80 or more")

figures$rmr_access <- function() {
  s <- access_summary()
  print(transform(s, mean = round(mean, 1), median = round(median, 1), none = round(100 * none, 1)),
        row.names = FALSE)
  write.csv(s, file.path(results, "access-summary.csv"), row.names = FALSE)
  get <- function(m, what, k, col) s[[col]][s$mode == m & s$what == what & s$minutes == k]
  polys <- cell_polygons()
  panels <- c(walk = "Walking", transit = "Walking + public transport")
  inside <- in_extent(polys, core_extent)$id
  cat(sprintf("residents in the cells of the map: %s of the region\n",
              pct(sum(cells$population[cells$id %in% inside]) / people, 1)))
  layers <- do.call(rbind, lapply(names(panels), function(m) {
    a <- access_of(m)
    x <- merge(polys, data.frame(id = a$id, value = a$schools_30), by = "id")
    x <- in_extent(x[x$value >= 1, ], core_extent)
    dissolve(x, cut(x$value, access_breaks, labels = access_labels, right = FALSE), panels[[m]])
  }))
  layers$panel <- factor(layers$panel, levels = panels)
  p <- ggplot() +
    map_layers(core_extent) +
    geom_sf(data = layers, aes(fill = class), colour = NA, alpha = 0.9) +
    scale_fill_manual(values = setNames(ramp_access$light, access_labels), drop = FALSE, name = NULL) +
    guides(fill = guide_legend(nrow = 1)) +
    facet_wrap(~panel, nrow = 1) +
    pe_coords(core_extent) +
    labs(title = "Public schools within 30 minutes, on foot and with public transport",
         subtitle = wrap(sprintf(paste(
           "Recife metropolitan region: %s public schools (INEP) counted from each of %s inhabited cells;",
           "departures 07:00–07:59 on %s (median). The average resident reaches %.0f on foot and %.0f",
           "with buses, metro and VLT; within 45 minutes, %.0f and %.0f. Blank: none, or no residents"),
           num(nrow(schools)), num(nrow(cells)), when, get("walk", "schools", 30, "mean"),
           get("transit", "schools", 30, "mean"), get("walk", "schools", 45, "mean"),
           get("transit", "schools", 45, "mean"))),
         caption = wrap_credit(rmr_credit(credit_ibge, "Schools: INEP, Catálogo de Escolas 2023."), 118)) +
    theme_peabiru(map = TRUE) + legend_top + small_caption
  save_svg(p, "access-rmr", width = 8, height = 8.3,
           title = "Public schools within 30 minutes on foot and by public transport, Recife metropolitan region",
           desc = sprintf(paste(
             "Two maps of the conurbation of Recife with the cells of a grid coloured by the number of public",
             "schools reached within 30 minutes at the morning peak: on the left walking, on the right walking",
             "plus public transport. On foot the average resident of the region reaches %.0f schools; with",
             "buses, metro and VLT %.0f, and the cells along the corridors that lead to the centre of Recife",
             "reach 80 or more."),
             get("walk", "schools", 30, "mean"), get("transit", "schools", 30, "mean")))
}

# ------------------------------------------------------------------ what the rail adds
# People within an hour by public transport, with the metro and the VLT and
# with buses only (the same bus timetables: no line is redrawn).
rail_gain <- function() {
  a <- access_of("transit")
  b <- access_of("bus")
  w <- access_of("walk")
  x <- data.frame(id = a$id, population = a$population, all = a$population_60, bus = b$population_60[match(a$id, b$id)],
                  walk = w$population_60[match(a$id, w$id)])
  x$gain <- x$all - x$bus
  x$ratio <- ifelse(x$bus > 0, x$all / x$bus - 1, ifelse(x$all > 0, Inf, 0))
  x
}
gain_breaks <- c(0.02, 0.05, 0.10, 0.25, 0.50, Inf)
gain_labels <- c("2–5% more", "5–10%", "10–25%", "25–50%", "50% or more")

# The part of the region the rail lines serve, from Camaragibe to Cabo.
rail_extent <- ll_box(-35.065, -34.855, -8.305, -7.985)

figures$rmr_rail <- function() {
  x <- rail_gain()
  stats <- c(all = wmean(x$all, x$population), bus = wmean(x$bus, x$population),
             over10 = wmean(x$ratio >= 0.10, x$population), over25 = wmean(x$ratio >= 0.25, x$population),
             lose = wmean(x$gain < 0, x$population))
  print(round(stats, 4))
  cat(sprintf("mean people within 60 min: %s with rail, %s buses only (%s more)\n", num(stats[["all"]]),
              num(stats[["bus"]]), pct(stats[["all"]] / stats[["bus"]] - 1, 1)))
  polys <- merge(cell_polygons(), x[, c("id", "ratio")], by = "id")
  cat(sprintf("residents with a gain of 2%% or more outside the map: %s\n",
              num(sum(x$population[x$ratio >= gain_breaks[1] & !x$id %in% in_extent(polys, rail_extent)$id]))))
  polys <- in_extent(polys[polys$ratio >= gain_breaks[1], ], rail_extent)
  layer <- dissolve(polys, cut(polys$ratio, gain_breaks, labels = gain_labels, right = FALSE))
  rail <- get_rail()
  stations <- st_transform(st_as_sf(terminals, coords = c("lon", "lat"), crs = 4326), 3857)
  stations <- in_extent(stations, rail_extent)
  map <- ggplot() +
    map_layers(rail_extent, min_w = 0.5, tol = 40, min_water = 1e5) +
    geom_sf(data = layer, aes(fill = class), colour = NA, alpha = 0.9) +
    geom_sf(data = rail, aes(linetype = kind), colour = light("ink"), linewidth = 0.55) +
    geom_sf(data = stations, shape = 21, size = 1.3, stroke = 0.5, fill = light("panel"),
            colour = light("ink")) +
    scale_fill_manual(values = setNames(ramp_access$light[-1], gain_labels), drop = FALSE, name = NULL) +
    scale_linetype_manual(values = c(Metro = "solid", VLT = "31"), name = NULL) +
    guides(fill = guide_legend(nrow = 2, order = 1), linetype = guide_legend(order = 2)) +
    pe_coords(rail_extent) +
    theme_peabiru(base_size = 10, map = TRUE) + legend_top +
    theme(legend.box = "vertical", legend.box.just = "left", legend.spacing.y = unit(1, "pt"),
          plot.margin = margin(0, 0, 0, 0))
  b <- by_municipality()
  b$gain <- b$all / b$bus - 1
  print(transform(b[order(-b$gain), c("name", "bus", "all", "gain")], bus = round(bus), all = round(all),
                  gain = round(100 * gain, 1)), row.names = FALSE)
  b$name <- factor(b$name, levels = b$name[order(b$gain, b$all)])
  b$label <- ifelse(b$gain < 0.0005, "none", paste0("+", ifelse(b$gain < 0.1, pct(b$gain, 1), pct(b$gain))))
  bars <- ggplot(b, aes(y = name, x = gain)) +
    geom_col(fill = light("forest"), width = 0.62) +
    geom_text(aes(label = label), hjust = 0, nudge_x = 0.006, size = 2.8, colour = light("ink"),
              family = "Helvetica Neue") +
    scale_x_continuous(labels = function(v) paste0(round(100 * v), "%"), breaks = c(0, 0.1, 0.2),
                       limits = c(0, max(b$gain) * 1.3), expand = expansion(mult = c(0, 0))) +
    labs(subtitle = "More people within an hour with the rail lines,\nby municipality of residence (average resident)",
         x = NULL, y = NULL) +
    theme_peabiru(base_size = 10) +
    theme(panel.grid.major.y = element_blank(), axis.text.y = element_text(size = 8, colour = light("ink")),
          plot.subtitle = element_text(size = 9, colour = light("ink")), plot.margin = margin(0, 0, 0, 14),
          plot.background = element_blank())
  top <- b[order(-b$gain), ]
  fig <- map + bars + plot_layout(widths = c(1, 0.95)) +
    plot_annotation(
      title = "What the metro and the VLT add",
      subtitle = wrap(sprintf(paste(
        "Residents of the region within an hour by public transport from each cell, with the rail lines",
        "over buses alone (mode = c(\"WALK\", \"BUS\"): the same bus timetables); departures 07:00–07:59 on",
        "%s. The average resident reaches %s people with rail and %s without, %s more;",
        "%s of residents reach at least 10%% more. Circles: integrated terminals"),
        when, num(stats[["all"]]), num(stats[["bus"]]), pct(stats[["all"]] / stats[["bus"]] - 1, 1),
        pct(stats[["over10"]])), 108),
      caption = wrap_credit(rmr_credit(credit_ibge), 130),
      theme = theme_peabiru(base_size = 10) + small_caption)
  save_svg(fig, "rail-rmr", width = 8, height = 7.6,
           title = "What the metro and the VLT add to the people within an hour by public transport, Recife metropolitan region",
           desc = sprintf(paste(
             "On the left, a map of the west and south of the conurbation of Recife with the metro and VLT lines",
             "and the integrated terminals, and the cells of a grid coloured by how many more residents of the",
             "region they reach within an hour by public transport with the rail lines than with buses alone:",
             "the gain follows the lines, to Camaragibe and Jaboatão in the west and to Cajueiro Seco in the",
             "south. On the right, bars of the same gain for the average resident of each municipality: %s in",
             "%s, %s in %s, %s in %s, and none in the municipalities of the north. The average resident of the",
             "region reaches %s people with rail and %s without."),
             pct(top$gain[1]), top$name[1], pct(top$gain[2]), top$name[2], pct(top$gain[3], 1), top$name[3],
             num(stats[["all"]]), num(stats[["bus"]])))
}

# ------------------------------------------------------------------ by municipality
by_municipality <- function() {
  x <- rail_gain()
  m <- read.csv(file.path(rmr$streets, "cells_muni.csv"), colClasses = c(id = "character", code = "character"))
  m <- merge(m, x[, c("id", "all", "bus", "walk")], by = "id")
  rows <- do.call(rbind, lapply(split(m, m$code), function(r) {
    data.frame(code = r$code[1], name = muni_names[[r$code[1]]], residents = sum(r$population),
               walk = wmean(r$walk, r$population), bus = wmean(r$bus, r$population),
               all = wmean(r$all, r$population))
  }))
  rows[order(-rows$all), ]
}

figures$rmr_municipalities <- function() {
  b <- by_municipality()
  print(transform(b, residents = round(residents), walk = round(walk), bus = round(bus), all = round(all),
                  share = round(100 * all / people, 1), rail = round(100 * (all / bus - 1), 1)), row.names = FALSE)
  write.csv(b, file.path(results, "by-municipality.csv"), row.names = FALSE)
  b$name <- factor(b$name, levels = rev(b$name))
  kinds <- c(walk = "Walking", all = "Walking + public transport")
  long <- do.call(rbind, lapply(names(kinds), function(k) {
    data.frame(name = b$name, kind = kinds[[k]], share = b[[k]] / people)
  }))
  long$kind <- factor(long$kind, levels = kinds)
  b$label <- ifelse(b$all / people < 0.01, pct(b$all / people, 1), pct(b$all / people))
  top <- b[1, ]
  last <- b[nrow(b), ]
  p <- ggplot(b, aes(y = name)) +
    geom_segment(aes(x = walk / people, xend = all / people, yend = name), colour = light("street"),
                 linewidth = 2.2, lineend = "round") +
    geom_point(data = long, aes(x = share, colour = kind), size = 2.4) +
    geom_text(aes(x = all / people + 0.012, label = label), hjust = 0, size = 2.8, colour = light("ink"),
              family = "Helvetica Neue") +
    scale_colour_manual(values = setNames(c(light("forest"), light("route")), kinds), name = NULL) +
    scale_x_continuous(labels = function(x) paste0(round(100 * x), "%"),
                       limits = c(0, max(b$all / people) * 1.12), expand = expansion(mult = c(0.01, 0))) +
    labs(title = "People within an hour, by municipality of residence",
         subtitle = wrap(sprintf(paste(
           "Share of the metropolitan region's %s residents reached within 60 minutes from home, average",
           "of each municipality's residents; departures 07:00–07:59 on %s (median). Numbers: with",
           "buses, metro and VLT"), num(people), when), 104),
         x = NULL, y = NULL, caption = wrap_credit(rmr_credit(credit_ibge), 130)) +
    theme_peabiru(base_size = 10) + legend_top + small_caption +
    theme(panel.grid.major.y = element_blank(), axis.text.y = element_text(size = 8.5, colour = light("ink")))
  save_svg(p, "municipalities-rmr", width = 7.6, height = 5.2,
           title = "People within an hour by public transport, by municipality of the Recife metropolitan region",
           desc = sprintf(paste(
             "A dot plot with one row per municipality of the Recife metropolitan region: the share of the",
             "region's residents that the average resident of the municipality reaches within 60 minutes on",
             "foot and with buses, metro and VLT, at the morning peak. %s is first, with",
             "%s of the region within an hour by public transport, and %s last, with %s."),
             top$name, pct(top$all / people), last$name, pct(last$all / people, 1)))
}

# ------------------------------------------------------------------ how much a trip varies
# From each integrated terminal to Praça do Derby, leaving at each minute of
# the hour: the percentiles of the 60 travel times.
terminal_times <- function() {
  cached("terminals.csv", function() {
    m <- timed("terminals to the origin, percentiles",
               travel_time_matrix(get_net(), terminals, origin, mode = transit,
                                  departure_datetime = departure, time_window = window,
                                  percentiles = c(5, 25, 50, 75, 95), max_trip_duration = 180,
                                  n_threads = threads))
    m[, c("from_id", paste0("travel_time_p", c("05", "25", "50", "75", "95")))]
  })
}

figures$rmr_percentiles <- function() {
  m <- terminal_times()
  names(m) <- c("terminal", "p05", "p25", "p50", "p75", "p95")
  m <- m[order(m$p50, m$p95), ]
  m$spread <- m$p95 - m$p05
  print(m, row.names = FALSE)
  m$terminal <- factor(m$terminal, levels = rev(m$terminal))
  widest <- m[which.max(m$spread), ]
  p <- ggplot(m, aes(y = terminal)) +
    geom_segment(aes(x = p05, xend = p95, yend = terminal), colour = light("stop"), linewidth = 1.2,
                 lineend = "round") +
    geom_segment(aes(x = p25, xend = p75, yend = terminal), colour = light("route"), linewidth = 3.2,
                 lineend = "butt") +
    geom_point(aes(x = p50), shape = 21, size = 2.2, stroke = 0.9, fill = light("panel"),
               colour = light("ink")) +
    scale_x_continuous(limits = c(0, NA), breaks = seq(0, 180, 30), expand = expansion(mult = c(0, 0.04))) +
    labs(title = "One trip, many travel times: from each integrated terminal to Praça do Derby",
         subtitle = wrap(sprintf(paste(
           "Door to door by public transport, leaving at each minute from 07:00 to 07:59 on %s.",
           "Thin bar: 5th to 95th percentile of the 60 travel times; thick bar: 25th to 75th; dot: median.",
           "The spread comes from the timetable, the wait for the next departure and the connections it",
           "makes: %d minutes from %s, %d from %s"),
           when, min(m$spread), m$terminal[which.min(m$spread)], max(m$spread), widest$terminal), 104),
         x = "Travel time (minutes)", y = NULL, caption = wrap_credit(rmr_credit(), 130)) +
    theme_peabiru(base_size = 10) + small_caption +
    theme(panel.grid.major.y = element_blank(), axis.text.y = element_text(size = 7.5, colour = light("ink")),
          axis.title.x = element_text(colour = light("soft"), hjust = 0))
  save_svg(p, "percentiles-rmr", width = 7.6, height = 6,
           title = "Percentiles of the travel time by public transport from the integrated terminals of the Recife metropolitan region to Praça do Derby",
           desc = sprintf(paste(
             "For each of the %d integrated terminals of Grande Recife, sorted by median travel time to Praça do",
             "Derby at the morning peak, a range bar from the 5th to the 95th percentile of the travel times over",
             "an hour of departures, a thicker bar from the 25th to the 75th, and a dot at the median. Medians go",
             "from %d minutes (%s) to %d (%s); the 5th and 95th percentiles are %d to %d minutes apart."),
             nrow(m), min(m$p50), m$terminal[which.min(m$p50)], max(m$p50), m$terminal[which.max(m$p50)],
             min(m$spread), max(m$spread)))
}

# ------------------------------------------------------------------ trip by trip
# From the metro terminus of Camaragibe to Praça de Boa Viagem (OpenStreetMap).
trip_from <- terminals[terminals$id == "TI Camaragibe", ]
trip_to <- data.frame(id = "Praça de Boa Viagem", lon = -34.9014, lat = -8.1317)

breakdown <- function() {
  cached("breakdown.csv", function() {
    e <- timed("expanded matrix, TI Camaragibe to Praça de Boa Viagem",
               expanded_travel_time_matrix(get_net(), trip_from, trip_to, mode = transit,
                                           departure_datetime = departure, time_window = 30,
                                           breakdown = TRUE, n_threads = threads))
    as.data.frame(e)
  }, colClasses = c(routes = "character"))
}

figures$rmr_breakdown <- function() {
  e <- breakdown()
  print(e[, c("departure_time", "access_time", "wait_time", "ride_time", "transfer_time", "egress_time",
              "routes", "total_time")], row.names = FALSE)
  parts <- c(access_time = "Walk to the station", wait_time = "Wait", ride_time = "Ride",
             transfer_time = "Walk between stops", egress_time = "Walk from the stop")
  long <- do.call(rbind, lapply(names(parts), function(k)
    data.frame(departure = e$departure_time, part = parts[[k]], minutes = e[[k]])))
  long$part <- factor(long$part, levels = rev(parts))
  hhmm <- function(x) substr(x, 1, 5)
  long$departure <- factor(hhmm(long$departure), levels = rev(sort(unique(hhmm(e$departure_time)))))
  e$departure <- factor(hhmm(e$departure_time), levels = levels(long$departure))
  short <- gsub("METRO_CENTRO1", "Metro Centro", gsub("METRO_SUL", "Metro Sul", e$routes))
  short <- gsub("\\|([0-9])", "|bus \\1", short)
  e$label <- sprintf("%s · %s min", gsub("\\|", " → ", short), format(e$total_time, nsmall = 1))
  cols <- setNames(c(light("forest"), light("stop"), light("route"), light("slate"), light("forest2")),
                   unname(parts))
  # the longest run of departures that arrive together (on the same last ride)
  secs <- as.numeric(as.difftime(e$departure_time, format = "%H:%M:%S", units = "secs"))
  arrival <- round(secs + e$total_time * 60, -1)   # total_time is in tenths of a minute
  runs <- rle(arrival)
  k <- which.max(runs$lengths)
  first <- sum(runs$lengths[seq_len(k - 1)]) + 1
  same <- list(from = hhmm(e$departure_time[first]), to = hhmm(e$departure_time[first + runs$lengths[k] - 1]),
               arrival = format(as.POSIXct(arrival[first], origin = "1970-01-01", tz = "UTC"), "%H:%M"),
               n = runs$lengths[k])
  print(unlist(same))
  p <- ggplot(long, aes(x = minutes, y = departure, fill = part)) +
    geom_col(width = 0.72, colour = light("panel"), linewidth = 0.25) +
    geom_text(data = e, aes(x = total_time, y = departure, label = label), inherit.aes = FALSE,
              hjust = 0, nudge_x = 1.2, size = 2.6, colour = light("soft"), family = "Helvetica Neue") +
    scale_fill_manual(values = cols, breaks = unname(parts), name = NULL) +
    scale_x_continuous(breaks = seq(0, 90, 15), limits = c(0, max(e$total_time) * 1.62),
                       expand = expansion(mult = c(0, 0))) +
    labs(title = "Trip by trip: Camaragibe to Boa Viagem, minute by minute",
         subtitle = wrap(sprintf(paste(
           "From the integrated terminal of Camaragibe to Praça de Boa Viagem, each departure minute from",
           "07:00 to 07:29 on %s, split into its parts, with the lines taken. Totals go from %s",
           "to %s minutes: every departure from %s to %s arrives on the same bus, at %s"),
           when, format(min(e$total_time), nsmall = 1), format(max(e$total_time), nsmall = 1),
           same$from, same$to, same$arrival), 104),
         x = "Minutes", y = "Departure", caption = wrap_credit(rmr_credit(), 130)) +
    theme_peabiru(base_size = 10) + legend_top + small_caption +
    theme(panel.grid.major.y = element_blank(), legend.key.size = unit(9, "pt"),
          axis.title.x = element_text(colour = light("soft"), hjust = 0),
          axis.title.y = element_text(colour = light("soft"), hjust = 1),
          axis.text.y = element_text(size = 7.5))
  save_svg(p, "trip-breakdown-rmr", width = 7.6, height = 6.8,
           title = "Trips by public transport from Camaragibe to Boa Viagem, departure by departure",
           desc = sprintf(paste(
             "Thirty horizontal stacked bars, one per departure minute from 07:00 to 07:29, from the integrated",
             "terminal of Camaragibe to Praça de Boa Viagem in Recife: walk to the station, wait, ride, walk",
             "between stops and walk from the stop, with the metro and bus lines taken and the total time at",
             "the end of each bar. Totals go from %s to %s minutes; the ride barely changes, and the wait",
             "makes the difference: the %d departures from %s to %s all arrive on the same bus, at %s."),
             format(min(e$total_time), nsmall = 1), format(max(e$total_time), nsmall = 1),
             same$n, same$from, same$to, same$arrival))
}

# ------------------------------------------------------------------ the options of one trip
# From Praça do Carmo, at the foot of Olinda's historic centre, to Praça de
# Boa Viagem, leaving at 07:20 (both from OpenStreetMap). Time, transfers and
# walking: no fare (see the top of this file).
options_trip <- list(
  from = data.frame(id = "Praça do Carmo, Olinda", lon = -34.8485, lat = -8.0170),
  to = trip_to, departure = as.POSIXct("2026-11-10 07:20:00", tz = "America/Recife"))

figures$rmr_trip_options <- function() {
  res <- timed("trip_options, Praça do Carmo to Praça de Boa Viagem",
               trip_options(get_net(), options_trip$from, options_trip$to,
                            departure_datetime = options_trip$departure, time_window = 1,
                            criteria = c("TIME", "TRANSFERS", "WALK"), n_threads = threads,
                            progress = FALSE))
  opt <- as.data.frame(res$options)
  print(opt[, c("option", "total_time", "transfers", "walk_time", "arrival_time", "description")])
  write.csv(opt, file.path(results, "trip-options.csv"), row.names = FALSE)
  stopifnot(nrow(opt) == 3)
  legs <- st_transform(res$itineraries, 3857)
  legs <- legs[!st_is_empty(legs), ]
  names_opt <- character(3)
  names_opt[which.min(opt$total_time)] <- "Fastest"
  names_opt[which.min(opt$transfers)] <- "No transfer"
  names_opt[names_opt == ""] <- "Least walking"
  stopifnot(!anyDuplicated(names_opt))
  cols <- setNames(c(light("route"), light("stop"), light("forest"))[match(names_opt, c("Fastest", "Least walking", "No transfer"))],
                   names_opt)
  legs$name <- factor(names_opt[legs$option], levels = names_opt)
  legs$walk <- legs$mode == "WALK"
  legs <- st_simplify(legs, dTolerance = 10)
  ends <- st_transform(st_as_sf(rbind(options_trip$from, options_trip$to), coords = c("lon", "lat"),
                                crs = 4326), 3857)
  ends$label <- c("A", "B")
  extent <- st_bbox(st_buffer(st_as_sfc(st_bbox(legs)), 900))
  xy <- st_coordinates(ends)
  width_of <- c("No transfer" = 2.3, "Least walking" = 1.5, "Fastest" = 0.8)
  ride <- function(n) geom_sf(data = legs[!legs$walk & legs$name == n, ], aes(colour = name),
                              linewidth = width_of[[n]], lineend = "round")
  map <- ggplot() +
    map_layers(extent, min_w = 0.38, tol = 30, min_water = 5e4) +
    # widest first, thinnest last: where options share a street all show
    ride("No transfer") + ride("Least walking") + ride("Fastest") +
    geom_sf(data = legs[legs$walk, ], aes(colour = name), linewidth = 0.7, linetype = "11",
            show.legend = FALSE) +
    geom_sf(data = ends, shape = 21, size = 5, stroke = 1, fill = light("panel"), colour = light("ink")) +
    annotate("text", x = xy[, 1], y = xy[, 2], label = ends$label, size = 2.8, fontface = "bold",
             colour = light("ink"), family = "Helvetica Neue") +
    scale_colour_manual(values = cols, name = NULL, drop = FALSE) +
    pe_coords(extent) +
    theme_peabiru(base_size = 10, map = TRUE) +
    theme(legend.position = "none", plot.margin = margin(0, 0, 0, 0))
  routes <- gsub("WALK > | > WALK", "", opt$description)
  routes <- gsub("BUS ", "", routes)
  tab <- data.frame(
    y = 3:1, name = names_opt,
    line1 = sprintf("%.0f min  ·  arrives %s", opt$total_time, substr(opt$arrival_time, 1, 5)),
    line2 = sprintf("%s  ·  %.0f min on foot",
                    ifelse(opt$transfers == 0, "no transfer", ifelse(opt$transfers == 1, "1 transfer",
                                                                     paste(opt$transfers, "transfers"))),
                    opt$walk_time),
    line3 = paste(ifelse(opt$transfers == 0, "bus", "buses"), gsub(" > ", ", then ", routes)))
  side <- ggplot(tab) +
    geom_segment(aes(x = 0, xend = 0.09, y = y + 0.24, yend = y + 0.24, colour = name), linewidth = 1.6,
                 lineend = "round", show.legend = FALSE) +
    geom_text(aes(0.13, y + 0.24, label = name), hjust = 0, size = 3.3, fontface = "bold",
              colour = light("ink"), family = "Helvetica Neue") +
    geom_text(aes(0, y, label = line1), hjust = 0, size = 3.4, colour = light("ink"), family = "Helvetica Neue") +
    geom_text(aes(0, y - 0.2, label = line2), hjust = 0, size = 2.9, colour = light("soft"), family = "Helvetica Neue") +
    geom_text(aes(0, y - 0.38, label = line3), hjust = 0, size = 2.9, colour = light("soft"), family = "Helvetica Neue") +
    scale_colour_manual(values = cols) +
    scale_x_continuous(limits = c(0, 1), expand = c(0, 0)) +
    scale_y_continuous(limits = c(0.3, 3.6), expand = c(0, 0)) +
    theme_void() + theme(plot.margin = margin(0, 0, 0, 12))
  fig <- map + side + plot_layout(widths = c(0.9, 1)) +
    plot_annotation(
      title = "Three ways from Olinda to Boa Viagem at 07:20",
      subtitle = wrap(sprintf(paste(
        "From Praça do Carmo (A) to Praça de Boa Viagem (B): the trips that no other beats on time, transfers",
        "and walking at once (trip_options), leaving at 07:20 on %s. Solid: on the bus; dotted:",
        "on foot. Fares are not a criterion: the feed does not say which transfers are free"), when), 104),
      caption = wrap_credit(rmr_credit("Boundaries: IBGE."), 130),
      theme = theme_peabiru(base_size = 10) + small_caption)
  o <- function(n) opt[match(n, names_opt), ]
  save_svg(fig, "trip-options-rmr", width = 7.6, height = 6.3,
           title = "The fastest trip, the one with least walking and the one without a transfer from Olinda to Boa Viagem",
           desc = sprintf(paste(
             "A map of Olinda and Recife with three trips by bus from Praça do Carmo, in Olinda, to Praça de",
             "Boa Viagem, in the south of Recife, and beside it their time, transfers and walking. The fastest",
             "takes %.0f minutes with one transfer and %.0f minutes on foot; with %.0f minutes on foot and one",
             "transfer, %.0f minutes; a single bus, without a transfer, takes %.0f minutes."),
             o("Fastest")$total_time, o("Fastest")$walk_time, o("Least walking")$walk_time,
             o("Least walking")$total_time, o("No transfer")$total_time))
}

# ------------------------------------------------------------------ what the timetable assumes
# Scheduled speeds by hour of departure, on the day of the analysis: a
# timetable written for an empty street would overstate access at the peak.
trip_speeds <- function() {
  cached("speeds.csv", function() {
    s <- timed("scheduled speed of every trip of the day", rmr_trip_speeds(rmr$feed, as.Date(departure, tz = "America/Recife")))
    s[, c("trip_id", "route_id", "shape_id", "mode", "hour", "minutes", "shape_km", "straight_km", "stops")]
  }, colClasses = c(route_id = "character", shape_id = "character"))
}
speeds_by_hour <- function() {
  s <- trip_speeds()
  s <- s[s$minutes > 0 & !is.na(s$shape_km), ]
  s$kmh <- s$shape_km / (s$minutes / 60)
  by <- do.call(rbind, lapply(split(s, list(s$mode, s$hour), drop = TRUE), function(r) {
    q <- quantile(r$kmh, c(0.25, 0.5, 0.75))
    data.frame(mode = r$mode[1], hour = r$hour[1], trips = nrow(r), q25 = q[[1]], median = q[[2]], q75 = q[[3]])
  }))
  # the same path through the day: its running time over its own shortest
  bus <- s[s$mode == "Bus", ]
  med <- aggregate(minutes ~ shape_id + hour, data = bus, FUN = median)
  med$ratio <- med$minutes / ave(med$minutes, med$shape_id, FUN = min)
  r <- aggregate(ratio ~ hour, data = med, FUN = median)
  by$ratio <- ifelse(by$mode == "Bus", r$ratio[match(by$hour, r$hour)], NA)
  by[order(by$mode, by$hour), ]
}

figures$rmr_speeds <- function() {
  by <- speeds_by_hour()
  print(transform(by, q25 = round(q25, 1), median = round(median, 1), q75 = round(q75, 1), ratio = round(ratio, 2)),
        row.names = FALSE)
  write.csv(by, file.path(results, "speeds-by-hour.csv"), row.names = FALSE)
  day <- by[by$hour >= 4 & by$hour <= 23 & by$trips >= 3, ]
  bus <- day[day$mode == "Bus", ]
  at <- function(h) bus$median[bus$hour == h]
  ends <- do.call(rbind, lapply(split(day, day$mode), function(r) r[which.max(r$hour), ]))
  ends$label <- c(Bus = "Buses", Metro = "Metro", VLT = "VLT")[ends$mode]
  cols <- c(Bus = light("route"), Metro = light("slate"), VLT = light("forest"))
  p <- ggplot(day, aes(hour, median, colour = mode)) +
    geom_ribbon(data = bus, aes(ymin = q25, ymax = q75), fill = light("street"), colour = NA) +
    geom_line(linewidth = 0.9) + geom_point(size = 1.3) +
    geom_text(data = ends, aes(label = label), hjust = 0, nudge_x = 0.35, size = 3.2, family = "Helvetica Neue") +
    scale_colour_manual(values = cols, guide = "none") +
    scale_x_continuous(breaks = seq(4, 22, 2), labels = function(h) sprintf("%02d:00", h), limits = c(4, 25.2),
                       expand = expansion(mult = c(0.01, 0))) +
    scale_y_continuous(limits = c(0, NA), expand = expansion(mult = c(0, 0.05))) +
    labs(title = "The timetable slows down at the peaks",
         subtitle = wrap(sprintf(paste(
           "Median scheduled speed of the trips that start in each hour, end to end along their path, on",
           "%s (%s trips). Buses: %.1f km/h at 07:00 and %.1f at 17:00, against %.1f at 04:00",
           "and %.1f at 22:00; band: half the bus trips. Scheduled times, not observed ones"),
           when, num(sum(by$trips)), at(7), at(17), at(4), at(22)), 104),
         x = "Hour of departure", y = "km/h", caption = wrap_credit(credit_gtfs, 130)) +
    theme_peabiru(base_size = 10) + small_caption +
    theme(panel.grid.major.x = element_blank(), axis.title.x = element_text(colour = light("soft"), hjust = 0),
          axis.title.y = element_text(colour = light("soft")))
  save_svg(p, "speeds-rmr", width = 7.6, height = 4.2,
           title = "Scheduled speed of buses, metro and VLT by hour of departure, Grande Recife",
           desc = sprintf(paste(
             "Lines of the median scheduled speed of the trips of the feed by hour of departure on a weekday.",
             "Buses run at %.1f kilometres per hour at 04:00, fall to %.1f at 07:00, stay near %.1f through",
             "the day, fall to %.1f at 17:00 and recover to %.1f at 22:00. The metro and the VLT keep the",
             "same speed all day."),
             at(4), at(7), at(12), at(17), at(22)))
}

# ------------------------------------------------------------------ run
if (sys.nframe() == 0L) {
  wanted <- commandArgs(TRUE)
  if (length(wanted) == 0) wanted <- names(figures)
  unknown <- setdiff(wanted, names(figures))
  if (length(unknown)) stop("no figure named ", paste(unknown, collapse = ", "),
                            " (have: ", paste(names(figures), collapse = ", "), ")")
  for (f in wanted) {
    message("== ", f)
    figures[[f]]()
  }
}
