# The figures of the "Brazilian capitals with open GTFS" example: access to
# jobs, schools and health facilities by public transport in São Paulo, Rio de
# Janeiro and Belo Horizonte, by income, and, on r5r's sample of Porto Alegre
# with the fare structure r5r ships, travel time against fare. Computed with
# the peabiru R package on the data prepared by site/figures/capitals-data.R.
#
#   Rscript site/figures/capitals.R               # all of them
#   Rscript site/figures/capitals.R cap_income    # only the ones named
#
# or `bash site/figures/make.sh --capitals`. The first run downloads about
# 1 GB into the cache of capitals-data.R and needs osmium. The results of the
# routing (the slow part: 22 minutes for São Paulo, 9 for Rio de Janeiro on
# four threads of a busy laptop) are kept in <cache>/results/ with a log of
# the time each step took, and reused; delete that directory to compute them
# again.

root_dir <- local({
  f <- sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE))
  normalizePath(file.path(dirname(f), "..", ".."))
})
source(file.path(root_dir, "site", "figures", "common.R"))
source(file.path(root_dir, "site", "figures", "capitals-data.R"))
suppressPackageStartupMessages(library(patchwork))

threads <- 4
cache <- cap_cache()
results <- file.path(cache, "results")
dir.create(results, showWarnings = FALSE)
keys <- c("spo", "rio", "bho")
city_name <- vapply(cap_cities[keys], `[[`, "", "name")

# The analysis: Tuesday 17 November 2026, departures every minute from 07:00
# to 07:59, the median travel time over that hour; frequency-based routes
# (São Paulo, nearly all of Rio de Janeiro) with R5's Monte Carlo draws, seed 42.
departure <- as.POSIXct("2026-11-17 07:00:00", tz = "America/Sao_Paulo")
window <- 60
seed <- 42

rss_mib <- function() {
  as.numeric(system2("ps", c("-o", "rss=", "-p", Sys.getpid()), stdout = TRUE)) / 1024
}
log_line <- function(...) {
  line <- sprintf(...)
  message(line)
  cat(format(Sys.time(), "%Y-%m-%d %H:%M  "), line, "\n", file = file.path(results, "log.txt"),
      append = TRUE, sep = "")
}

nets <- list()
get_net <- function(key) {
  if (is.null(nets[[key]])) {
    dir <- prepare_capital(key, cache)
    t <- system.time(n <- build_network(file.path(dir, "network"), n_threads = threads))
    log_line("%s: network built in %.1f s, R process RSS now %.0f MiB", key, t[["elapsed"]], rss_mib())
    nets[[key]] <<- n
  }
  nets[[key]]
}
cells_of <- function(key) {
  read.csv(file.path(prepare_capital(key, cache), "cells.csv"), colClasses = c(id = "character"))
}
origins_of <- function(cells) cells[cells$population > 0, ]
destinations_of <- function(cells) cells[cells$jobs + cells$schools + cells$health > 0, ]

# One row per populated cell: its population, income decile, and the
# opportunities reached (columns jobs_30, jobs_60, schools_30, ...), from a
# long table of accessibility().
widen <- function(a, key) {
  a$what <- paste(a$opportunity, a$cutoff, sep = "_")
  w <- reshape(a[, c("id", "what", "accessibility")], idvar = "id", timevar = "what",
               direction = "wide")
  names(w) <- sub("^accessibility\\.", "", names(w))
  out <- merge(origins_of(cells_of(key)), w, by = "id", all.x = TRUE)
  for (n in setdiff(names(w), "id")) out[[n]][is.na(out[[n]])] <- 0
  out
}

# Cumulative access (a step at 30 and 60 minutes) to jobs, schools and health
# facilities from every populated cell.
access_of <- function(key) {
  f <- file.path(results, paste0(key, "-access.csv"))
  if (!file.exists(f)) {
    net <- get_net(key)
    cells <- cells_of(key)
    o <- origins_of(cells)
    d <- destinations_of(cells)
    t <- system.time(a <- accessibility(
      net, o, d, opportunities_colnames = c("jobs", "schools", "health"),
      mode = c("WALK", "TRANSIT"), departure_datetime = departure, time_window = window,
      percentiles = 50, decay_function = "step", cutoffs = c(30, 60),
      max_trip_duration = 60, seed = seed, n_threads = threads, progress = FALSE))
    log_line("%s: accessibility, %d origins x %d destinations, %.1f s, R process RSS now %.0f MiB",
             key, nrow(o), nrow(d), t[["elapsed"]], rss_mib())
    write.csv(a, f, row.names = FALSE)
  }
  widen(read.csv(f, colClasses = c(id = "character")), key)
}
access_cache <- list()
get_access <- function(key) {
  if (is.null(access_cache[[key]])) access_cache[[key]] <<- access_of(key)
  access_cache[[key]]
}

# ------------------------------------------------------------------ summaries
wmean <- function(x, w) sum(as.numeric(x) * w) / sum(as.numeric(w))   # integers would overflow
wmedian <- function(x, w) {
  o <- order(x)
  x[o][which(cumsum(as.numeric(w[o])) >= sum(as.numeric(w)) / 2)[1]]
}
# The share of the city's opportunities reached, by income decile
# (population-weighted mean), and the Palma ratio: the mean of the richest
# tenth over the mean of the poorest four tenths.
by_decile <- function(key, column) {
  a <- get_access(key)
  a <- a[!is.na(a$decile), ]
  total <- sum(cells_of(key)[[sub("_[0-9]+$", "", column)]])
  share <- a[[column]] / total
  data.frame(city = city_name[[key]], decile = 1:10,
             mean = vapply(1:10, function(k) wmean(share[a$decile == k], a$population[a$decile == k]), 0))
}
palma <- function(key, column) {
  a <- get_access(key)
  a <- a[!is.na(a$decile), ]
  rich <- a$decile == 10
  poor <- a$decile <= 4
  wmean(a[[column]][rich], a$population[rich]) / wmean(a[[column]][poor], a$population[poor])
}

# ------------------------------------------------------------------ maps
cap_credit <- paste0(
  "Streets © OpenStreetMap contributors (ODbL), Geofabrik extract of 2026-11-01. GTFS: SPTrans (São Paulo), SMTR (Rio de Janeiro, CC BY 4.0),\n",
  "SUMOB (Belo Horizonte, CC BY). Cells, population and income (Censo 2010), jobs (RAIS 2019), schools and health facilities (2019): Ipea, aopdata.")
bg_cache <- list()
get_bg <- function(key) {
  if (is.null(bg_cache[[key]])) {
    bg_cache[[key]] <<- background(file.path(cache, key, "network", paste0(key, ".osm.pbf")))
  }
  bg_cache[[key]]
}
boundary_of <- function(key) {
  st_transform(st_read(file.path(cache, key, "boundary.geojson"), quiet = TRUE), 3857)
}
cell_polygons <- function(key) {
  st_transform(st_read(file.path(cache, key, "cells.gpkg"), quiet = TRUE)[, "id"], 3857)
}
# Cells of one class, dissolved into one polygon each and thinned: a map of
# 10,000 hexagons, drawn one by one, would weigh megabytes.
dissolve <- function(cells, class) {
  parts <- lapply(levels(class), function(l) {
    g <- st_union(st_geometry(cells)[!is.na(class) & class == l])
    if (length(g) == 0 || st_is_empty(g)) return(NULL)
    st_sf(class = factor(l, levels = levels(class)),
          geometry = st_simplify(st_buffer(g, 1), dTolerance = 70))
  })
  do.call(rbind, parts[!vapply(parts, is.null, TRUE)])
}
map_layers <- function(key, extent) {
  bg <- get_bg(key)
  box <- st_as_sfc(st_bbox(st_buffer(st_as_sfc(extent), 200)))
  # a whole city on one map: only the main roads, thinned further
  lines <- bg$lines[bg$lines$w >= 0.5, ]
  lines <- st_simplify(suppressWarnings(st_intersection(lines, box)), dTolerance = 60)
  lines <- lines[!st_is_empty(lines), ]
  water <- bg$water[lengths(st_intersects(bg$water, box)) > 0]
  water <- water[as.numeric(st_area(water)) > 2e5]
  water <- st_simplify(water, dTolerance = 60)
  list(geom_sf(data = water, fill = light("water"), colour = NA),
       geom_sf(data = lines, aes(linewidth = w), colour = light("street"), show.legend = FALSE),
       scale_linewidth_identity())
}
legend_top <- theme(legend.position = "top", legend.justification = "left",
                    legend.margin = margin(0, 0, 0, 0), legend.key.size = unit(10, "pt"))
pct <- function(x, digits = 0) paste0(formatC(100 * x, format = "f", digits = digits), "%")
# axis labels: a decimal when the breaks are not whole percentages
pct_axis <- function(x) {
  whole <- all(abs(100 * x - round(100 * x)) < 1e-6, na.rm = TRUE)
  ifelse(is.na(x), NA, pct(x, if (whole) 0 else 1))
}
wquantile <- function(x, w, p) {
  o <- order(x)
  cw <- cumsum(as.numeric(w[o])) / sum(as.numeric(w))
  vapply(p, function(q) x[o][which(cw >= q)[1]], 0)
}
num <- function(x) format(round(x), big.mark = ",")

figures <- list()

# ------------------------------------------------------------------ jobs within an hour, one map per city
# the same classes in the three cities, so the maps compare
share_breaks <- c(0, 0.05, 0.10, 0.20, 0.40, 0.60, 1.01)
share_labels <- c("under 5%", "5–10%", "10–20%", "20–40%", "40–60%", "60% or more")

map_credit <- c(
  spo = "Streets © OpenStreetMap contributors (ODbL). GTFS: SPTrans, as fetched by the Mobility Database on 12 November 2026.\nCells, population (Censo 2010) and jobs (RAIS 2019): Ipea, Access to Opportunities Project (aopdata).",
  rio = "Streets © OpenStreetMap contributors (ODbL). GTFS: SMTR, Prefeitura do Rio de Janeiro, 10 June 2026 (CC BY 4.0). Cells, population\n(Censo 2010) and jobs (RAIS 2019): Ipea, Access to Opportunities Project (aopdata).",
  bho = "Streets © OpenStreetMap contributors (ODbL). GTFS: SUMOB, Prefeitura de Belo Horizonte,\n11 November 2026 (CC BY). Cells, population (Censo 2010) and jobs (RAIS 2019): Ipea, Access to\nOpportunities Project (aopdata).")
map_note <- c(spo = "", rio = ". Buses and BRT only: the feed has no metro, train, VLT or ferry",
              bho = ". Buses only: the feed has no metro")

jobs_map <- function(key, width, height, desc) {
  note <- map_note[[key]]
  a <- get_access(key)
  total <- sum(cells_of(key)$jobs)
  a$share <- a$jobs_60 / total
  med <- wmedian(a$share, a$population)
  cells <- merge(cell_polygons(key), a[, c("id", "share")], by = "id")
  cells$class <- cut(cells$share, share_breaks, labels = share_labels, right = FALSE)
  polys <- dissolve(cells, cells$class)
  b <- boundary_of(key)
  extent <- st_bbox(st_buffer(st_as_sfc(st_bbox(b)), 500))
  p <- ggplot() +
    map_layers(key, extent) +
    geom_sf(data = polys, aes(fill = class), colour = NA, alpha = 0.9) +
    geom_sf(data = st_geometry(b), fill = NA, colour = light("soft"), linewidth = 0.3,
            linetype = "22") +
    scale_fill_manual(values = setNames(ramp_access$light, share_labels), drop = FALSE, name = NULL) +
    guides(fill = guide_legend(nrow = 1)) +
    coord_sf(xlim = extent[c("xmin", "xmax")], ylim = extent[c("ymin", "ymax")], expand = FALSE,
             crs = 3857, datum = NA) +
    labs(title = sprintf("Jobs within an hour by public transport, %s", city_name[[key]]),
         subtitle = sprintf(paste0(
           "Share of the municipality's %s formal jobs reached from each cell,\n",
           "walking and riding, at the morning peak (median of departures 07:00–07:59,\n",
           "Tuesday 17 November 2026). Half the residents reach %s or less.\n",
           "Blank: cells without residents%s"),
           num(total), pct(med, 1), note),
         caption = map_credit[[key]]) +
    theme_peabiru(base_size = 10, map = TRUE) + legend_top +
    theme(plot.caption = element_text(lineheight = 1.1, size = 6.5))
  save_svg(p, paste0("jobs-60min-", key), width = width, height = height,
           title = sprintf("Jobs within 60 minutes by public transport, %s", city_name[[key]]),
           desc = desc(med))
}

figures$cap_jobs_spo <- function() {
  jobs_map("spo", width = 7, height = 8.6, desc = function(med) paste0(
    "A map of the municipality of São Paulo covered by cells coloured by the share of the city's formal ",
    "jobs reached within 60 minutes by walking and public transport at the morning peak. The centre and ",
    "the south-west, where the jobs are, reach the most; access falls towards the far east, the far south ",
    "and the north. Half the residents reach ", pct(med, 1), " of the jobs or less."))
}
figures$cap_jobs_rio <- function() {
  jobs_map("rio", width = 8, height = 6, desc = function(med) paste0(
    "A map of the municipality of Rio de Janeiro covered by cells coloured by the share of the city's ",
    "formal jobs reached within 60 minutes by walking and public transport (buses and BRT; the feed has no ",
    "rail) at the morning peak. The centre and the neighbourhoods around it reach the most; the west ",
    "reaches the least. Half the residents reach ", pct(med, 1), " of the jobs or less."))
}
figures$cap_jobs_bho <- function() {
  jobs_map("bho", width = 6, height = 7.6, desc = function(med) paste0(
    "A map of the municipality of Belo Horizonte covered by cells coloured by the share of the city's ",
    "formal jobs reached within 60 minutes by walking and bus at the morning peak. The centre, inside the ",
    "Avenida do Contorno, and the cells around it reach the most; the edges of the city the least. ",
    "Half the residents reach ", pct(med, 1), " of the jobs or less."))
}

# ------------------------------------------------------------------ access by income decile
figures$cap_income <- function() {
  rows <- do.call(rbind, lapply(keys, function(k) {
    a <- get_access(k)
    a <- a[!is.na(a$decile), ]
    data.frame(city = city_name[[k]], decile = factor(a$decile, levels = 1:10),
               share = a$jobs_60 / sum(cells_of(k)$jobs), population = a$population)
  }))
  rows$city <- factor(rows$city, levels = city_name)
  ratios <- vapply(keys, palma, 0, column = "jobs_60")
  lab <- data.frame(city = factor(city_name, levels = city_name),
                    text = sprintf("Palma ratio %.1f", ratios))
  print(lab)
  boxes <- do.call(rbind, lapply(split(rows, list(rows$city, rows$decile)), function(r) {
    q <- wquantile(r$share, r$population, c(0.1, 0.25, 0.5, 0.75, 0.9))
    data.frame(city = r$city[1], decile = r$decile[1], p10 = q[1], p25 = q[2], p50 = q[3],
               p75 = q[4], p90 = q[5])
  }))
  write.csv(boxes, file.path(results, "income-boxes.csv"), row.names = FALSE)
  p <- ggplot(boxes, aes(decile)) +
    geom_linerange(aes(ymin = p10, ymax = p90), colour = light("forest"), linewidth = 0.4) +
    geom_crossbar(aes(y = p50, ymin = p25, ymax = p75), fill = light("forest2"),
                  colour = light("forest"), linewidth = 0.3, width = 0.7, middle.linewidth = 0.7) +
    geom_text(data = lab, aes(x = 0.6, y = 0.97, label = text), inherit.aes = FALSE, hjust = 0,
              size = 3.1, colour = light("ink"), family = "Helvetica Neue") +
    facet_wrap(~city, nrow = 1) +
    scale_y_continuous(labels = function(x) pct(x), limits = c(0, 1), expand = expansion(mult = c(0, 0.02))) +
    scale_x_discrete(labels = c("1\npoorest", 2:9, "10\nrichest")) +
    labs(title = "Jobs within an hour by public transport, by income",
         subtitle = paste0(
           "Share of the city's formal jobs reached from each cell, by income decile of its residents (Censo 2010),\n",
           "weighted by population. Boxes: quartiles and median; lines: 10th to 90th percentile. Palma ratio: the mean\n",
           "of the richest tenth over the mean of the poorest four tenths. Morning peak, Tuesday 17 November 2026"),
         x = "Income decile (per capita income of the cell)", y = NULL, caption = cap_credit) +
    theme_peabiru(base_size = 10) +
    theme(panel.grid.major.x = element_blank(), axis.text.x = element_text(size = 6.5, lineheight = 0.9),
          plot.caption = element_text(size = 6.5, lineheight = 1.1))
  save_svg(p, "access-income-capitals", width = 8.4, height = 4.6,
           title = "Jobs within 60 minutes by public transport, by income decile, São Paulo, Rio de Janeiro and Belo Horizonte",
           desc = paste0(
             "Three panels of box plots, one per city, of the share of the city's formal jobs reached within ",
             "60 minutes by public transport from the cells of each income decile. In all three cities access ",
             "rises with income. The richest tenth reaches on average ",
             paste(sprintf("%.1f times as many jobs as the poorest four tenths in %s", ratios, city_name),
                   collapse = ", "), "."))
}

# ------------------------------------------------------------------ the three cities, three opportunities
figures$cap_compare <- function() {
  what <- c(jobs_60 = "Jobs, within 60 minutes", schools_30 = "Public schools, within 30 minutes",
            health_30 = "Health facilities, within 30 minutes")
  rows <- do.call(rbind, lapply(names(what), function(col) {
    r <- do.call(rbind, lapply(keys, by_decile, column = col))
    r$what <- what[[col]]
    r
  }))
  rows$what <- factor(rows$what, levels = what)
  rows$city <- factor(rows$city, levels = city_name)
  ratios <- outer(keys, names(what), Vectorize(function(k, col) palma(k, col)))
  dimnames(ratios) <- list(city_name, names(what))
  print(round(ratios, 2))
  write.csv(rows, file.path(results, "by-decile.csv"), row.names = FALSE)
  cols <- c(light("route"), light("forest"), light("slate"))
  p <- ggplot(rows, aes(decile, mean, colour = city)) +
    geom_line(linewidth = 0.7) + geom_point(size = 1.4) +
    facet_wrap(~what, nrow = 1, scales = "free_y") +
    scale_colour_manual(values = setNames(cols, city_name), name = NULL) +
    scale_x_continuous(breaks = c(1, 4, 7, 10), labels = c("1\npoorest", 4, 7, "10\nrichest")) +
    scale_y_continuous(labels = pct_axis, limits = c(0, NA), expand = expansion(mult = c(0, 0.06))) +
    labs(title = "Access to opportunities by public transport, by income decile",
         subtitle = sprintf(paste0(
           "Mean share of the city's opportunities reached at the morning peak, weighted by population.\n",
           "Palma ratio for jobs: São Paulo %.1f, Rio de Janeiro %.1f, Belo Horizonte %.1f"),
           ratios[1, 1], ratios[2, 1], ratios[3, 1]),
         x = "Income decile", y = NULL, caption = cap_credit) +
    theme_peabiru(base_size = 10) + legend_top +
    theme(plot.caption = element_text(size = 6.5, lineheight = 1.1),
          axis.text.x = element_text(size = 7, lineheight = 0.9))
  save_svg(p, "access-compare-capitals", width = 8.4, height = 4.2,
           title = "Access to jobs, schools and health facilities by public transport, by income decile, in three Brazilian capitals",
           desc = paste0(
             "Three panels, for jobs within 60 minutes and public schools and health facilities within 30 ",
             "minutes, each with one line per city of the mean share of the city's opportunities reached by ",
             "public transport, by income decile. Access to jobs rises steeply with income in all three ",
             "cities; schools, spread across the city, vary much less. Palma ratios for jobs: ",
             sprintf("São Paulo %.1f, Rio de Janeiro %.1f, Belo Horizonte %.1f.",
                     ratios[1, 1], ratios[2, 1], ratios[3, 1])))
}

# ------------------------------------------------------------------ against Ipea's own estimates for 2019
# The Access to Opportunities Project published, for the same cells and the
# same jobs, access by public transport at the morning peak of 2019 (r5r, the
# GTFS feeds of 2019, departures 06:00-08:00). Not a check of peabiru: the
# feeds, the street data, the window and the engine all differ.
aop_of <- function(key) {
  f <- fetch(sprintf(cap_sources$aop, sprintf("access_2019_public_transport_%s.csv", cap_cities[[key]]$aop)),
             file.path(cache, "raw", sprintf("access_2019_public_transport_%s.csv", cap_cities[[key]]$aop)))
  a <- read.csv(f)
  a <- a[a$peak == 1, c("id_hex", "CMATT60")]
  names(a) <- c("id", "aop_jobs_60")
  a
}
aop_compare <- function() {
  do.call(rbind, lapply(keys, function(k) {
    a <- merge(get_access(k), aop_of(k), by = "id")
    a <- a[!is.na(a$decile), ]
    data.frame(city = city_name[[k]], decile = 1:10,
               peabiru = vapply(1:10, function(d) wmean(a$jobs_60[a$decile == d], a$population[a$decile == d]), 0),
               aop = vapply(1:10, function(d) wmean(a$aop_jobs_60[a$decile == d], a$population[a$decile == d]), 0),
               cells = nrow(a),
               correlation = cor(a$jobs_60, a$aop_jobs_60))
  }))
}
figures$cap_aop <- function() {
  r <- aop_compare()
  print(r)
  write.csv(r, file.path(results, "aop-compare.csv"), row.names = FALSE)
  long <- rbind(data.frame(r[, c("city", "decile")], jobs = r$peabiru, source = "peabiru, GTFS of 2026"),
                data.frame(r[, c("city", "decile")], jobs = r$aop, source = "Ipea (r5r), GTFS of 2019"))
  long$city <- factor(long$city, levels = city_name)
  long$source <- factor(long$source, levels = c("peabiru, GTFS of 2026", "Ipea (r5r), GTFS of 2019"))
  cors <- unique(r[, c("city", "correlation")])
  p <- ggplot(long, aes(decile, jobs, colour = source, linetype = source)) +
    geom_line(linewidth = 0.7) + geom_point(size = 1.3) +
    facet_wrap(~city, nrow = 1, scales = "free_y") +
    scale_colour_manual(values = c(light("route"), light("slate")), name = NULL) +
    scale_linetype_manual(values = c("solid", "22"), name = NULL) +
    scale_x_continuous(breaks = c(1, 4, 7, 10), labels = c("1\npoorest", 4, 7, "10\nrichest")) +
    scale_y_continuous(labels = function(x) paste0(round(x / 1000), "k"), limits = c(0, NA),
                       expand = expansion(mult = c(0, 0.06))) +
    labs(title = "Jobs within an hour: peabiru in 2026 and Ipea's estimates for 2019",
         subtitle = sprintf(paste0(
           "Mean formal jobs (RAIS 2019) reached by public transport at the morning peak, by income decile.\n",
           "Correlation between the two, cell by cell: %s"),
           paste(sprintf("%s %.2f", cors$city, cors$correlation), collapse = ", ")),
         x = "Income decile", y = NULL, caption = cap_credit) +
    theme_peabiru(base_size = 10) + legend_top +
    theme(plot.caption = element_text(size = 6.5, lineheight = 1.1),
          axis.text.x = element_text(size = 7, lineheight = 0.9))
  save_svg(p, "access-aop-capitals", width = 8.4, height = 4.2,
           title = "Jobs within 60 minutes by public transport: peabiru with the feeds of 2026 against Ipea's estimates for 2019",
           desc = paste0(
             "Three panels, one per city, each with two lines of the mean number of formal jobs reached within ",
             "60 minutes by public transport by income decile: computed here with the GTFS feeds of 2026, and ",
             "published by Ipea's Access to Opportunities Project for 2019. ",
             paste(sprintf("In %s the cell-by-cell correlation is %.2f.", cors$city, cors$correlation),
                   collapse = " ")))
}

# ------------------------------------------------------------------ time against fare, Porto Alegre
# r5r's sample of Porto Alegre and the fare structure r5r ships for it: the
# feeds are of May 2019 and only have service in the afternoon, hence Monday
# 13 May 2019, departures 14:00-14:59.
poa_departure <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")
poa_budgets <- c(one = 4.8, two = 9.6, any = Inf)
poa_credit <- paste0(
  "Sample data of r5r 2.5.0 (Ipea): streets © OpenStreetMap contributors (ODbL); GTFS of EPTC and Trensurb, May 2019; fare structure\n",
  "of r5r. Cells, population and jobs: r5r's sample grid; income (Censo 2010): Ipea, aopdata.")
poa_env <- new.env()
get_poa <- function() {
  if (is.null(poa_env$net)) {
    dir <- prepare_poa(cache)
    poa_env$dir <- dir
    poa_env$net <- build_network(file.path(dir, "network"), n_threads = threads)
    poa_env$fares <- read_fare_structure(file.path(dir, "fares_poa.zip"))
    poa_env$cells <- read.csv(file.path(dir, "cells.csv"), colClasses = c(id = "character"))
  }
  poa_env
}
# Jobs within 60 minutes from every populated cell, with a limit on the fare.
poa_access <- function() {
  f <- file.path(results, "poa-fare-access.csv")
  if (!file.exists(f)) {
    p <- get_poa()
    o <- p$cells[p$cells$population > 0, ]
    d <- p$cells[p$cells$jobs > 0, ]
    out <- o[, c("id", "population", "income", "quintile")]
    for (b in names(poa_budgets)) {
      t <- system.time(a <- accessibility(
        p$net, o, d, opportunities_colnames = "jobs", mode = c("WALK", "TRANSIT"),
        departure_datetime = poa_departure, time_window = 60, percentiles = 50, cutoffs = 60,
        max_trip_duration = 60, fare_structure = p$fares, max_fare = poa_budgets[[b]],
        draws_per_minute = 1, n_threads = threads, progress = FALSE))
      log_line("poa: accessibility with max_fare %s, %d origins x %d destinations, %.1f s",
               format(poa_budgets[[b]]), nrow(o), nrow(d), t[["elapsed"]])
      out[[b]] <- a$accessibility[match(out$id, a$id)]
    }
    write.csv(out, f, row.names = FALSE)
  }
  read.csv(f, colClasses = c(id = "character"))
}

figures$cap_fare_budget <- function() {
  a <- poa_access()
  total <- sum(get_poa()$cells$jobs)
  a <- a[!is.na(a$quintile), ]
  rows <- do.call(rbind, lapply(names(poa_budgets), function(b) {
    data.frame(quintile = 1:5, budget = b,
               share = vapply(1:5, function(q) wmean(a[[b]][a$quintile == q], a$population[a$quintile == q]), 0) / total)
  }))
  kept <- reshape(rows, idvar = "quintile", timevar = "budget", direction = "wide")
  kept$one <- kept$share.one / kept$share.any
  kept$two <- kept$share.two / kept$share.any
  all_kept <- c(one = wmean(a$one, a$population) / wmean(a$any, a$population),
                two = wmean(a$two, a$population) / wmean(a$any, a$population))
  print(kept); print(all_kept)
  write.csv(kept, file.path(results, "poa-fare-budget.csv"), row.names = FALSE)
  labels <- c(any = "any fare", two = "up to two bus fares (R$ 9.60)", one = "up to one bus fare (R$ 4.80)")
  rows$budget <- factor(labels[rows$budget], levels = labels)
  cols <- setNames(c(light("forest2"), light("forest"), light("route")), labels)
  p <- ggplot(rows, aes(factor(quintile), share, fill = budget)) +
    geom_col(position = position_dodge(width = 0.8), width = 0.74) +
    geom_text(data = kept, aes(x = factor(quintile), y = share.one, label = pct(one)),
              inherit.aes = FALSE, nudge_x = 0.267, vjust = -0.5, size = 2.9, colour = light("ink"),
              family = "Helvetica Neue") +
    scale_fill_manual(values = cols, name = NULL) +
    scale_x_discrete(labels = c("1\npoorest", 2:4, "5\nrichest")) +
    scale_y_continuous(labels = function(x) pct(x), expand = expansion(mult = c(0, 0.08))) +
    labs(title = "What a fare budget leaves of the jobs within an hour, Porto Alegre",
         subtitle = sprintf(paste0(
           "Mean share of the sample's %s jobs reached within 60 minutes by public transport, by income quintile\n",
           "of the city (Censo 2010), Monday 13 May 2019, departures 14:00–14:59. Over the bars of one fare: the part\n",
           "of the access at any fare that it keeps (%s for all residents; two fares keep %s)"),
           num(total), pct(all_kept[["one"]]), pct(all_kept[["two"]])),
         x = "Income quintile of the cell's residents", y = NULL, caption = poa_credit) +
    theme_peabiru(base_size = 10) + legend_top +
    theme(panel.grid.major.x = element_blank(), plot.caption = element_text(size = 6.5, lineheight = 1.1),
          axis.text.x = element_text(size = 7.5, lineheight = 0.9))
  save_svg(p, "fare-budget-poa", width = 7.6, height = 4.6,
           title = "Jobs within 60 minutes by public transport under a fare budget, by income quintile, Porto Alegre",
           desc = paste0(
             "Grouped bars, one group per income quintile, of the mean share of jobs reached within 60 minutes by ",
             "public transport in the Porto Alegre sample at any fare, with at most two bus fares and with at most ",
             "one. One bus fare, which pays a single ride, keeps ", pct(all_kept[["one"]]),
             " of the access at any fare, and two fares, which pay a transfer, ", pct(all_kept[["two"]]),
             "; by quintile, one fare keeps ",
             paste(sprintf("%s in the %s", pct(kept$one), c("poorest", "second", "third", "fourth", "richest")),
                   collapse = ", "), "."))
}

# ------------------------------------------------------------------ the frontier of three trips
# Three pairs of cells of the sample, chosen to show three shapes of the
# trade-off (ids are H3 cells of r5r's grid).
poa_pairs <- data.frame(
  from = c("89a9012aed3ffff", "89a9012aa7bffff", "89a9012830bffff"),
  to = c("89a9012aa7bffff", "89a9012992bffff", "89a901284c7ffff"),
  label = c("Along a corridor\nCavalhada to Praia de Belas", "Across town\nPraia de Belas to Navegantes",
            "East to south\nPetrópolis to Teresópolis"),
  key = c("corridor", "across", "south"))
poa_cutoffs <- c(0, 4.5, 4.8, 7.2, 8.37, 9.6, 12)

poa_frontier <- function() {
  f <- file.path(results, "poa-frontier.csv")
  if (!file.exists(f)) {
    p <- get_poa()
    rows <- do.call(rbind, lapply(seq_len(nrow(poa_pairs)), function(i) {
      o <- p$cells[p$cells$id == poa_pairs$from[i], ]
      d <- p$cells[p$cells$id == poa_pairs$to[i], ]
      pf <- pareto_frontier(p$net, o, d, mode = c("WALK", "TRANSIT"), departure_datetime = poa_departure,
                            time_window = 60, percentiles = 50, max_trip_duration = 120,
                            fare_structure = p$fares, fare_cutoffs = poa_cutoffs, seed = 42,
                            n_threads = threads, progress = FALSE)
      data.frame(key = poa_pairs$key[i], fare = round(pf$monetary_cost, 2), minutes = pf$travel_time)
    }))
    write.csv(rows, f, row.names = FALSE)
  }
  read.csv(f)
}

figures$cap_pareto <- function() {
  pf <- poa_frontier()
  print(pf)
  pf$label <- factor(poa_pairs$label[match(pf$key, poa_pairs$key)], levels = poa_pairs$label)
  # a step that holds until the next cutoff, and to the right edge
  steps <- do.call(rbind, lapply(split(pf, pf$key), function(r) {
    r <- r[order(r$fare), ]
    rbind(r, transform(r[nrow(r), ], fare = 13.5))
  }))
  what <- c(`0` = "on foot", `4.5` = "one train", `4.8` = "one bus", `7.2` = "two buses",
            `8.37` = "bus and train", `12` = "three rides")
  pf$what <- sprintf("%s\n%d min", what[as.character(pf$fare)], pf$minutes)
  pf$left <- pf$fare == 4.5   # the train's label goes to the left of its point
  # a step of a minute or two right after another: its label goes under the line
  pf <- pf[order(pf$key, pf$fare), ]
  same <- c(FALSE, pf$key[-1] == pf$key[-nrow(pf)])
  pf$below <- same & c(0, diff(pf$fare)) < 2.5 & c(0, diff(pf$minutes)) > -4 & !pf$left
  p <- ggplot(pf, aes(fare, minutes)) +
    geom_step(data = steps, colour = light("route"), linewidth = 0.8) +
    geom_point(colour = light("route"), size = 2) +
    geom_text(data = pf[pf$below, ], aes(label = what), hjust = 0, vjust = 1.3, nudge_x = 0.15,
              size = 2.7, lineheight = 0.95, colour = light("ink"), family = "Helvetica Neue") +
    geom_text(data = pf[!pf$left & !pf$below, ], aes(label = what), hjust = 0, vjust = -0.35, nudge_x = 0.15,
              size = 2.7, lineheight = 0.95, colour = light("ink"), family = "Helvetica Neue") +
    geom_text(data = pf[pf$left, ], aes(label = what), hjust = 1, vjust = 0.5, nudge_x = -0.35,
              size = 2.7, lineheight = 0.95, colour = light("ink"), family = "Helvetica Neue") +
    facet_wrap(~label, nrow = 1) +
    scale_x_continuous(breaks = c(0, 4.8, 7.2, 12), labels = c("0", "4.80", "7.20", "12.00"),
                       limits = c(-0.4, 17.4), expand = c(0, 0)) +
    scale_y_continuous(limits = c(0, NA), expand = expansion(mult = c(0, 0.16))) +
    labs(title = "What each fare buys in travel time, Porto Alegre",
         subtitle = paste0(
           "Median travel time of the fastest trip within each fare budget, door to door, departures 14:00–14:59,\n",
           "Monday 13 May 2019. Each point is a budget at which the time changes: R$ 4.50 pays a train, 4.80 a bus,\n",
           "7.20 two buses with the transfer discount, 12.00 three rides"),
         x = "Fare budget (R$)", y = "Minutes", caption = poa_credit) +
    theme_peabiru(base_size = 10) +
    theme(panel.grid.major.x = element_blank(), strip.text = element_text(size = 8.5, lineheight = 1.05),
          plot.caption = element_text(size = 6.5, lineheight = 1.1))
  at <- function(k, fare) pf$minutes[pf$key == k & abs(pf$fare - fare) < 0.01]
  save_svg(p, "pareto-poa", width = 8.4, height = 4.2,
           title = "Travel time against fare budget for three trips in Porto Alegre",
           desc = paste0(
             "Three step charts of the median travel time of the fastest trip within a fare budget, for three ",
             "pairs of places in Porto Alegre. Along a corridor, one bus fare (R$ 4.80) brings the trip from ",
             at("corridor", 0), " minutes on foot to ", at("corridor", 4.8), " and a second ride saves a minute. ",
             "Across town, one fare gives ", at("across", 4.8), " minutes, most of them walking to a direct line, ",
             "and the R$ 7.20 of two buses with the transfer discount gives ", at("across", 7.2), ". From ",
             "Petrópolis to Teresópolis, each step still buys time: ", at("south", 4.8), " minutes with one bus, ",
             at("south", 7.2), " with two and ", at("south", 12), " with three rides, at R$ 12.00."))
}

# ------------------------------------------------------------------ the options of one trip
# From Jardim Carvalho, in the east, to Praia de Belas, leaving at 14:19.
# (Neighbourhood names: OpenStreetMap, through Nominatim.)
poa_trip <- list(from = "89a9012917bffff", to = "89a9012aa7bffff",
                 departure = as.POSIXct("2019-05-13 14:19:00", tz = "America/Sao_Paulo"))

figures$cap_trip_options <- function() {
  p <- get_poa()
  o <- p$cells[p$cells$id == poa_trip$from, ]
  d <- p$cells[p$cells$id == poa_trip$to, ]
  res <- trip_options(p$net, o, d, departure_datetime = poa_trip$departure, time_window = 1,
                      criteria = c("TIME", "FARE", "TRANSFERS"), fare_structure = p$fares,
                      n_threads = threads, progress = FALSE)
  opt <- as.data.frame(res$options)
  print(opt[, c("option", "total_time", "fare", "transfers", "walk_time", "description")])
  stopifnot(nrow(opt) == 3)
  legs <- st_transform(res$itineraries, 3857)
  legs <- legs[!st_is_empty(legs), ]
  names_opt <- c("Fastest", "In between", "Cheapest, no transfer")
  cols <- c(light("route"), light("stop"), light("forest"))
  legs$name <- factor(names_opt[legs$option], levels = names_opt)
  legs$walk <- legs$mode == "WALK"
  ends <- st_transform(st_as_sf(rbind(o, d), coords = c("lon", "lat"), crs = 4326), 3857)
  ends$label <- c("A", "B")
  extent <- st_bbox(st_buffer(st_as_sfc(st_bbox(legs)), 500))
  bg <- background(file.path(p$dir, "network", "poa_osm.pbf"))
  box <- st_as_sfc(extent)
  lines <- bg$lines[lengths(st_intersects(bg$lines, box)) > 0, ]
  water <- bg$water[lengths(st_intersects(bg$water, box)) > 0]
  xy <- st_coordinates(ends)
  map <- ggplot() +
    geom_sf(data = water, fill = light("water"), colour = NA) +
    geom_sf(data = lines, aes(linewidth = w), colour = light("street"), show.legend = FALSE) +
    scale_linewidth_identity() +
    # the cheapest first and widest, the fastest last and thinnest: where two
    # options share a street both show
    geom_sf(data = legs[!legs$walk & legs$option == 3, ], aes(colour = name), linewidth = 1.4, lineend = "round") +
    geom_sf(data = legs[!legs$walk & legs$option == 2, ], aes(colour = name), linewidth = 2.2, lineend = "round") +
    geom_sf(data = legs[!legs$walk & legs$option == 1, ], aes(colour = name), linewidth = 0.9, lineend = "round") +
    geom_sf(data = legs[legs$walk, ], aes(colour = name), linewidth = 0.7, linetype = "11",
            show.legend = FALSE) +
    geom_sf(data = ends, shape = 21, size = 5, stroke = 1, fill = light("panel"), colour = light("ink")) +
    annotate("text", x = xy[, 1], y = xy[, 2], label = ends$label, size = 2.8, fontface = "bold",
             colour = light("ink"), family = "Helvetica Neue") +
    scale_colour_manual(values = setNames(cols, names_opt), name = NULL, drop = FALSE) +
    coord_sf(xlim = extent[c("xmin", "xmax")], ylim = extent[c("ymin", "ymax")], expand = FALSE,
             crs = 3857, datum = NA) +
    theme_peabiru(base_size = 10, map = TRUE) +
    theme(legend.position = "none", plot.margin = margin(0, 0, 0, 0))
  # the table, drawn as text beside the map
  routes <- gsub("WALK > | > WALK", "", opt$description)
  routes <- gsub("BUS ", "", routes)
  tab <- data.frame(
    y = rep(3:1, each = 1),
    name = names_opt,
    line1 = sprintf("%.0f min  ·  R$ %.2f", opt$total_time, opt$fare),
    line2 = sprintf("%d %s  ·  %.0f min on foot", opt$transfers + 1L,
                    ifelse(opt$transfers == 0, "bus", "buses"), opt$walk_time),
    line3 = paste(ifelse(opt$transfers == 0, "line", "lines"), gsub(" > ", ", then ", routes)))
  side <- ggplot(tab) +
    geom_segment(aes(x = 0, xend = 0.09, y = y + 0.24, yend = y + 0.24, colour = name), linewidth = 1.6,
                 lineend = "round", show.legend = FALSE) +
    geom_text(aes(0.13, y + 0.24, label = name), hjust = 0, size = 3.3, fontface = "bold",
              colour = light("ink"), family = "Helvetica Neue") +
    geom_text(aes(0, y, label = line1), hjust = 0, size = 3.5, colour = light("ink"), family = "Helvetica Neue") +
    geom_text(aes(0, y - 0.2, label = line2), hjust = 0, size = 2.9, colour = light("soft"), family = "Helvetica Neue") +
    geom_text(aes(0, y - 0.38, label = line3), hjust = 0, size = 2.9, colour = light("soft"), family = "Helvetica Neue") +
    scale_colour_manual(values = setNames(cols, names_opt)) +
    scale_x_continuous(limits = c(0, 1), expand = c(0, 0)) +
    scale_y_continuous(limits = c(0.3, 3.6), expand = c(0, 0)) +
    theme_void() + theme(plot.margin = margin(0, 0, 0, 12))
  fig <- map + side + plot_layout(widths = c(1.75, 1)) +
    plot_annotation(
      title = "Three ways from Jardim Carvalho to Praia de Belas at 14:19, and what each costs",
      subtitle = paste0(
        "The trips from A to B that no other beats on time, fare and transfers at once (trip_options), leaving at 14:19\n",
        "on Monday 13 May 2019. Solid: on the bus; dotted: on foot"),
      caption = poa_credit,
      theme = theme_peabiru(base_size = 10) +
        theme(plot.caption = element_text(size = 6.5, lineheight = 1.1)))
  save_svg(fig, "trip-options-poa", width = 8.4, height = 4.6,
           title = "The fastest, the cheapest and the option in between for one trip across Porto Alegre",
           desc = sprintf(paste0(
             "A map of Porto Alegre with three trips from Jardim Carvalho, in the east, to Praia de Belas, south of the ",
             "centre, and beside it their time, fare and transfers. The fastest takes %.0f minutes on three buses ",
             "and costs R$ %.2f; with two buses and the transfer discount, %.0f minutes for R$ %.2f; the ",
             "cheapest, a single bus for R$ %.2f, takes %.0f minutes, %.0f of them on foot."),
             opt$total_time[1], opt$fare[1], opt$total_time[2], opt$fare[2], opt$fare[3], opt$total_time[3],
             opt$walk_time[3]))
}

# ------------------------------------------------------------------ run
if (sys.nframe() == 0L) {
  wanted <- commandArgs(TRUE)
  if (length(wanted) == 0) wanted <- names(figures)
  unknown <- setdiff(wanted, names(figures))
  if (length(unknown)) stop("no figure named ", paste(unknown, collapse = ", "),
                            " (have: ", paste(names(figures), collapse = ", "), ")")
  for (f in wanted) figures[[f]]()
}
