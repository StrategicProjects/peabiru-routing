# The figures of the documentation sites, computed with the peabiru R package
# on the sample networks of Porto Alegre (r5r's) and São Paulo (peabiru's).
#
#   Rscript site/figures/figures.R            # all figures
#   Rscript site/figures/figures.R iso_modes  # only the ones named
#
# or `bash site/figures/make.sh`, which also installs the R package into a
# private library when needed. Writes site/assets/figures/*.svg
# (PEABIRU_FIGURES_OUT changes the directory) and prints the size of each.
# Needs the peabiru, sf, ggplot2 and svglite R packages.

root_dir <- local({
  f <- sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE))
  normalizePath(file.path(dirname(f), "..", ".."))
})
source(file.path(root_dir, "site", "figures", "common.R"))

threads <- 4
poa <- poa_dir()
net_poa <- build_network(poa, n_threads = threads)
hex <- read.csv(file.path(poa, "poa_hexgrid.csv"))
poi <- read.csv(file.path(poa, "poa_points_of_interest.csv"))
bg <- background(file.path(poa, "poa_osm.pbf"))
monday_2pm <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")

# The origin of the single-origin figures: Farroupilha park (Redenção).
origin <- poi[poi$id == "farroupilha_park", c("id", "lon", "lat")]
origin_3857 <- st_transform(st_as_sf(origin, coords = c("lon", "lat"), crs = 4326), 3857)

# Map extent: the hexagonal grid, a little padded.
hex_sf <- hexagons(hex)
extent <- st_bbox(st_buffer(st_as_sfc(st_bbox(hex_sf)), 300))

base_map <- function() {
  list(
    geom_sf(data = bg$water, fill = light("water"), colour = NA),
    geom_sf(data = bg$lines, aes(linewidth = w), colour = light("street"),
            show.legend = FALSE),
    scale_linewidth_identity()
  )
}
map_coords <- function() {
  coord_sf(xlim = extent[c("xmin", "xmax")], ylim = extent[c("ymin", "ymax")],
           expand = FALSE, crs = 3857, datum = NA)
}
origin_mark <- function(size = 2.6) {
  geom_sf(data = origin_3857, shape = 21, size = size, stroke = 1.1,
          fill = light("ink"), colour = light("panel"))
}
osm_credit <- "Map data © OpenStreetMap contributors (ODbL); GTFS: EPTC and Trensurb, via r5r's sample data."

figures <- list()

# ------------------------------------------------------------------ isochrones by mode
figures$iso_modes <- function() {
  cutoffs <- c(10, 20, 30)
  modes <- list(
    walk = list(mode = "walk", label = "Walking"),
    bicycle = list(mode = "bicycle", label = "Cycling"),
    car = list(mode = "car", label = "Driving"),
    transit = list(mode = c("walk", "transit"), label = "Walking + public transport")
  )
  polys <- do.call(rbind, lapply(names(modes), function(m) {
    iso <- isochrone(net_poa, origin, mode = modes[[m]]$mode, cutoffs = cutoffs,
                     zoom = 11, departure_datetime = monday_2pm, time_window = 30,
                     n_threads = threads)
    iso$mode <- modes[[m]]$label
    iso[, c("mode", "isochrone")]
  }))
  polys <- st_transform(polys, 3857)
  polys$mode <- factor(polys$mode, levels = vapply(modes, `[[`, "", "label"))
  polys$band <- factor(paste(polys$isochrone, "min"), levels = paste(cutoffs, "min"))
  polys <- polys[order(polys$mode, -polys$isochrone), ]

  p <- ggplot() +
    base_map() +
    geom_sf(data = polys, aes(fill = band), colour = NA, alpha = 0.82) +
    origin_mark() +
    facet_wrap(~mode, nrow = 2) +
    scale_fill_manual(values = ramp_time$light[c(1, 3, 4)], name = NULL) +
    map_coords() +
    labs(title = "Isochrones from Farroupilha Park, Porto Alegre, by mode",
         subtitle = "Areas reached within 10, 20 and 30 minutes; by public transport, the median\nof the departures between 14:00 and 14:30 on a Monday",
         caption = osm_credit) +
    theme_peabiru(map = TRUE) +
    theme(legend.position = "top", legend.justification = "left",
          legend.margin = margin(0, 0, 0, 0), legend.key.size = unit(10, "pt"))
  save_svg(p, "isochrones-by-mode", width = 8, height = 9.2,
           title = "Isochrones by mode from Farroupilha Park, Porto Alegre",
           desc = paste(
             "Four maps of the same part of Porto Alegre, one per mode, each shading the",
             "areas reached from Farroupilha Park within 10, 20 and 30 minutes. Walking reaches",
             "a small area around the park; cycling a few kilometres around it; driving",
             "almost the whole map; walking plus public transport reaches long fingers along",
             "the bus corridors and the train line, between walking and driving."))
}

# ------------------------------------------------------------------ travel-time map
figures$travel_time_map <- function() {
  ttm <- function(mode) {
    m <- travel_time_matrix(net_poa, origin, hex, mode = mode,
                            departure_datetime = monday_2pm, time_window = 30,
                            max_trip_duration = 60, n_threads = threads)
    merge(hex_sf, data.frame(id = m$to_id, value = m$travel_time_p50), by = "id")
  }
  walk <- ttm("WALK")
  walk$panel <- "Walking"
  transit <- ttm(c("WALK", "TRANSIT"))
  transit$panel <- "Walking + public transport"
  both <- rbind(walk, transit)
  labels <- c("under 10 min", "10–20", "20–30", "30–45", "45–60")
  p <- ggplot() +
    base_map() +
    hex_layer(both, c(0, 10, 20, 30, 45, 61), labels, ramp_time$light) +
    origin_mark() +
    facet_wrap(~panel, nrow = 1) +
    map_coords() +
    labs(title = "Travel time from Farroupilha Park to every cell of a grid",
         subtitle = "Porto Alegre, departures between 14:00 and 14:30 on a Monday (median); blank: more than an hour",
         caption = osm_credit) +
    theme_peabiru(map = TRUE) +
    theme(legend.position = "top", legend.justification = "left",
          legend.margin = margin(0, 0, 0, 0), legend.key.size = unit(10, "pt"))
  save_svg(p, "travel-time-map", width = 8, height = 5.4,
           title = "Travel time from one origin to a grid, walking and by public transport",
           desc = paste(
             "Two maps of Porto Alegre covered by a grid of hexagons, coloured by the median travel",
             "time from Farroupilha Park: on the left walking, on the right walking plus public",
             "transport. Walking reaches the cells within about three kilometres in an hour; with",
             "public transport most of the grid is within an hour, and the cells along the bus",
             "corridors and the train line are reached in 30 to 45 minutes."))
}

# ------------------------------------------------------------------ accessibility map
figures$accessibility_map <- function() {
  acc <- function(mode) {
    a <- accessibility(net_poa, hex, hex, opportunities_colnames = "jobs",
                       mode = mode, departure_datetime = monday_2pm,
                       time_window = 30, decay_function = "step", cutoffs = 30,
                       n_threads = threads)
    merge(hex_sf, data.frame(id = a$id, value = a$accessibility), by = "id")
  }
  walk <- acc("WALK")
  walk$panel <- "Walking"
  transit <- acc(c("WALK", "TRANSIT"))
  transit$panel <- "Walking + public transport"
  both <- rbind(walk, transit)
  breaks <- c(0, 5e3, 1e4, 25e3, 5e4, 1e5, Inf)
  labels <- c("under 5k", "5k–10k", "10k–25k", "25k–50k", "50k–100k", "100k or more")
  p <- ggplot() +
    base_map() +
    hex_layer(both, breaks, labels, ramp_access$light) +
    guides(fill = guide_legend(nrow = 1)) +
    facet_wrap(~panel, nrow = 1) +
    map_coords() +
    labs(title = "Jobs reachable within 30 minutes",
         subtitle = sprintf("Porto Alegre, %s jobs on %s grid cells; by public transport, the median of the\ndepartures between 14:00 and 14:30 on a Monday",
                            format(sum(hex$jobs, na.rm = TRUE), big.mark = ","),
                            format(nrow(hex), big.mark = ",")),
         caption = osm_credit) +
    theme_peabiru(map = TRUE) +
    theme(legend.position = "top", legend.justification = "left",
          legend.margin = margin(0, 0, 0, 0), legend.key.size = unit(10, "pt"))
  save_svg(p, "accessibility-jobs", width = 8, height = 5.4,
           title = "Jobs reachable within 30 minutes, walking and by public transport",
           desc = paste(
             "Two maps of Porto Alegre covered by a grid of hexagons, coloured by the number of jobs",
             "reachable within 30 minutes from each cell. Walking, the cells of the city centre",
             "reach 50 to 100 thousand and the south of the grid fewer than 10 thousand. With",
             "public transport, the centre and the corridors leading to it reach more than 100",
             "thousand, and most of the grid more than 25 thousand."))
}

# ------------------------------------------------------------------ share of people reached, by mode
figures$reach_by_mode <- function() {
  modes <- list(walk = "WALK", bicycle = "BICYCLE", car = "CAR",
                transit = c("WALK", "TRANSIT"))
  label <- c(walk = "Walking", bicycle = "Cycling", car = "Driving",
             transit = "Walking + public transport")
  pop <- sum(hex$population)
  curves <- do.call(rbind, lapply(names(modes), function(m) {
    t <- travel_time_matrix(net_poa, origin, hex, mode = modes[[m]],
                            departure_datetime = monday_2pm, time_window = 30,
                            max_trip_duration = 60, n_threads = threads)
    reached <- merge(t, hex[, c("id", "population")], by.x = "to_id", by.y = "id")
    minutes <- 0:60
    share <- vapply(minutes, function(x)
      sum(reached$population[reached$travel_time_p50 <= x]) / pop, 0)
    data.frame(mode = m, minutes = minutes, share = share)
  }))
  curves$mode <- factor(curves$mode, levels = names(modes))
  ends <- curves[curves$minutes == 60, ]
  ends$label <- sprintf("%s  %d%%", label[as.character(ends$mode)], round(100 * ends$share))
  # labels at the end of the curves, moved apart where the curves end together
  ends <- ends[order(-ends$share), ]
  ends$y <- ends$share
  for (i in seq_len(nrow(ends))[-1]) ends$y[i] <- min(ends$y[i], ends$y[i - 1] - 0.075)
  cols <- vapply(mode_colour[names(modes)], light, "")
  p <- ggplot(curves, aes(minutes, share, colour = mode)) +
    geom_step(linewidth = 1.1, direction = "hv") +
    geom_text(data = ends, aes(y = y, label = label), hjust = 0, nudge_x = 1.2, size = 3.5,
              family = "Helvetica Neue") +
    scale_colour_manual(values = cols, guide = "none") +
    scale_x_continuous(breaks = seq(0, 60, 10), limits = c(0, 95),
                       expand = expansion(mult = c(0, 0))) +
    scale_y_continuous(labels = function(x) paste0(round(100 * x), "%"),
                       limits = c(0, 1), expand = expansion(mult = c(0, 0.02))) +
    labs(title = "Share of Porto Alegre's population within reach, by mode",
         subtitle = "From Farroupilha Park; the grid's population counted at each cell",
         x = "Travel time (minutes)", y = NULL) +
    theme_peabiru() +
    theme(panel.grid.major.x = element_blank(),
          axis.title.x = element_text(colour = light("soft"), hjust = 0))
  save_svg(p, "reach-by-mode", width = 7.2, height = 3.9,
           title = "Share of the population within reach of one origin, by mode and travel time",
           desc = paste(
             "Four step curves of the share of the population of the Porto Alegre grid reached from",
             "Farroupilha Park within each number of minutes, from 0 to 60. Driving rises fastest",
             "and reaches nearly everyone within about 20 minutes; walking plus public transport",
             "and cycling rise more slowly, and walking reaches the smallest share in an hour."))
}

# ------------------------------------------------------------------ percentiles on a frequency network
figures$percentiles <- function() {
  spo <- spo_dir()
  net <- build_network(spo, n_threads = threads)
  pts <- read.csv(file.path(spo, "spo_points.csv"))
  from <- pts[1, ]
  dep <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")
  m <- travel_time_matrix(net, from, pts[-1, ], mode = c("WALK", "TRANSIT"),
                          departure_datetime = dep, time_window = 60,
                          percentiles = c(5, 25, 50, 75, 95), seed = 42,
                          n_threads = threads)
  m <- m[order(m$travel_time_p50, m$travel_time_p95), ]
  m$rank <- seq_len(nrow(m))
  p <- ggplot(m, aes(y = rank)) +
    geom_segment(aes(x = travel_time_p05, xend = travel_time_p95, yend = rank),
                 colour = light("stop"), linewidth = 1.2, lineend = "round") +
    geom_segment(aes(x = travel_time_p25, xend = travel_time_p75, yend = rank),
                 colour = light("route"), linewidth = 3.2, lineend = "butt") +
    geom_point(aes(x = travel_time_p50), shape = 21, size = 2.2, stroke = 0.9,
               fill = light("panel"), colour = light("ink")) +
    scale_y_reverse(breaks = NULL) +
    scale_x_continuous(limits = c(0, NA), expand = expansion(mult = c(0, 0.04))) +
    labs(title = "One trip, many travel times: percentiles on a frequency-based network",
         subtitle = "São Paulo, from one point to 26 others by walking and public transport, departures from\n14:00 to 15:00 with 5 random schedules per minute (seed 42). Thin bar: 5th to 95th\npercentile; thick bar: 25th to 75th; dot: median. Points reached on foot have no spread.",
         x = "Travel time (minutes)", y = "Destinations, by median") +
    theme_peabiru() +
    theme(panel.grid.major.y = element_blank(),
          axis.title.x = element_text(colour = light("soft"), hjust = 0),
          axis.title.y = element_text(colour = light("soft"), hjust = 1))
  save_svg(p, "percentiles", width = 7.2, height = 5.2,
           title = "Percentiles of the travel time on a frequency-based network",
           desc = paste(
             "For 26 destinations in São Paulo, sorted by median travel time from one point,",
             "a range bar from the 5th to the 95th percentile of the travel times by walking and",
             "public transport over an hour of departures, a thicker bar from the 25th to the 75th,",
             "and a dot at the median. Destinations reached on foot have no spread; those reached",
             "by bus or metro spread by several minutes, mostly from waiting."))
}

# ------------------------------------------------------------------ trip by trip
figures$trip_breakdown <- function() {
  to <- poi[poi$id == "pucrs", c("id", "lon", "lat")]
  e <- expanded_travel_time_matrix(net_poa, origin, to, mode = c("WALK", "TRANSIT"),
                                   departure_datetime = monday_2pm, time_window = 30,
                                   breakdown = TRUE, n_threads = threads)
  parts <- c(access_time = "Walk to the stop", wait_time = "Wait",
             ride_time = "Ride", transfer_time = "Transfer (walk and wait)",
             egress_time = "Walk from the stop")
  long <- do.call(rbind, lapply(names(parts), function(k)
    data.frame(departure = e$departure_time, part = parts[[k]], minutes = e[[k]])))
  long$part <- factor(long$part, levels = rev(parts))
  hhmm <- function(x) substr(x, 1, 5)
  long$departure <- factor(hhmm(long$departure), levels = rev(sort(unique(hhmm(e$departure_time)))))
  e$departure <- factor(hhmm(e$departure_time), levels = levels(long$departure))
  e$label <- sprintf("%s · %s min", gsub("\\|", " → ", e$routes),
                     format(e$total_time, nsmall = 1))
  cols <- setNames(c(light("forest"), light("stop"), light("route"), light("slate"),
                     light("forest2")), unname(parts))
  p <- ggplot(long, aes(x = minutes, y = departure, fill = part)) +
    geom_col(width = 0.72, colour = light("panel"), linewidth = 0.25) +
    geom_text(data = e, aes(x = total_time, y = departure, label = label), inherit.aes = FALSE,
              hjust = 0, nudge_x = 0.8, size = 2.7, colour = light("soft"),
              family = "Helvetica Neue") +
    scale_fill_manual(values = cols, breaks = unname(parts), name = NULL) +
    scale_x_continuous(breaks = seq(0, 40, 10), limits = c(0, 62),
                       expand = expansion(mult = c(0, 0))) +
    labs(title = "Trip by trip: Farroupilha Park to PUCRS, Porto Alegre",
         subtitle = "Each departure minute from 14:00 to 14:29 on a Monday, split into its parts, with the\nroutes taken and the total travel time",
         x = "Minutes", y = "Departure") +
    theme_peabiru(base_size = 10) +
    theme(panel.grid.major.y = element_blank(), legend.position = "top",
          legend.justification = "left", legend.key.size = unit(9, "pt"),
          legend.margin = margin(0, 0, 0, 0),
          axis.title.x = element_text(colour = light("soft"), hjust = 0),
          axis.title.y = element_text(colour = light("soft"), hjust = 1),
          axis.text.y = element_text(size = 7.5))
  save_svg(p, "trip-breakdown", width = 7.2, height = 6.6,
           title = "Trips by public transport between two points, departure by departure",
           desc = paste(
             "Thirty horizontal stacked bars, one per departure minute from 14:00 to 14:29, from",
             "Farroupilha Park to PUCRS in Porto Alegre: walk to the stop, wait, ride, transfer and",
             "walk from the stop, with the routes taken and the total time at the end of each bar.",
             "Totals range between about 30 and 40 minutes; the wait changes most from minute to",
             "minute, and the routes change as earlier buses are missed."))
}

# ------------------------------------------------------------------ run
wanted <- commandArgs(TRUE)
if (length(wanted) == 0) wanted <- names(figures)
unknown <- setdiff(wanted, names(figures))
if (length(unknown)) stop("no figure named ", paste(unknown, collapse = ", "),
                          " (have: ", paste(names(figures), collapse = ", "), ")")
for (f in wanted) figures[[f]]()
