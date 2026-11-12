# The terrain figures of the documentation sites: what an elevation raster
# changes in walking and cycling times, in Pernambuco (the Região
# Metropolitana do Recife as one network, Recife, Olinda, Jaboatão dos
# Guararapes, Caruaru, Garanhuns and Petrolina) and in two cities of hills
# (Salvador and Olinda). Computed with the peabiru R package on the data of
# site/figures/pernambuco-data.R (OpenStreetMap, Censo 2022, INEP, CNES) and
# the Copernicus DEM: GLO-90 for the results, GLO-30 for the comparison of
# tr_dem_sensitivity (on streets lined by buildings the 30 m surface model
# reads their roofs, and the walls become slopes; see the article).
#
#   Rscript site/figures/terrain.R                 # all of them
#   Rscript site/figures/terrain.R tr_pe_shares    # only the ones named
#
# or `bash site/figures/make.sh --terrain`. The first run downloads the
# Nordeste extract and the registers (as pernambuco.R does), the census
# tracts of Bahia, and reads the Copernicus tiles over each city.

root_dir <- local({
  f <- sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE))
  normalizePath(file.path(dirname(f), "..", ".."))
})
# the helpers of the Recife and Caruaru figures (city data, maps, credits)
source(file.path(root_dir, "site", "figures", "pernambuco.R"))

tr_cities <- c("recife", "olinda", "jaboatao", "caruaru", "garanhuns", "petrolina")
hill_cities <- c("salvador", "olinda")
modes2 <- c(WALK = "Walking", BICYCLE = "Cycling")
elev_names <- c(NONE = "Flat (no raster)", TOBLER = "Tobler", MINETTI = "Minetti")

tr_credit <- paste0(
  "Streets © OpenStreetMap contributors (ODbL), Geofabrik extract of 2026-11-01. Elevation: Copernicus DEM\n",
  "GLO-90 © DLR e.V. 2010-2014 and © Airbus Defence and Space GmbH 2014-2018, provided under COPERNICUS by\n",
  "the European Union and ESA. Boundaries and population: IBGE, Censo 2022. Schools: INEP. Health units: CNES.")

# ------------------------------------------------------------------ networks
# network/ (no raster: the networks of pernambuco.R), network-dem90/ (the
# same extract and GLO-90) with each cost function, "TOBLER" or "MINETTI",
# and network-dem/ (GLO-30), "TOBLER30".
nets <- list()
build_log <- list()
net_of <- function(key, elevation = "NONE") {
  id <- paste(key, elevation)
  if (is.null(nets[[id]])) {
    if (elevation == "NONE") {
      t <- system.time(n <- get_city(key)$network)
    } else {
      path <- prepare_dem(key, cache, arcsec = if (grepl("30$", elevation)) 1 else 3)
      t <- system.time(n <- build_network(path, elevation = sub("30$", "", elevation),
                                          n_threads = threads))
    }
    build_log[[id]] <<- t[["elapsed"]]
    nets[[id]] <<- n
  }
  nets[[id]]
}

# The time from every cell to the nearest school and health unit, and which
# one it is (the first by time, then by id), for a network variant.
near_e <- list()
nearest_e <- function(key, elevation, mode, max_lts = 2, limit = 60) {
  id <- paste(key, elevation, mode, max_lts, limit)
  if (!is.null(near_e[[id]])) return(near_e[[id]])
  d <- get_city(key)
  net <- net_of(key, elevation)
  out <- d$points
  for (what in c("schools", "health")) {
    m <- travel_time_matrix(net, d$points, d[[what]], mode = mode, max_lts = max_lts,
                            max_trip_duration = limit, n_threads = threads)
    m <- m[order(m$from_id, m$travel_time_p50, m$to_id), ]
    m <- m[!duplicated(m$from_id), ]
    k <- match(as.character(out$id), m$from_id)
    out[[what]] <- m$travel_time_p50[k]
    out[[paste0(what, "_id")]] <- m$to_id[k]
  }
  near_e[[id]] <<- out
  out
}

share_of <- function(t, what, minutes) {
  sum(t$population[!is.na(t[[what]]) & t[[what]] <= minutes]) / sum(t$population)
}

city_name <- function(key) {
  if (key == "rmr") "Recife Metropolitan Region" else pe_cities[[key]]$name
}

# ------------------------------------------------------------------ Pernambuco: flat and with elevation
pe_shares_table <- function() {
  do.call(rbind, lapply(tr_cities, function(key) {
    do.call(rbind, lapply(names(modes2), function(m) {
      do.call(rbind, lapply(c("NONE", "TOBLER"), function(e) {
        t <- nearest_e(key, e, m)
        data.frame(city = city_name(key), mode = modes2[[m]], elevation = e,
                   what = c("A public school", "A primary health care unit"),
                   within15 = c(share_of(t, "schools", 15), share_of(t, "health", 15)),
                   within30 = c(share_of(t, "schools", 30), share_of(t, "health", 30)))
      }))
    }))
  }))
}

figures$tr_pe_shares <- function() {
  s <- pe_shares_table()
  print(transform(s, within15 = round(100 * within15, 1), within30 = round(100 * within30, 1)))
  w <- reshape(s[, c("city", "mode", "what", "elevation", "within15")], direction = "wide",
               idvar = c("city", "mode", "what"), timevar = "elevation")
  w$drop <- w$within15.NONE - w$within15.TOBLER
  # cities ordered by the walking drop to a health unit, largest at the top
  key_drop <- w[w$mode == "Walking" & w$what == "A primary health care unit", ]
  w$city <- factor(w$city, levels = key_drop$city[order(key_drop$drop)])
  w$mode <- factor(w$mode, levels = modes2)
  w$what <- factor(w$what, levels = c("A public school", "A primary health care unit"))
  w$label <- ifelse(w$drop >= 0.005, sprintf("−%.0f pp", 100 * w$drop), "")
  long <- rbind(data.frame(w[, c("city", "mode", "what")], x = w$within15.NONE, e = "Flat"),
                data.frame(w[, c("city", "mode", "what")], x = w$within15.TOBLER,
                           e = "With elevation (Tobler)"))
  p <- ggplot(w, aes(y = city)) +
    geom_segment(aes(x = within15.TOBLER, xend = within15.NONE, yend = city),
                 colour = light("street"), linewidth = 2.2, lineend = "round") +
    geom_point(data = long, aes(x = x, colour = e), size = 2.3) +
    geom_text(aes(x = pmin(within15.TOBLER, within15.NONE) - 0.02, label = label),
              hjust = 1, size = 2.7, colour = light("soft"), family = "Helvetica Neue") +
    scale_colour_manual(values = c("Flat" = light("forest2"), "With elevation (Tobler)" = light("route")),
                        name = NULL) +
    scale_x_continuous(labels = function(x) paste0(round(100 * x), "%"), limits = c(0, 1),
                       breaks = c(0, 0.25, 0.5, 0.75, 1)) +
    facet_grid(mode ~ what) +
    labs(title = "Residents within 15 minutes of the nearest unit, flat and with elevation",
         subtitle = paste("Six municipalities of Pernambuco, each weighted by the population of its cells (Censo 2022);",
                          "\nnumbers: percentage points lost to the slopes. Copernicus DEM GLO-90; Tobler's hiking function"),
         x = NULL, y = NULL, caption = tr_credit) +
    theme_peabiru(base_size = 10) + legend_top +
    theme(panel.grid.major.y = element_blank(), strip.text.y = element_text(angle = 0),
          plot.caption = element_text(lineheight = 1.1))
  save_svg(p, "terrain-pe-shares", width = 8, height = 5.6,
           title = "Share of residents within 15 minutes of a school and a health unit, flat and with elevation, six municipalities of Pernambuco",
           desc = paste(
             "Dot plots, one row per municipality, of the share of residents within 15 minutes of the",
             "nearest public school and primary health care unit, walking and cycling, on a flat network",
             "and with the slopes of the Copernicus elevation model. Every municipality loses a few points",
             "on foot, Garanhuns the most and Petrolina the least; by bicycle almost nothing changes."))
}

# ------------------------------------------------------------------ the metropolitan region
# The municipality of each unit, from where it is (outside the region: its
# own label).
unit_muni <- function(units, munis) {
  p <- st_as_sf(units, coords = c("lon", "lat"), crs = 4674)
  p <- st_transform(p, st_crs(munis))
  hit <- st_intersects(p, munis)
  vapply(hit, function(i) if (length(i)) munis$code[i[1]] else "outside", "")
}

rmr_names <- c("2611606" = "Recife", "2609600" = "Olinda", "2607901" = "Jaboatão dos Guararapes",
               "2610707" = "Paulista", "2603454" = "Camaragibe", "2613701" = "São Lourenço da Mata",
               "2602902" = "Cabo de Santo Agostinho", "2607208" = "Ipojuca", "2609402" = "Moreno",
               "2606804" = "Igarassu", "2600054" = "Abreu e Lima", "2607752" = "Itapissuma",
               "2607604" = "Ilha de Itamaracá", "2601052" = "Araçoiaba")

# Residents whose nearest unit on foot is in another municipality, by
# municipality of residence: the population of each cell by the municipality
# of its tracts, and the time from the cell to the nearest unit in each
# municipality. "Across" only when a unit elsewhere is strictly nearer (times
# are whole minutes) than every unit of the resident's own municipality, or
# when none of their own is within the hour.
border_table <- function(elevation = "TOBLER", mode = "WALK", limit = 60) {
  d <- get_city("rmr")
  net <- net_of("rmr", elevation)
  munis <- st_read(file.path(cache, "rmr", "boundary.geojson"), quiet = TRUE)
  by_muni <- read.csv(file.path(cache, "rmr", "cells_muni.csv"), colClasses = c(code = "character"))
  by_muni <- by_muni[by_muni$id %in% d$points$id, ]
  do.call(rbind, lapply(c("schools", "health"), function(what) {
    um <- setNames(unit_muni(d[[what]], munis), d[[what]]$id)
    m <- travel_time_matrix(net, d$points, d[[what]], mode = mode, max_trip_duration = limit,
                            n_threads = threads)
    best <- tapply(m$travel_time_p50, list(m$from_id, um[m$to_id]), min)
    rows <- match(as.character(by_muni$id), rownames(best))
    own <- best[cbind(rows, match(by_muni$code, colnames(best)))]
    other <- vapply(seq_along(rows), function(i) {
      if (is.na(rows[i])) return(NA_real_)
      v <- best[rows[i], colnames(best) != by_muni$code[i]]
      if (all(is.na(v))) NA_real_ else min(v, na.rm = TRUE)
    }, 0)
    across <- !is.na(other) & (is.na(own) | other < own)
    tie <- !is.na(other) & !is.na(own) & other == own
    none <- is.na(other) & is.na(own)
    agg <- aggregate(cbind(pop = by_muni$population, across = by_muni$population * across,
                           tie = by_muni$population * tie, none = by_muni$population * none) ~ code,
                     data = by_muni, FUN = sum)
    data.frame(what = what, code = agg$code, name = unname(rmr_names[agg$code]),
               population = agg$pop, across = agg$across, tied = agg$tie, unreached = agg$none)
  }))
}

figures$tr_rmr_border <- function() {
  b <- border_table()
  tot <- aggregate(cbind(population, across, tied, unreached) ~ what, data = b, FUN = sum)
  print(transform(tot, share = round(100 * across / population, 1)))
  print(transform(b, share = round(100 * across / population, 1))[order(b$what, -b$across / b$population), ],
        row.names = FALSE)
  w <- b
  w$share <- w$across / w$population
  w$what <- factor(ifelse(w$what == "schools", "Nearest public school", "Nearest primary health care unit"),
                   levels = c("Nearest public school", "Nearest primary health care unit"))
  ord <- aggregate(share ~ name, data = w, FUN = sum)
  w$name <- factor(w$name, levels = ord$name[order(ord$share)])
  w$label <- sprintf("%.0f%% · %s", 100 * w$share, format(round(w$across, -2), big.mark = ","))
  p <- ggplot(w, aes(y = name, x = share)) +
    geom_col(fill = light("route"), width = 0.66) +
    geom_text(aes(label = label), hjust = -0.08, size = 2.6, colour = light("ink"),
              family = "Helvetica Neue") +
    scale_x_continuous(labels = function(x) paste0(round(100 * x), "%"), limits = c(0, 0.2),
                       breaks = c(0, 0.05, 0.1), expand = expansion(mult = c(0, 0))) +
    facet_wrap(~what, nrow = 1) +
    labs(title = "Residents whose nearest unit, on foot, is in another municipality",
         subtitle = paste0("Região Metropolitana do Recife as one street network (14 municipalities); walking, with slope costs.\n",
                           "Share of each municipality's residents with a unit strictly nearer across the border, and their number"),
         x = NULL, y = NULL, caption = tr_credit) +
    theme_peabiru(base_size = 10) +
    theme(panel.grid.major.y = element_blank(), plot.caption = element_text(lineheight = 1.1),
          axis.text.y = element_text(size = 8))
  save_svg(p, "terrain-rmr-border", width = 8, height = 5.2,
           title = "Residents of the Recife metropolitan region whose nearest school or health unit on foot is in another municipality",
           desc = paste(
             "Horizontal bars, one per municipality of the Região Metropolitana do Recife, of the share of its",
             "residents whose nearest public school, on the left, and nearest primary health care unit, on the",
             "right, by walking time, lies in another municipality."))
}

# The size of the region's network, its build times, and the size of an
# extract stripped as the demo's are (demo_extract(): no buildings, no
# metadata), over the 14 municipalities without the buffer: the browser demo
# takes a city only under 3 MB. Printed; nothing is written to the site.
figures$tr_rmr_size <- function() {
  dir <- file.path(cache, "rmr")
  pbf <- file.path(dir, "network", "rmr.osm.pbf")
  cat(sprintf("extract (with 3 km around): %.1f MB\n", file.size(pbf) / 1e6))
  for (e in c("NONE", "TOBLER")) {
    p <- if (e == "NONE") file.path(dir, "network") else prepare_dem("rmr", cache, arcsec = 3)
    t <- system.time(n <- build_network(p, elevation = e, n_threads = threads))[["elapsed"]]
    cat(sprintf("build, elevation = %s, %d threads: %.1f s\n", e, threads, t))
    print(n)
  }
  tmp <- file.path(tempdir(), "rmr-demo")
  dir.create(tmp, showWarnings = FALSE)
  munis <- st_read(file.path(dir, "boundary.geojson"), quiet = TRUE)
  area <- st_sf(geometry = st_union(st_make_valid(st_set_precision(st_transform(munis, utm), 100))))
  st_write(st_transform(area, 4326), file.path(tmp, "area.geojson"), delete_dsn = TRUE, quiet = TRUE)
  muni <- file.path(tmp, "area.osm.pbf")
  osmium("extract", "--overwrite", "-p", file.path(tmp, "area.geojson"), pbf, "-o", muni)
  osmium("tags-filter", "--overwrite", "-i", muni, "w/building", "-o", file.path(tmp, "nb.osm.pbf"))
  ways <- system2("osmium", c("cat", "-t", "way", "-f", "opl", file.path(tmp, "nb.osm.pbf")), stdout = TRUE)
  osmium("tags-filter", "--overwrite", "-R", muni, "r/type=restriction", "r/park_ride",
         "-o", file.path(tmp, "rel.osm.pbf"))
  rels <- system2("osmium", c("cat", "-t", "relation", "-f", "opl", file.path(tmp, "rel.osm.pbf")), stdout = TRUE)
  writeLines(sub(" .*", "", c(ways, rels)), file.path(tmp, "ids.txt"))
  out <- file.path(tmp, "rmr-demo.osm.pbf")
  system2("osmium", c("getid", "--overwrite", "-r", muni, "-i", file.path(tmp, "ids.txt"),
                      "-f", "pbf,add_metadata=false", "-o", out))
  cat(sprintf("stripped as the demo's, the 14 municipalities: %.2f MB\n", file.size(out) / 1e6))
  # and with highways only (every way without a highway tag dropped too)
  osmium("tags-filter", "--overwrite", muni, "w/highway", "-o", file.path(tmp, "hw.osm.pbf"))
  hw <- system2("osmium", c("cat", "-t", "way", "-f", "opl", file.path(tmp, "hw.osm.pbf")), stdout = TRUE)
  writeLines(sub(" .*", "", c(hw, rels)), file.path(tmp, "ids2.txt"))
  out2 <- file.path(tmp, "rmr-highways.osm.pbf")
  system2("osmium", c("getid", "--overwrite", "-r", muni, "-i", file.path(tmp, "ids2.txt"),
                      "-f", "pbf,add_metadata=false", "-o", out2))
  cat(sprintf("streets only (highway ways and turn restrictions): %.2f MB\n", file.size(out2) / 1e6))
}

# Walking-time penalty of the slopes to the nearest health unit, by cell.
penalty_cells <- function(key, what = "health", mode = "WALK", elevation = "TOBLER") {
  d <- get_city(key)
  f <- nearest_e(key, "NONE", mode)
  e <- nearest_e(key, elevation, mode)
  x <- data.frame(id = f$id, flat = f[[what]], elev = e[[what]])
  x$value <- x$elev - x$flat
  merge(d$cells[, c("id", "population")], x, by = "id")
}
penalty_breaks <- c(-Inf, -0.5, 0.5, 1.5, 3, 6, Inf)
penalty_labels <- c("faster", "±0", "+1", "+2–3", "+4–6", "over +6 min")
penalty_values <- c(light("forest2"), "#F5D3B6", "#E8A57B", "#CF7445", "#A84B25", "#7A3318")

penalty_map <- function(key, extent, file, width, height, title, desc, what = c("schools", "health")) {
  d <- get_city(key)
  long <- do.call(rbind, lapply(what, function(w) {
    x <- penalty_cells(key, w)
    x$panel <- if (w == "schools") "Nearest public school" else "Nearest primary health care unit"
    x
  }))
  long <- long[!is.na(long$value), ]
  long <- long[lengths(st_intersects(long, st_as_sfc(extent))) > 0, ]
  long$panel <- factor(long$panel, levels = c("Nearest public school", "Nearest primary health care unit"))
  p <- ggplot() +
    pe_map(d, extent) +
    hex_layer(long, penalty_breaks, penalty_labels, penalty_values) +
    guides(fill = guide_legend(nrow = 1)) +
    pe_coords(extent) +
    labs(title = title$main, subtitle = title$sub, caption = tr_credit) +
    theme_peabiru(map = TRUE) + legend_top +
    theme(plot.caption = element_text(lineheight = 1.1))
  if (length(what) > 1) p <- p + facet_wrap(~panel, nrow = 1)
  save_svg(p, file, width = width, height = height, title = title$alt, desc = desc)
}

box_ll <- function(xmin, xmax, ymin, ymax) {
  st_bbox(st_transform(st_as_sfc(st_bbox(c(xmin = xmin, xmax = xmax, ymin = ymin, ymax = ymax),
                                         crs = st_crs(4326))), 3857))
}

figures$tr_rmr_penalty <- function() {
  x <- penalty_cells("rmr")
  pop <- sum(x$population[!is.na(x$value)])
  stats <- c(over2 = sum(x$population[!is.na(x$value) & x$value >= 1.5]) / pop,
             over5 = sum(x$population[!is.na(x$value) & x$value >= 4.5]) / pop)
  print(round(100 * stats, 1))
  ext <- box_ll(-35.12, -34.82, -8.20, -7.92)
  penalty_map("rmr", ext, "terrain-rmr-penalty", width = 8, height = 8.6, what = "health",
              title = list(
                main = "Minutes the slopes add on foot, Recife's core",
                sub = sprintf(paste0("Walking time to the nearest primary health care unit with Tobler's slope costs\n",
                                     "(Copernicus DEM GLO-90), minus the time on flat ground. Região Metropolitana do\n",
                                     "Recife as one network; %.0f%% of its residents lose 2 minutes or more, %.1f%% 5 or more"),
                              100 * stats[["over2"]], 100 * stats[["over5"]]),
                alt = "Walking-time penalty of the slopes to the nearest health unit, Recife and its neighbours"),
              desc = paste(
                "A map of Recife, Olinda, Camaragibe and northern Jaboatão covered by hexagons coloured by the",
                "minutes that slopes add to the walk to the nearest primary health care unit. The coastal plain",
                "barely changes; the hills of the north and west of Recife, of Olinda and of Camaragibe add",
                "several minutes."))
}

# ------------------------------------------------------------------ cities of hills: isochrones
iso_cache <- list()
iso_e <- function(key, elevation, mode, cutoffs = c(10, 20, 30)) {
  id <- paste(key, elevation, mode)
  if (is.null(iso_cache[[id]])) {
    d <- get_city(key)
    i <- isochrone(net_of(key, elevation), origin_of(d), mode = mode, cutoffs = cutoffs,
                   zoom = 11, n_threads = threads)
    i$km2 <- as.numeric(st_area(st_transform(i, city_get(d$city, "utm")))) / 1e6
    iso_cache[[id]] <<- i
  }
  iso_cache[[id]]
}

iso_areas <- function(keys = hill_cities, models = c("NONE", "TOBLER", "MINETTI")) {
  do.call(rbind, lapply(keys, function(key) {
    do.call(rbind, lapply(names(modes2), function(m) {
      do.call(rbind, lapply(models, function(e) {
        i <- iso_e(key, e, m)
        data.frame(city = city_name(key), mode = m, elevation = e, minutes = i$isochrone,
                   km2 = round(i$km2, 2))
      }))
    }))
  }))
}

iso_extents <- list(
  salvador = function(d) box_around(d, 3500, 8500, 7000, 5500),
  olinda = function(d) box_around(d, 6500, 2000, 6000, 4500)
)

hill_iso_figure <- function(key, file, width, height, title, desc) {
  d <- get_city(key)
  cutoffs <- c(10, 20, 30)
  ext <- iso_extents[[key]](d)
  clip <- st_as_sfc(st_bbox(st_buffer(st_as_sfc(ext), 200)))
  rows <- list()
  outlines <- list()
  for (m in names(modes2)) {
    a <- iso_e(key, "NONE", m)
    b <- iso_e(key, "TOBLER", m)
    k30 <- function(i) i$km2[i$isochrone == 30]
    lost <- k30(a) - k30(b)
    lab_flat <- sprintf("%s, flat: 30 min = %.1f km²", modes2[[m]], k30(a))
    lab_elev <- sprintf("%s, slopes: %.1f km² (−%.0f%%)", modes2[[m]], k30(b), 100 * lost / k30(a))
    for (x in list(list(i = a, lab = lab_flat), list(i = b, lab = lab_elev))) {
      p <- st_transform(x$i[, "isochrone"], 3857)
      p$panel <- x$lab
      p$mode <- m
      rows[[length(rows) + 1]] <- p
    }
    o <- st_transform(a[a$isochrone == 30, "isochrone"], 3857)
    o$panel <- lab_elev
    outlines[[length(outlines) + 1]] <- o
  }
  polys <- do.call(rbind, rows)
  polys$panel <- factor(polys$panel, levels = unique(polys$panel))
  polys$band <- factor(paste(polys$isochrone, "min"), levels = paste(cutoffs, "min"))
  polys <- polys[order(polys$panel, -polys$isochrone), ]
  polys <- suppressWarnings(st_intersection(st_make_valid(polys), clip))
  outl <- do.call(rbind, outlines)
  outl$panel <- factor(outl$panel, levels = levels(polys$panel))
  outl <- suppressWarnings(st_intersection(st_make_valid(outl), clip))
  p <- ggplot() +
    pe_map(d, ext) +
    geom_sf(data = polys, aes(fill = band), colour = NA, alpha = 0.82) +
    geom_sf(data = outl, fill = NA, colour = light("ink"), linewidth = 0.35, linetype = "22") +
    geom_sf(data = origin_3857_of(d), shape = 21, size = 2.2, stroke = 1.1,
            fill = light("ink"), colour = light("panel")) +
    facet_wrap(~panel, ncol = 2) +
    scale_fill_manual(values = ramp_time$light[c(1, 3, 4)], name = NULL) +
    pe_coords(ext) +
    labs(title = title$main, subtitle = title$sub, caption = tr_credit) +
    theme_peabiru(map = TRUE) + legend_top +
    theme(plot.caption = element_text(lineheight = 1.1), strip.text = element_text(size = 9.5))
  save_svg(p, file, width = width, height = height, title = title$alt, desc = desc)
}

figures$tr_iso_salvador <- function() {
  print(iso_areas("salvador"))
  hill_iso_figure("salvador", "terrain-iso-salvador", width = 8, height = 8.6,
                  title = list(
                    main = "From the Praça Municipal, Salvador: flat and with slopes",
                    sub = "Areas reached in 10, 20 and 30 minutes on foot and by bicycle. Right: with Tobler's slope costs\n(Copernicus DEM GLO-90); dashed: the flat 30-minute isochrone",
                    alt = "Isochrones on foot and by bicycle from the Praça Municipal, Salvador, without and with elevation"),
                  desc = paste(
                    "Four maps of central Salvador. On the left, the areas reached from the Praça Municipal, on the",
                    "escarpment of the upper city, within 10, 20 and 30 minutes walking and cycling on flat ground;",
                    "on the right, the same with the slopes, smaller, with the flat 30-minute outline dashed."))
}

figures$tr_iso_olinda <- function() {
  print(iso_areas("olinda"))
  hill_iso_figure("olinda", "terrain-iso-olinda", width = 8, height = 8.2,
                  title = list(
                    main = "From the Alto da Sé, Olinda: flat and with slopes",
                    sub = "Areas reached in 10, 20 and 30 minutes on foot and by bicycle. Right: with Tobler's slope costs\n(Copernicus DEM GLO-90); dashed: the flat 30-minute isochrone",
                    alt = "Isochrones on foot and by bicycle from the Alto da Sé, Olinda, without and with elevation"),
                  desc = paste(
                    "Four maps of Olinda and the north of Recife. On the left, the areas reached from the Alto da Sé,",
                    "on the hill of the historic centre, within 10, 20 and 30 minutes walking and cycling on flat",
                    "ground; on the right, the same with the slopes, with the flat 30-minute outline dashed."))
}

# ------------------------------------------------------------------ cities of hills: penalty maps
figures$tr_penalty_salvador <- function() {
  x <- penalty_cells("salvador")
  pop <- sum(x$population[!is.na(x$value)])
  s2 <- sum(x$population[!is.na(x$value) & x$value >= 1.5]) / pop
  ext <- box_ll(-38.54, -38.30, -13.02, -12.86)
  penalty_map("salvador", ext, "terrain-penalty-salvador", width = 8, height = 4.6,
              title = list(
                main = "Minutes the slopes add on foot, Salvador",
                sub = sprintf(paste0("Walking time to the nearest unit with Tobler's slope costs, minus the time on flat ground\n",
                                     "(500 m cells, Censo 2022; Copernicus DEM GLO-90). For %.0f%% of the residents the walk to a\n",
                                     "health unit is 2 minutes or more longer"),
                              100 * s2),
                alt = "Walking-time penalty of the slopes to the nearest school and health unit, Salvador"),
              desc = paste(
                "Two maps of Salvador covered by hexagons coloured by the minutes that slopes add to the walk to",
                "the nearest public school, on the left, and primary health care unit, on the right."))
}

figures$tr_penalty_olinda <- function() {
  x <- penalty_cells("olinda")
  pop <- sum(x$population[!is.na(x$value)])
  s2 <- sum(x$population[!is.na(x$value) & x$value >= 1.5]) / pop
  d <- get_city("olinda")
  ext <- st_bbox(st_buffer(st_as_sfc(st_bbox(d$boundary)), 300))
  penalty_map("olinda", ext, "terrain-penalty-olinda", width = 8, height = 5.4,
              title = list(
                main = "Minutes the slopes add on foot, Olinda",
                sub = sprintf(paste0("Walking time to the nearest unit with Tobler's slope costs, minus the time on flat ground\n",
                                     "(250 m cells, Censo 2022; Copernicus DEM GLO-90). For %.0f%% of the residents the walk to a\n",
                                     "health unit is 2 minutes or more longer"),
                              100 * s2),
                alt = "Walking-time penalty of the slopes to the nearest school and health unit, Olinda"),
              desc = paste(
                "Two maps of Olinda covered by hexagons coloured by the minutes that slopes add to the walk to",
                "the nearest public school, on the left, and primary health care unit, on the right."))
}

# ------------------------------------------------------------------ Tobler and Minetti
models_table <- function(keys = c("salvador", "olinda", "garanhuns", "caruaru")) {
  do.call(rbind, lapply(keys, function(key) {
    do.call(rbind, lapply(names(modes2), function(m) {
      do.call(rbind, lapply(names(elev_names), function(e) {
        t <- nearest_e(key, e, m)
        data.frame(city = city_name(key), mode = modes2[[m]], elevation = e,
                   health15 = share_of(t, "health", 15), school15 = share_of(t, "schools", 15),
                   mean_health = weighted.mean(t$health, t$population, na.rm = TRUE))
      }))
    }))
  }))
}

figures$tr_models <- function() {
  s <- models_table()
  print(transform(s, health15 = round(100 * health15, 1), school15 = round(100 * school15, 1),
                  mean_health = round(mean_health, 2)))
  s$mode <- factor(s$mode, levels = modes2)
  s$e <- factor(elev_names[s$elevation], levels = elev_names)
  ord <- s[s$mode == "Walking" & s$elevation == "NONE", ]
  s$city <- factor(s$city, levels = rev(ord$city))
  p <- ggplot(s, aes(x = mean_health, y = city, colour = e, shape = e)) +
    geom_point(size = 2.6, stroke = 1) +
    scale_colour_manual(values = setNames(c(light("forest2"), light("route"), light("slate")), elev_names),
                        name = NULL) +
    scale_shape_manual(values = setNames(c(16, 16, 4), elev_names), name = NULL) +
    facet_wrap(~mode, nrow = 1, scales = "free_x") +
    labs(title = "Tobler against Minetti: average time to the nearest health unit",
         subtitle = paste("Minutes, averaged over the residents (Censo 2022) who reach one within an hour. Minetti's energy cost",
                          "\nweighs climbs more than Tobler's hiking function does: in every city here it is the slower of the two"),
         x = "minutes", y = NULL, caption = tr_credit) +
    theme_peabiru(base_size = 10) + legend_top +
    theme(panel.grid.major.y = element_blank(), plot.caption = element_text(lineheight = 1.1))
  save_svg(p, "terrain-models", width = 8, height = 4.4,
           title = "Average walking and cycling time to the nearest health unit, flat, with Tobler and with Minetti",
           desc = paste(
             "Dot plots, one row per city, of the average time from residents to the nearest primary health",
             "care unit, walking and cycling, on flat ground, with Tobler's hiking function and with Minetti's",
             "energy cost."))
}

# ------------------------------------------------------------------ stress and slope
lts_table <- function(keys = c("salvador", "olinda", "recife")) {
  do.call(rbind, lapply(keys, function(key) {
    do.call(rbind, lapply(1:4, function(l) {
      do.call(rbind, lapply(c("NONE", "TOBLER"), function(e) {
        t <- nearest_e(key, e, "BICYCLE", max_lts = l)
        data.frame(city = city_name(key), max_lts = l, elevation = e,
                   health10 = share_of(t, "health", 10), health15 = share_of(t, "health", 15),
                   school15 = share_of(t, "schools", 15),
                   mean_health = weighted.mean(t$health, t$population, na.rm = TRUE))
      }))
    }))
  }))
}

figures$tr_lts <- function() {
  s <- lts_table()
  print(transform(s, health10 = round(100 * health10, 1), health15 = round(100 * health15, 1),
                  school15 = round(100 * school15, 1), mean_health = round(mean_health, 2)))
  s$e <- factor(c(NONE = "Flat", TOBLER = "With slopes (Tobler)")[s$elevation],
                levels = c("Flat", "With slopes (Tobler)"))
  s$city <- factor(s$city, levels = c("Salvador", "Olinda", "Recife"))
  p <- ggplot(s, aes(x = max_lts, y = mean_health, colour = e)) +
    geom_line(linewidth = 0.8) + geom_point(size = 2) +
    scale_colour_manual(values = c("Flat" = light("forest2"), "With slopes (Tobler)" = light("route")),
                        name = NULL) +
    scale_y_continuous(limits = c(0, NA), expand = expansion(mult = c(0, 0.08))) +
    scale_x_continuous(breaks = 1:4, labels = c("1", "2*", "3", "4"), expand = expansion(add = 0.3)) +
    facet_wrap(~city, nrow = 1) +
    labs(title = "Cycling to the nearest health unit: traffic stress and slope",
         subtitle = paste("Average minutes over the residents, by the highest level of traffic stress ridden (max_lts; on",
                          "\nbusier streets the bicycle is walked; * the default); flat and with Tobler's slope costs"),
         x = "max_lts: 1 = calm streets only, 4 = any street", y = "minutes", caption = tr_credit) +
    theme_peabiru(base_size = 10) + legend_top +
    theme(plot.caption = element_text(lineheight = 1.1))
  save_svg(p, "terrain-lts", width = 8, height = 4.4,
           title = "Average cycling time to the nearest health unit by traffic stress and slope, Salvador, Olinda and Recife",
           desc = paste(
             "Line charts, one per city, of the average cycling time from residents to the nearest primary",
             "health care unit as the highest level of traffic stress ridden goes from 1 to 4, on flat",
             "ground and with slopes: stress costs more than slope."))
}

# ------------------------------------------------------------------ the DEM against the ground
# Printed, not drawn: what the raster holds where the streets are. Streets
# sampled every 10 m, as peabiru samples them, at the nearest pixel (terra).
dem_file <- function(key, arcsec = 3) {
  file.path(prepare_dem(key, cache, arcsec = arcsec),
            paste0(key, "_dem", if (arcsec == 1) "" else 30 * arcsec, ".tif"))
}
street_samples <- function(key, bridges = FALSE, arcsec = 3) {
  d <- get_city(key)
  lines <- suppressWarnings(st_read(d$pbf, "lines", quiet = TRUE))
  lines <- lines[!is.na(lines$highway), ]
  is_bridge <- grepl('"bridge"=>"(yes|viaduct)"', lines$other_tags)
  lines <- lines[if (bridges) is_bridge else !is_bridge, ]
  lines <- st_transform(lines, city_get(d$city, "utm"))
  lines <- lines[as.numeric(st_length(lines)) >= 20, ]
  pts <- st_line_sample(lines, density = 1 / 10, type = "regular")
  n <- lengths(lapply(pts, function(p) if (st_is_empty(p)) NULL else st_coordinates(p)[, 1]))
  xy <- st_coordinates(pts)
  ll <- sf_project(st_crs(lines), st_crs(4326), xy[, 1:2])
  r <- terra::rast(dem_file(key, arcsec))
  z <- terra::extract(r, ll, method = "simple")[, 1]
  data.frame(way = rep(lines$osm_id[n > 0], n[n > 0]), name = rep(lines$name[n > 0], n[n > 0]),
             x = xy[, 1], y = xy[, 2], lon = ll[, 1], lat = ll[, 2], z = z)
}

dem_check <- function(key, arcsec = 3) {
  r <- terra::rast(dem_file(key, arcsec))
  v <- terra::values(r, mat = FALSE)
  s <- street_samples(key, arcsec = arcsec)
  b <- street_samples(key, bridges = TRUE, arcsec = arcsec)
  # a bridge's profile: how far its lowest sample falls below its two ends
  sag <- do.call(rbind, lapply(split(b, b$way), function(w) {
    if (nrow(w) < 3) return(NULL)
    ends <- mean(w$z[c(1, nrow(w))])
    data.frame(way = w$way[1], name = w$name[1], n = nrow(w), sag = ends - min(w$z[-c(1, nrow(w))]))
  }))
  list(
    raster = c(rows = nrow(r), cols = ncol(r), min = min(v), max = max(v),
               voids = sum(is.na(v) | v < -1000 | v > 9000), zero = mean(v == 0), negative = mean(v < 0)),
    streets = c(samples = nrow(s), off_raster = sum(is.na(s$z)), at_or_below_0 = mean(s$z <= 0, na.rm = TRUE),
                quantile(s$z, c(0.5, 0.9, 0.99), na.rm = TRUE)),
    bridges = c(ways = nrow(sag), sag_over_2m = mean(sag$sag > 2), sag_over_5m = mean(sag$sag > 5),
                median_sag = median(sag$sag)),
    worst_bridges = head(sag[order(-sag$sag), ], 5),
    street_samples = s
  )
}

figures$tr_dem_check <- function() {
  for (key in c("rmr", "salvador", "olinda", "caruaru", "garanhuns", "petrolina", "jaboatao", "recife")) {
    x <- dem_check(key)
    cat("\n==", key, "GLO-90\n")
    print(round(x$raster, 4)); print(round(x$streets, 3)); print(round(x$bridges, 3))
    print(x$worst_bridges)
  }
  # Boa Viagem, on the flat coastal plain of Recife (2 to 5 m above the sea
  # on the topographic maps), lined by towers: what each raster reads on its
  # streets
  for (arcsec in c(1, 3)) {
    x <- dem_check("recife", arcsec)
    if (arcsec == 1) { cat("\n== recife GLO-30\n"); print(round(x$bridges, 3)) }
    s <- x$street_samples
    bv <- s[in_boa_viagem(s$lon, s$lat), ]
    cat("\nBoa Viagem street samples, GLO-", 30 * arcsec, ": ", nrow(bv), "\n", sep = "")
    print(round(quantile(bv$z, c(0.1, 0.5, 0.9, 0.99, 1), na.rm = TRUE), 1))
    cat("consecutive samples of a street differing by more than 2 m:",
        round(mean(abs(diff(bv$z))[bv$way[-1] == bv$way[-nrow(bv)]] > 2, na.rm = TRUE), 3), "\n")
  }
}

# ------------------------------------------------------------------ GLO-30 against GLO-90
# Printed, not drawn: how much of what the slopes cost depends on the
# raster. The same surface model at 1 and at 3 arc seconds, in a flat
# district of towers (Boa Viagem, Recife: a control, any penalty there is the
# model's), in whole cities, and in the isochrones of the cities of hills.
# This is why the figures use GLO-90.
in_boa_viagem <- function(lon, lat) lon > -34.925 & lon < -34.885 & lat > -8.145 & lat < -8.09

sensitivity_table <- function(keys = c("recife", "olinda", "salvador", "garanhuns", "petrolina")) {
  rows <- do.call(rbind, lapply(keys, function(key) {
    f <- nearest_e(key, "NONE", "WALK")
    do.call(rbind, lapply(c("TOBLER30", "TOBLER"), function(e) {
      t <- nearest_e(key, e, "WALK")
      d <- t$health - f$health
      ok <- !is.na(d)
      data.frame(area = city_name(key), raster = if (e == "TOBLER30") "GLO-30" else "GLO-90",
                 residents = round(sum(f$population)),
                 flat15 = round(100 * share_of(f, "health", 15), 1),
                 slopes15 = round(100 * share_of(t, "health", 15), 1),
                 mean_added = round(weighted.mean(d[ok], f$population[ok]), 2),
                 add2 = round(100 * sum(f$population[ok & d >= 1.5]) / sum(f$population[ok]), 1))
    }))
  }))
  f <- nearest_e("recife", "NONE", "WALK")
  bv <- in_boa_viagem(f$lon, f$lat)
  control <- do.call(rbind, lapply(c("TOBLER30", "TOBLER"), function(e) {
    t <- nearest_e("recife", e, "WALK")
    d <- (t$health - f$health)
    ok <- bv & !is.na(d)
    data.frame(area = "Boa Viagem (flat)", raster = if (e == "TOBLER30") "GLO-30" else "GLO-90",
               residents = round(sum(f$population[bv])),
               flat15 = round(100 * share_of(f[bv, ], "health", 15), 1),
               slopes15 = round(100 * share_of(t[bv, ], "health", 15), 1),
               mean_added = round(weighted.mean(d[ok], f$population[ok]), 2),
               add2 = round(100 * sum(f$population[ok & d >= 1.5]) / sum(f$population[ok]), 1))
  }))
  rbind(control, rows)
}

figures$tr_dem_sensitivity <- function() {
  print(sensitivity_table(), row.names = FALSE)
  print(iso_areas(hill_cities, c("NONE", "TOBLER30", "TOBLER"))[c(TRUE, FALSE, FALSE), ], row.names = FALSE)
}

# ------------------------------------------------------------------ run
if (sys.nframe() == 0L) {
  terrain_figures <- grep("^tr_", names(figures), value = TRUE)
  wanted <- commandArgs(TRUE)
  if (length(wanted) == 0) wanted <- terrain_figures
  unknown <- setdiff(wanted, terrain_figures)
  if (length(unknown)) stop("no figure named ", paste(unknown, collapse = ", "),
                            " (have: ", paste(terrain_figures, collapse = ", "), ")")
  for (f in wanted) figures[[f]]()
  print(unlist(build_log))
}
