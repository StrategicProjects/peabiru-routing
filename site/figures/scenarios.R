# The "before and after" figures of the documentation sites: scenarios on
# Porto Alegre (r5r's sample data) and a city outside Brazil, Helsinki,
# computed with the peabiru R package on the data prepared by
# site/figures/scenarios-data.R.
#
#   Rscript site/figures/scenarios.R               # all of them
#   Rscript site/figures/scenarios.R scen_fares    # only the ones named
#
# or `bash site/figures/make.sh --scenarios`. The first run downloads about
# 880 MB into the cache of scenarios-data.R and needs osmium.

root_dir <- local({
  f <- sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE))
  normalizePath(file.path(dirname(f), "..", ".."))
})
source(file.path(root_dir, "site", "figures", "common.R"))
source(file.path(root_dir, "site", "figures", "scenarios-data.R"))

threads <- 4
cache <- sc_cache()
memo <- new.env()
once <- function(key, expr) {
  if (!exists(key, memo, inherits = FALSE)) assign(key, expr, memo)
  get(key, memo, inherits = FALSE)
}
legend_top <- theme(legend.position = "top", legend.justification = "left",
                    legend.margin = margin(0, 0, 0, 0), legend.key.size = unit(10, "pt"))
fmt <- function(x, digits = 0) format(round(x, digits), big.mark = ",", nsmall = digits)
pct <- function(x, digits = 1) paste0(format(round(100 * x, digits), nsmall = digits), "%")

# ------------------------------------------------------------------ Porto Alegre
poa <- function() once("poa", {
  dir <- prepare_poa(cache)
  hex <- read.csv(file.path(dir, "poa_hexgrid.csv"))
  hex$jobs[is.na(hex$jobs)] <- 0          # 5 cells have no count
  hex$healthcare[is.na(hex$healthcare)] <- 0
  osm <- suppressWarnings(st_read(file.path(dir, "poa_osm.pbf"), "lines", quiet = TRUE))
  list(dir = dir, hex = hex, cells = hexagons(hex), osm = osm,
       network = build_network(file.path(dir, "network"), n_threads = threads),
       bg = background(file.path(dir, "poa_osm.pbf")))
})
monday_2pm <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")
ways_named <- function(name) {
  o <- poa()$osm
  o[!is.na(o$name) & o$name == name & !is.na(o$highway), ]
}
poa_extent <- function() once("poa_extent", st_bbox(st_buffer(st_as_sfc(st_bbox(poa()$cells)), 300)))
poa_map <- function(ext = poa_extent()) {
  bg <- poa()$bg
  box <- st_as_sfc(st_bbox(st_buffer(st_as_sfc(ext), 200)))
  list(geom_sf(data = bg$water[lengths(st_intersects(bg$water, box)) > 0], fill = light("water"), colour = NA),
       geom_sf(data = bg$lines[lengths(st_intersects(bg$lines, box)) > 0, ], aes(linewidth = w),
               colour = light("street"), show.legend = FALSE),
       scale_linewidth_identity())
}
poa_coords <- function(ext = poa_extent()) {
  coord_sf(xlim = ext[c("xmin", "xmax")], ylim = ext[c("ymin", "ymax")],
           expand = FALSE, crs = 3857, datum = NA)
}
poa_credit <- paste0("Map data © OpenStreetMap contributors (ODbL). Grid (population, jobs, schools, health), GTFS\n",
                     "(EPTC, Trensurb) and fares: r5r 2.5.0's sample data of Porto Alegre.")
weighted <- function(value, id) {
  h <- poa()$hex
  stats::weighted.mean(value, h$population[match(id, h$id)])
}
share_pop <- function(id) {
  h <- poa()$hex
  sum(h$population[h$id %in% id]) / sum(h$population)
}

# -- a new cycle path: Avenida Protásio Alves at level of traffic stress 1
cycle_path <- function() once("cycle", {
  d <- poa()
  ways <- ways_named("Avenida Protásio Alves")
  lts <- data.frame(osm_id = as.numeric(ways$osm_id), lts = 1L)
  acc <- function(...) accessibility(d$network, d$hex, d$hex, opportunities_colnames = "jobs",
                                     mode = "BICYCLE", max_lts = 2, cutoffs = 20,
                                     n_threads = threads, ...)
  before <- acc()
  after <- acc(new_lts = lts)
  m <- merge(before[, c("id", "accessibility")], after[, c("id", "accessibility")],
             by = "id", suffixes = c("_before", "_after"))
  m$gain <- m$accessibility_after - m$accessibility_before
  od <- data.frame(id = c("Jardim Carvalho", "Santa Casa"), lon = c(-51.1405, -51.2224),
                   lat = c(-30.0415, -30.03043))
  trip <- function(...) detailed_itineraries(d$network, od[1, ], od[2, ], mode = "BICYCLE",
                                             max_lts = 2, departure_datetime = monday_2pm, ...)
  list(ways = ways, lts = lts, access = m, od = od, trip_before = trip(), trip_after = trip(new_lts = lts))
})

figures <- list()

figures$scen_cycle <- function() {
  r <- cycle_path()
  d <- poa()
  cells <- merge(d$cells[, "id"], r$access, by = "id")
  cells$value <- cells$gain
  cells$panel <- "map"
  gainers <- cells$id[cells$gain > 0]
  labels <- c("1–1,000", "1,000–5,000", "5,000–10,000", "10,000 or more")
  ways <- st_transform(st_geometry(r$ways), 3857)
  ext <- st_bbox(st_buffer(st_as_sfc(st_bbox(cells[cells$gain > 0, ])), 700))
  routes <- rbind(
    st_sf(when = "Today", geometry = st_geometry(st_transform(r$trip_before, 3857))),
    st_sf(when = "With the cycle path", geometry = st_geometry(st_transform(r$trip_after, 3857))))
  ends <- st_transform(st_as_sf(r$od, coords = c("lon", "lat"), crs = 4326), 3857)
  before <- weighted(r$access$accessibility_before, r$access$id)
  after <- weighted(r$access$accessibility_after, r$access$id)
  tb <- r$trip_before$total_duration[1]
  ta <- r$trip_after$total_duration[1]
  sub <- sprintf(paste0(
    "Jobs gained within 20 minutes by bicycle on low-stress streets (LTS 1–2; the bicycle is walked\n",
    "elsewhere) if Avenida Protásio Alves (green, about 7 km) had a protected cycle path. %s of\n",
    "residents gain; the average one reaches %s jobs instead of %s. The ride drawn takes\n",
    "%.1f minutes today and %.1f with the path"),
    pct(share_pop(gainers), 0), fmt(after), fmt(before), tb, ta)
  p <- ggplot() +
    poa_map(ext) +
    hex_layer(cells[cells$gain > 0, ], c(1, 1000, 5000, 10000, Inf), labels,
              ramp_access$light[c(2, 3, 4, 6)]) +
    geom_sf(data = ways, colour = light("forest"), linewidth = 1.1) +
    geom_sf(data = routes, aes(linetype = when), colour = light("route"), linewidth = 0.55) +
    scale_linetype_manual(values = c("Today" = "22", "With the cycle path" = "solid"), name = NULL) +
    geom_sf(data = ends, shape = 21, size = 2.2, stroke = 1, fill = light("ink"), colour = light("panel")) +
    guides(fill = guide_legend(nrow = 1, order = 1), linetype = guide_legend(order = 2)) +
    poa_coords(ext) +
    labs(title = "A new cycle path on Avenida Protásio Alves, Porto Alegre", subtitle = sub,
         caption = poa_credit) +
    theme_peabiru(map = TRUE) + legend_top + theme(legend.box = "vertical", legend.box.just = "left")
  save_svg(p, "scenario-cycle-path", width = 8, height = 6.6,
           title = "Jobs gained by bicycle with a new cycle path on Avenida Protásio Alves, Porto Alegre",
           desc = paste(
             "A map of Porto Alegre with the hexagons that gain access to jobs within 20 minutes by",
             "bicycle, on low-stress streets, if Avenida Protásio Alves, a radial avenue to the east",
             "of the centre, were given a protected cycle path. The gains follow the avenue and the",
             "neighbourhoods on both sides of it, from the centre to the eastern edge of the grid.",
             sprintf("%s of residents gain; one ride from the east end to the Santa Casa hospital", pct(share_pop(gainers), 0)),
             sprintf("drops from %.1f to %.1f minutes, following the avenue instead of side streets.", tb, ta)))
}

# -- congestion over the centre, and a closed avenue
centre_polygon <- function() {
  # 2 km around the Mercado Público, in metres (SIRGAS 2000 / UTM zone 22S)
  centre <- st_transform(st_sfc(st_point(c(-51.22781, -30.02756)), crs = 4326), 31982)
  st_sf(poly_id = "centre", scale = 0.5, priority = 1L,
        geometry = st_transform(st_buffer(centre, 2000, nQuadSegs = 16), 4326))
}
car_scenarios <- function() once("car", {
  d <- poa()
  ipiranga <- ways_named("Avenida Ipiranga")
  closed <- data.frame(osm_id = as.numeric(ipiranga$osm_id), max_speed = 0, speed_type = "km/h")
  ttm <- function(...) {
    m <- travel_time_matrix(d$network, d$hex, d$hex, mode = "CAR", n_threads = threads, ...)
    m[, c("from_id", "to_id", "travel_time_p50")]
  }
  base <- ttm()
  slow <- ttm(new_carspeeds = centre_polygon())
  shut <- ttm(new_carspeeds = closed)
  health <- function(...) accessibility(d$network, d$hex, d$hex, opportunities_colnames = "healthcare",
                                        mode = "CAR", cutoffs = 10, n_threads = threads, ...)
  list(ipiranga = ipiranga, closed = closed, base = base, slow = slow, shut = shut,
       health = list(base = health(), slow = health(new_carspeeds = centre_polygon()),
                     shut = health(new_carspeeds = closed)))
})
mean_change <- function(base, scen) {
  m <- merge(base, scen, by = c("from_id", "to_id"), all.x = TRUE, suffixes = c("", "_s"))
  # a pair no longer reached within the limit counts at the limit (120 min)
  m$travel_time_p50_s[is.na(m$travel_time_p50_s)] <- 120
  m$delta <- m$travel_time_p50_s - m$travel_time_p50
  m
}

figures$scen_car <- function() {
  r <- car_scenarios()
  d <- poa()
  a <- mean_change(r$base, r$slow)
  b <- mean_change(r$base, r$shut)
  per <- function(m, panel) {
    x <- aggregate(delta ~ from_id, m, mean)
    names(x) <- c("id", "value")
    x$panel <- panel
    x
  }
  pa <- "Half speed within 2 km of the centre"
  pb <- "Avenida Ipiranga closed to cars"
  v <- rbind(per(a, pa), per(b, pb))
  cells <- merge(d$cells[, "id"], v, by = "id")
  cells$panel <- factor(cells$panel, levels = c(pa, pb))
  labels <- c("0.5–1 min", "1–2", "2–3", "3 min or more")
  ring <- st_transform(st_geometry(centre_polygon()), 3857)
  shut <- st_transform(st_geometry(r$ipiranga), 3857)
  overlay <- rbind(st_sf(panel = factor(pa, levels = c(pa, pb)), geometry = st_cast(ring, "MULTILINESTRING")))
  hw <- function(x) weighted(x$accessibility, x$id)
  sub <- sprintf(paste0(
    "How much longer, on average, the drive from each cell to all the others takes (blank: under half a\n",
    "minute; free flow otherwise). Centre at half speed: +%.1f min over all pairs, and the health facilities\n",
    "within 10 minutes' drive of the average resident fall from %.1f to %.1f. Avenida Ipiranga closed:\n",
    "+%.1f min, and %.1f health facilities"),
    mean(a$delta), hw(r$health$base), hw(r$health$slow), mean(b$delta), hw(r$health$shut))
  p <- ggplot() +
    poa_map() +
    hex_layer(cells[cells$value >= 0.5, ], c(0.5, 1, 2, 3, Inf), labels, rev(ramp_time$light)[2:5]) +
    geom_sf(data = overlay, colour = light("ink"), linewidth = 0.45, linetype = "22") +
    geom_sf(data = st_sf(panel = factor(pb, levels = c(pa, pb)), geometry = shut),
            colour = light("ink"), linewidth = 0.8) +
    guides(fill = guide_legend(nrow = 1)) +
    facet_wrap(~panel, nrow = 1) +
    poa_coords() +
    labs(title = "Congestion and a closed avenue: what driving loses", subtitle = sub,
         caption = poa_credit) +
    theme_peabiru(map = TRUE) + legend_top
  save_svg(p, "scenario-car", width = 8, height = 5.6,
           title = "Change in driving times in Porto Alegre with congestion in the centre and with Avenida Ipiranga closed",
           desc = paste(
             "Two maps of Porto Alegre covered by hexagons coloured by how much longer, on average,",
             "it takes to drive from each cell to all the others. On the left, cars move at half",
             "speed within 2 km of the centre (dashed circle): the centre itself loses most, and the",
             "cells whose trips cross it lose a little. On the right, Avenida Ipiranga, an east-west",
             "arterial along the Dilúvio creek, is closed to cars: the cells along it lose most."))
}

# -- a new facility: walking time to the nearest school or health facility
walk_matrix <- function() once("walk", {
  d <- poa()
  travel_time_matrix(d$network, d$hex, d$hex, mode = "WALK", max_trip_duration = 60,
                     n_threads = threads)
})
new_facility <- function(kind) once(paste0("facility_", kind), {
  h <- poa()$hex
  walk <- walk_matrix()
  has <- h$id[h[[kind]] > 0]
  near <- function(sites) {
    w <- walk[walk$to_id %in% sites, ]
    t <- tapply(w$travel_time_p50, w$from_id, min)
    unname(t[h$id])                           # NA: none within an hour
  }
  before <- near(has)
  covered <- !is.na(before) & before <= 15
  # the cell where one more facility brings the most residents within 15 minutes
  w <- walk[walk$travel_time_p50 <= 15 & !covered[match(walk$from_id, h$id)], ]
  w$pop <- h$population[match(w$from_id, h$id)]
  gain <- sort(tapply(w$pop, w$to_id, sum), decreasing = TRUE)
  site <- names(gain)[1]
  after <- near(c(has, site))
  share <- function(t) sum(h$population[!is.na(t) & t <= 15]) / sum(h$population)
  list(site = site, before = before, after = after, gain = unname(gain[1]),
       share_before = share(before), share_after = share(after), n = length(has))
})

figures$scen_health <- function() {
  r <- new_facility("healthcare")
  print(r[c("site", "gain", "share_before", "share_after", "n")])
  s <- new_facility("schools")
  print(s[c("site", "gain", "share_before", "share_after", "n")])
  d <- poa()
  h <- d$hex
  inhabited <- h$population > 0
  pa <- "Today: walking time to the nearest one"
  pb <- "With one more: minutes saved"
  before <- ifelse(is.na(r$before) | r$before > 30, NA, r$before)
  # cells with no facility within the hour before, and one after, saved at least 30
  saved <- ifelse(is.na(r$before) & !is.na(r$after), 30, r$before - r$after)
  long <- rbind(data.frame(id = h$id, value = before, panel = pa)[inhabited, ],
                data.frame(id = h$id, value = saved, panel = pb)[inhabited, ])
  cells <- merge(d$cells[, "id"], long, by = "id")
  cells <- cells[!is.na(cells$value) & cells$value > 0, ]
  cells$panel <- factor(cells$panel, levels = c(pa, pb))
  # one legend, two rows: the left panel in minutes to the nearest, the right
  # in minutes saved
  near_lab <- c("under 5 min", "5–10", "10–15", "15–20", "20–30")
  saved_lab <- c("saves 1–2 min", "saves 2–5", "saves 5–10", "saves 10 or more")
  cells$class <- NA_character_
  l <- cells$panel == pa
  cells$class[l] <- as.character(cut(cells$value[l], c(0, 5, 10, 15, 20, 31), right = FALSE, labels = near_lab))
  cells$class[!l] <- as.character(cut(cells$value[!l], c(0, 2, 5, 10, Inf), right = FALSE, labels = saved_lab))
  cells$class <- factor(cells$class, levels = c(near_lab, saved_lab))
  cells <- cells[order(cells$panel, cells$class), ]
  site <- st_transform(st_as_sf(h[h$id == r$site, c("lon", "lat")], coords = c("lon", "lat"), crs = 4326), 3857)
  site <- rbind(transform(site, panel = factor(pa, levels = c(pa, pb))),
                transform(site, panel = factor(pb, levels = c(pa, pb))))
  ext <- poa_extent()
  sub <- sprintf(paste0(
    "Left: walking time to the nearest health facility, from each inhabited cell (blank: over 30 minutes).\n",
    "Right: the minutes saved with one more facility in the marked cell, the one that brings the most\n",
    "residents within 15 minutes: %s more people, from %s to %s of the population. Not a network\n",
    "scenario: one row added to the destinations, answered from the same travel-time matrix"),
    fmt(r$gain), pct(r$share_before), pct(r$share_after))
  p <- ggplot() +
    poa_map(ext) +
    geom_sf(data = cells, aes(fill = class), colour = NA, alpha = 0.9) +
    scale_fill_manual(values = setNames(c(ramp_time$light, ramp_access$light[c(2, 3, 5, 6)]),
                                        c(near_lab, saved_lab)), name = NULL, drop = FALSE) +
    guides(fill = guide_legend(nrow = 2, byrow = TRUE)) +
    geom_sf(data = site, shape = 21, size = 2.4, stroke = 1.1, fill = light("ink"), colour = light("panel")) +
    facet_wrap(~panel, nrow = 1) +
    poa_coords(ext) +
    labs(title = "One more health facility: who gains, and how much", subtitle = sub, caption = poa_credit) +
    theme_peabiru(map = TRUE) + legend_top + theme(legend.box = "vertical", legend.box.just = "left")
  save_svg(p, "scenario-health", width = 8, height = 6.2,
           title = "Walking time to the nearest health facility in Porto Alegre, and the minutes one more would save",
           desc = paste(
             "Two maps of Porto Alegre. On the left, hexagons coloured by the walking time to the",
             "nearest health facility of the grid; on the right, the minutes saved by the residents",
             "of each cell if one more facility were placed in the cell where it brings the most",
             sprintf("residents within 15 minutes' walk: %s more people, in the cells around it.", fmt(r$gain))))
}

# -- fares
fare_structures <- function() once("fares", {
  sample <- read_fare_structure(file.path(poa()$dir, "fares", "fares_poa.zip"))
  free <- sample
  # every ride after the first, within the hour, is free: two rides cost one
  # fare (and up to max_rides - 1 = 2 rides are discounted)
  free$fares_per_transfer$fare <- pmax(sample$fares_per_type$fare[match(free$fares_per_transfer$first_leg,
                                                                        sample$fares_per_type$type)],
                                       sample$fares_per_type$fare[match(free$fares_per_transfer$second_leg,
                                                                        sample$fares_per_type$type)])
  free$max_discounted_transfers <- 2L
  none <- sample
  none$max_discounted_transfers <- 0L
  list(sample = sample, free = free, none = none)
})
fare_access <- function() once("fare_access", {
  d <- poa()
  fs <- fare_structures()
  acc <- function(...) {
    a <- accessibility(d$network, d$hex, d$hex, opportunities_colnames = "jobs",
                       mode = c("WALK", "TRANSIT"), departure_datetime = monday_2pm,
                       time_window = 5, cutoffs = 45, n_threads = threads, seed = 42, ...)
    weighted(a$accessibility, a$id)
  }
  # the reference: the same (multi-criteria) router with a budget that never
  # binds. The router used without a fare structure is another algorithm and
  # its travel times differ a little, both ways, so it is no upper bound
  out <- data.frame(structure = "Any budget", budget = "none",
                    jobs = acc(fare_structure = fs$sample, max_fare = 1000))
  for (budget in c(4.8, 7.2)) {
    for (s in names(fs)) {
      out <- rbind(out, data.frame(structure = s, budget = sprintf("R$ %.2f", budget),
                                   jobs = acc(fare_structure = fs[[s]], max_fare = budget)))
    }
  }
  out
})

figures$scen_fares <- function() {
  a <- fare_access()
  print(a)
  top <- a$jobs[a$budget == "none"]
  x <- a[a$budget != "none", ]
  names_s <- c(sample = "Sample fares: second bus R$ 2.40", free = "Transfers free within the hour",
               none = "No transfer discount")
  x$structure <- factor(names_s[x$structure], levels = rev(names_s))
  x$budget <- factor(paste("Budget per trip:", x$budget), levels = paste("Budget per trip:", c("R$ 4.80", "R$ 7.20")))
  x$label <- paste0(fmt(x$jobs / 1000), "k")
  p <- ggplot(x, aes(y = structure, x = jobs)) +
    geom_col(fill = light("route"), width = 0.62) +
    geom_vline(xintercept = top, colour = light("ink"), linetype = "22", linewidth = 0.4) +
    geom_text(aes(label = label), hjust = 1.2, size = 3, colour = light("panel"), family = "Helvetica Neue") +
    facet_wrap(~budget, nrow = 1) +
    scale_x_continuous(labels = function(v) paste0(v / 1000, "k"), limits = c(0, top * 1.12),
                       expand = expansion(mult = c(0, 0))) +
    labs(title = "Jobs within 45 minutes by public transport, under three fare rules",
         subtitle = sprintf(paste0(
           "Average over the residents of the grid, walking and public transport, departures 14:00–14:05 on a Monday.\n",
           "The dashed line: with any budget, %sk jobs. With one fare to spend (R$ 4.80), free transfers bring the\n",
           "average from %sk to %sk jobs"), fmt(top / 1000),
           fmt(x$jobs[x$structure == names_s[["sample"]] & grepl("4.80", x$budget)] / 1000),
           fmt(x$jobs[x$structure == names_s[["free"]] & grepl("4.80", x$budget)] / 1000)),
         x = NULL, y = NULL, caption = poa_credit) +
    theme_peabiru(base_size = 10) +
    theme(panel.grid.major.y = element_blank(), strip.text = element_text(size = 10))
  save_svg(p, "scenario-fares", width = 8, height = 3.9,
           title = "Jobs reachable by public transport in Porto Alegre under three fare rules and two budgets",
           desc = paste(
             "Horizontal bars of the average number of jobs a resident of Porto Alegre reaches within",
             "45 minutes by public transport, under the sample fare structure, with free transfers",
             "within the hour, and with no transfer discount, for a budget of one fare (R$ 4.80) and of",
             "R$ 7.20. With one fare to spend, only free transfers give the access of an unlimited",
             "budget; with R$ 7.20, the sample's discount on the second bus comes close to it."))
}

# ------------------------------------------------------------------ Helsinki
hel <- function() once("hel", {
  dir <- prepare_helsinki(cache)
  cells <- st_read(file.path(dir, "cells.gpkg"), quiet = TRUE)
  list(dir = dir, network = build_network(file.path(dir, "network"), n_threads = threads),
       cells = st_transform(cells, 3857), points = read.csv(file.path(dir, "cells.csv"), colClasses = c(id = "character")),
       jobs = read.csv(file.path(dir, "jobs.csv")),
       bg = hel_background(file.path(dir, "network", "helsinki.osm.pbf")))
})
# The background of the Helsinki maps: main streets, lakes and the coastline,
# thinned for a map of 40 km across (about 50 m per pixel). The ways wanted
# are filtered with osmium first: GDAL's OSM driver, reading one layer of an
# extract this size, drops ways whose nodes it has not kept.
hel_background <- function(pbf) {
  small <- tempfile(fileext = ".osm.pbf")
  on.exit(unlink(small))
  roads <- c(motorway = 0.6, trunk = 0.6, primary = 0.4, secondary = 0.25)
  status <- system2("osmium", c("tags-filter", "--overwrite", pbf,
                                paste0("w/highway=", paste(names(roads), collapse = ",")),
                                "w/natural=coastline", "wr/natural=water", "-o", small))
  stopifnot(status == 0)
  l <- st_transform(suppressWarnings(st_read(small, "lines", quiet = TRUE)), 3857)
  merged <- function(g, tolerance) {
    g <- st_cast(st_simplify(st_line_merge(st_union(g)), dTolerance = tolerance), "LINESTRING")
    g[as.numeric(st_length(g)) > 1000]
  }
  lines <- do.call(rbind, lapply(sort(unique(roads)), function(w) {
    st_sf(w = w, geometry = merged(st_geometry(l)[l$highway %in% names(roads)[roads == w]], 40))
  }))
  coast <- merged(st_geometry(l)[!is.na(l$other_tags) &
                                   grepl('"natural"=>"coastline"', l$other_tags, fixed = TRUE)], 80)
  water <- suppressWarnings(st_read(small, "multipolygons", quiet = TRUE))
  water <- st_transform(st_make_valid(st_geometry(water)[!is.na(water$natural) & water$natural == "water"]), 3857)
  water <- st_simplify(water[as.numeric(st_area(water)) > 2e5], dTolerance = 40)
  list(lines = lines, water = water[!st_is_empty(water)], coast = coast)
}
tuesday_730 <- as.POSIXct("2026-11-17 07:30:00", tz = "Europe/Helsinki")
hel_station <- data.frame(id = "Helsinki Central Station", lon = 24.94124, lat = 60.17128)
hel_extent <- function() once("hel_extent", st_bbox(st_buffer(st_as_sfc(st_bbox(hel()$cells)), 500)))
hel_map <- function(ext) {
  bg <- hel()$bg
  lines <- bg$lines[lengths(st_intersects(bg$lines, st_as_sfc(ext))) > 0, ]
  box <- st_as_sfc(st_bbox(st_buffer(st_as_sfc(ext), 200)))
  list(geom_sf(data = bg$water[lengths(st_intersects(bg$water, box)) > 0], fill = light("water"), colour = NA),
       geom_sf(data = bg$coast[lengths(st_intersects(bg$coast, box)) > 0], colour = light("soft"), linewidth = 0.2),
       geom_sf(data = lines, aes(linewidth = w), colour = light("street"), show.legend = FALSE),
       scale_linewidth_identity())
}
hel_coords <- function(ext) {
  coord_sf(xlim = ext[c("xmin", "xmax")], ylim = ext[c("ymin", "ymax")],
           expand = FALSE, crs = 3857, datum = NA)
}
hel_credit <- paste0(
  "Map data © OpenStreetMap contributors (ODbL), Geofabrik extract of 2026-11-11. Timetables © HSL 2026 (CC BY 4.0),\n",
  "feed of 2026-11-12. Residents (2025) and jobs (2024, companies and non-profits): © HSY (CC BY 4.0).")

hel_access <- function() once("hel_access", {
  h <- hel()
  accessibility(h$network, h$points, h$jobs, opportunities_colnames = "jobs",
                mode = c("WALK", "TRANSIT"), departure_datetime = tuesday_730,
                time_window = 60, cutoffs = c(30, 45), n_threads = threads)
})

figures$hel_jobs <- function() {
  h <- hel()
  a <- hel_access()
  a <- a[a$cutoff == 45, ]
  cells <- merge(h$cells[, "id"], data.frame(id = a$id, value = a$accessibility / 1000), by = "id")
  n_cells <- nrow(cells)
  total <- sum(h$jobs$jobs)
  w <- h$points$population[match(a$id, h$points$id)]
  avg <- stats::weighted.mean(a$accessibility, w)
  half <- sum(w[a$accessibility >= total / 2]) / sum(w)
  labels <- c("under 25k", "25k–50k", "50k–100k", "100k–150k", "150k–250k", "250k or more")
  ext <- hel_extent()
  st <- st_transform(st_as_sf(hel_station, coords = c("lon", "lat"), crs = 4326), 3857)
  # one shape per class: the cells of a class dissolved (a smaller file)
  cells$class <- cut(cells$value, c(0, 25, 50, 100, 150, 250, Inf), labels = labels, right = FALSE)
  classes <- do.call(rbind, lapply(labels, function(l) {
    g <- st_union(st_buffer(st_geometry(cells)[cells$class == l], 0.5, joinStyle = "MITRE", mitreLimit = 2))
    g <- st_simplify(g, dTolerance = 2)
    st_sf(class = factor(l, levels = labels), geometry = g)
  }))
  p <- ggplot() +
    hel_map(ext) +
    geom_sf(data = classes, aes(fill = class), colour = NA, alpha = 0.9) +
    scale_fill_manual(values = setNames(ramp_access$light, labels), drop = FALSE, name = NULL) +
    geom_sf(data = st, shape = 21, size = 2.2, stroke = 1, fill = light("ink"), colour = light("panel")) +
    guides(fill = guide_legend(nrow = 1)) +
    hel_coords(ext) +
    labs(title = "Jobs within 45 minutes by public transport, Helsinki capital region",
         subtitle = sprintf(paste0(
           "Walking and HSL's buses, trams, metro, trains and ferries, departures 07:30–08:30 on Tuesday\n",
           "17 November 2026 (median), from %s cells of 250 m with %s residents to %s jobs in\n",
           "companies and non-profits. The average resident reaches %s of them (%s); %s of\n",
           "residents reach at least half. Dot: Helsinki Central Station"),
           fmt(n_cells), fmt(sum(h$points$population)), fmt(total), fmt(avg), pct(avg / total, 0), pct(half, 0)),
         caption = hel_credit) +
    theme_peabiru(map = TRUE) + legend_top
  save_svg(p, "helsinki-jobs", width = 8, height = 6.4,
           title = "Jobs reachable within 45 minutes by public transport in the Helsinki capital region",
           desc = paste(
             "A map of Helsinki, Espoo, Vantaa and Kauniainen covered by 250 m cells coloured by the",
             "number of jobs reachable within 45 minutes walking and by public transport at the",
             "morning peak. The centre of Helsinki and the cells along the metro and the railway",
             "lines reach the most, over 250 thousand jobs; the edges of Espoo and Vantaa reach",
             "fewer than 25 thousand."))
}

figures$hel_iso <- function() {
  h <- hel()
  modes <- list(WALK = "Walking", BICYCLE = "Cycling", CAR = "Driving (free flow)",
                TRANSIT = "Walking + public transport")
  cutoffs <- c(15, 30, 45)
  # isochrone() repairs the polygons with st_make_valid(), as r5r does; with
  # s2 on, that repair enlarges the 30-minute driving polygon by 45 km2
  s2 <- suppressMessages(sf_use_s2(FALSE))
  on.exit(suppressMessages(sf_use_s2(s2)))
  polys <- do.call(rbind, lapply(names(modes), function(m) {
    mode <- if (m == "TRANSIT") c("WALK", "TRANSIT") else m
    iso <- isochrone(h$network, hel_station, mode = mode, cutoffs = cutoffs, zoom = 10,
                     departure_datetime = tuesday_730, time_window = 60, n_threads = threads)
    iso$mode <- modes[[m]]
    iso[, c("mode", "isochrone")]
  }))
  polys <- st_transform(polys, 3857)
  polys$mode <- factor(polys$mode, levels = unlist(modes))
  polys$band <- factor(paste(polys$isochrone, "min"), levels = paste(cutoffs, "min"))
  polys <- polys[order(polys$mode, -polys$isochrone), ]
  ext <- hel_extent()
  polys <- suppressWarnings(st_intersection(st_make_valid(polys), st_as_sfc(st_bbox(st_buffer(st_as_sfc(ext), 200)))))
  polys <- st_simplify(polys, dTolerance = 60)
  area <- aggregate(as.numeric(st_area(st_transform(polys, 3067))) / 1e6,
                    list(mode = polys$mode, band = polys$band), sum)
  print(area)
  st <- st_transform(st_as_sf(hel_station, coords = c("lon", "lat"), crs = 4326), 3857)
  p <- ggplot() +
    hel_map(ext) +
    geom_sf(data = polys, aes(fill = band), colour = NA, alpha = 0.82) +
    geom_sf(data = st, shape = 21, size = 2, stroke = 1, fill = light("ink"), colour = light("panel")) +
    facet_wrap(~mode, nrow = 2) +
    scale_fill_manual(values = ramp_time$light[c(1, 3, 4)], name = NULL) +
    hel_coords(ext) +
    labs(title = "Isochrones from Helsinki Central Station, by mode",
         subtitle = "Areas reached within 15, 30 and 45 minutes; by public transport, the median of the departures\n07:30–08:30 on Tuesday 17 November 2026. Driving is free flow: no congestion",
         caption = hel_credit) +
    theme_peabiru(map = TRUE) + legend_top
  save_svg(p, "helsinki-isochrones", width = 8, height = 7.4,
           title = "Isochrones by mode from Helsinki Central Station",
           desc = paste(
             "Four maps of the Helsinki capital region, one per mode, each shading the areas reached",
             "from Helsinki Central Station within 15, 30 and 45 minutes. Walking reaches the",
             "peninsula of the centre; cycling most of Helsinki; driving, with free-flowing traffic,",
             "most of the region; public transport reaches long arms along the metro to the east and",
             "the west and along the railway lines to the north."))
}

# ------------------------------------------------------------------ run
wanted <- commandArgs(TRUE)
if (length(wanted) == 0) wanted <- names(figures)
unknown <- setdiff(wanted, names(figures))
if (length(unknown)) stop("no figure named ", paste(unknown, collapse = ", "),
                          " (have: ", paste(names(figures), collapse = ", "), ")")
for (f in wanted) figures[[f]]()
