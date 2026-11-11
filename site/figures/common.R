# Shared by site/figures/figures.R: the sample data, the brand palette, a
# ggplot2 theme, and save_svg(), which writes an SVG that switches to the dark
# palette by itself (as the diagrams of site/assets/diagrams do).

suppressPackageStartupMessages({
  library(peabiru)
  library(sf)
  library(ggplot2)
})

# ------------------------------------------------------------------ data

# Porto Alegre: the sample data of r5r 2.5.0 (OpenStreetMap extract, the
# EPTC bus and Trensurb train feeds, a hexagonal grid with population, jobs,
# schools and health care). Downloaded once into a cache directory.
poa_files <- c("poa_osm.pbf", "poa_eptc.zip", "poa_trensurb.zip",
               "poa_hexgrid.csv", "poa_points_of_interest.csv")
poa_url <- "https://raw.githubusercontent.com/ipea/r5r/v2.5.0/r-package/inst/extdata/poa/"

poa_dir <- function() {
  dir <- Sys.getenv("PEABIRU_FIGURES_CACHE",
                    file.path(tools::R_user_dir("peabiru", "cache"), "poa-r5r-2.5.0"))
  dir.create(dir, recursive = TRUE, showWarnings = FALSE)
  for (f in poa_files) {
    dest <- file.path(dir, f)
    if (!file.exists(dest)) {
      message("downloading ", f)
      utils::download.file(paste0(poa_url, f), dest, mode = "wb", quiet = TRUE)
    }
  }
  dir
}

# São Paulo: the sample network of the R package (frequency-based feed).
spo_dir <- function() system.file("extdata/spo", package = "peabiru")

# Streets and water of an extract, for the background of the maps.
# (drawn widths, mm; only the main streets: every street would weigh megabytes)
street_classes <- c(motorway = 0.7, trunk = 0.7, primary = 0.5, secondary = 0.38,
                    tertiary = 0.25, motorway_link = 0.3, trunk_link = 0.3,
                    primary_link = 0.3)

background <- function(pbf, bbox = NULL) {
  lines <- suppressWarnings(st_read(pbf, "lines", quiet = TRUE))
  lines <- lines[!is.na(lines$highway) & lines$highway %in% names(street_classes), ]
  lines$w <- unname(street_classes[lines$highway])
  polys <- suppressWarnings(st_read(pbf, "multipolygons", quiet = TRUE))
  water <- polys[(!is.na(polys$natural) & polys$natural == "water"), ]
  lines <- st_transform(lines, 3857)
  water <- st_transform(st_make_valid(water), 3857)
  # thin the geometry: the maps are drawn at about 3 m per pixel
  lines <- st_simplify(lines, dTolerance = 20)
  lines <- lines[!st_is_empty(lines), ]
  lines <- lines[order(lines$w), ]   # one <path> per width in the SVG
  list(lines = lines[, "w"], water = st_geometry(water))
}

# Regular hexagons around the centres of an H3 grid (all cells of a small
# area share one orientation), so the grid can be drawn without an H3 library.
hexagons <- function(points) {
  p <- st_transform(st_as_sf(points, coords = c("lon", "lat"), crs = 4326), 3857)
  xy <- st_coordinates(p)
  d <- as.matrix(dist(xy))
  diag(d) <- Inf
  nn <- apply(d, 1, which.min)
  spacing <- stats::median(d[cbind(seq_len(nrow(xy)), nn)])
  ang <- atan2(xy[nn, 2] - xy[, 2], xy[nn, 1] - xy[, 1]) %% (pi / 3)
  theta <- stats::median(ang) + pi / 6           # vertices between neighbours
  r <- spacing / sqrt(3) * 1.002                 # circumradius, no hairline gaps
  k <- theta + (0:6) * pi / 3
  cells <- lapply(seq_len(nrow(xy)), function(i) {
    st_polygon(list(cbind(xy[i, 1] + r * cos(k), xy[i, 2] + r * sin(k))))
  })
  st_sf(points, geometry = st_sfc(cells, crs = 3857))
}

# ------------------------------------------------------------------ palette

# site/assets/brand.css; each light colour has a dark counterpart, applied by
# save_svg() when the reader's system is in dark mode.
pal <- list(
  panel = c("#FFFDF9", "#1D2427"),
  border = c("#E1D8C8", "#34403F"),
  ink = c("#1F2A2E", "#ECE6DA"),
  soft = c("#4A5A5F", "#A9B4B6"),
  grid = c("#ECE4D6", "#2C3538"),
  street = c("#DCD3C4", "#36423F"),
  water = c("#DCE6E6", "#20343A"),
  route = c("#B4532A", "#E98A5C"),
  stop = c("#C98A1D", "#F0B54A"),
  forest = c("#2F6B4F", "#6FBF95"),
  forest2 = c("#8DB9A0", "#3E7C5E"),
  slate = c("#3D4F56", "#C6D0D2")
)
light <- function(name) pal[[name]][[1]]

# Sequential ramps: travel time (terracotta, near = strongest) and
# opportunities (forest, more = strongest). In dark mode the strongest class
# is the brightest.
ramp_time <- list(
  light = c("#7A3318", "#A84B25", "#CF7445", "#E8A57B", "#F5D3B6"),
  dark = c("#F7C9A6", "#EE9C6E", "#D46F3D", "#A2502A", "#6A3820")
)
ramp_access <- list(
  light = c("#F1F0E2", "#CFE0CB", "#9DC6A8", "#64A07E", "#3A7B5A", "#1F5139"),
  dark = c("#283331", "#2F4D40", "#3B6F55", "#4E9470", "#73BC94", "#B5E2C6")
)
# Modes: one colour each, the same in every chart.
mode_colour <- c(walk = "forest", bicycle = "stop", car = "slate",
                 transit = "route")

# ------------------------------------------------------------------ theme

theme_peabiru <- function(base_size = 11, map = FALSE) {
  t <- theme_minimal(base_size = base_size, base_family = "Helvetica Neue") +
    theme(
      plot.background = element_rect(fill = light("panel"), colour = NA),
      panel.background = element_rect(fill = NA, colour = NA),
      text = element_text(colour = light("ink")),
      axis.text = element_text(colour = light("soft")),
      plot.title = element_text(face = "bold", size = base_size + 2),
      plot.subtitle = element_text(colour = light("soft")),
      plot.caption = element_text(colour = light("soft"), size = base_size - 2, hjust = 0),
      plot.title.position = "plot", plot.caption.position = "plot",
      strip.text = element_text(face = "bold", hjust = 0, size = base_size,
                                colour = light("ink")),
      panel.grid.major = element_line(colour = light("grid"), linewidth = 0.4),
      panel.grid.minor = element_blank(),
      legend.text = element_text(colour = light("ink")),
      legend.title = element_text(colour = light("soft")),
      plot.margin = margin(14, 16, 12, 16)
    )
  if (map) {
    t <- t + theme(axis.text = element_blank(), axis.title = element_blank(),
                   panel.grid.major = element_blank(),
                   panel.border = element_rect(fill = NA, colour = light("border"),
                                               linewidth = 0.4))
  }
  t
}

# ------------------------------------------------------------------ output

out_dir <- function() {
  d <- Sys.getenv("PEABIRU_FIGURES_OUT", "")
  if (d == "") d <- file.path(root_dir, "site", "assets", "figures")
  dir.create(d, recursive = TRUE, showWarnings = FALSE)
  d
}

# Light -> dark substitutions: every colour of `pal` and of the ramps.
dark_map <- function() {
  m <- c(setNames(vapply(pal, `[[`, "", 2), vapply(pal, `[[`, "", 1)),
         setNames(ramp_time$dark, ramp_time$light),
         setNames(ramp_access$dark, ramp_access$light))
  m[!duplicated(names(m))]
}

# svglite writes one <polyline> per street segment, each with its style;
# consecutive ones with the same style become one <path> (a tenth of the
# bytes). Lines are never filled, so joining them changes nothing drawn.
# "M x y L x y L x y Z" (or "M x,y x,y") with absolute coordinates becomes
# "Mx y l dx dy dx dy z": shorter numbers, and every hexagon of a grid the
# same string after its first point.
relative_path <- function(d) {
  subs <- regmatches(d, gregexpr("M[^M]*", d))[[1]]
  vapply(subs, function(s) {
    closed <- grepl("Z", s, fixed = TRUE)
    v <- as.numeric(strsplit(trimws(gsub("[MLZ,]", " ", s)), " +")[[1]])
    xy <- matrix(round(v * 10), ncol = 2, byrow = TRUE)   # tenths, exact deltas
    if (nrow(xy) < 2) return(s)
    dxy <- diff(xy)
    keep <- rowSums(abs(dxy)) > 0
    dxy <- dxy[keep, , drop = FALSE]
    num <- function(x) sub("^(-?)0\\.", "\\1.", as.character(x / 10))
    rel <- paste(num(as.vector(t(dxy))), collapse = " ")
    rel <- gsub(" -", "-", rel, fixed = TRUE)
    paste0("M", num(xy[1, 1]), " ", num(xy[1, 2]), if (nrow(dxy)) paste0("l", rel),
           if (closed) "z")
  }, "") |> paste0(collapse = "")
}

# Filled paths without a stroke merge too: the polygons of one isochrone band,
# or the hexagons of one class, never overlap.
merge_polylines <- function(lines) {
  re <- "^<polyline points='([^']*)' style='([^']*)' />$"
  re_fill <- "^<path d='([^']*)' style='([^']*fill: #[^']*stroke: none;[^']*)' />$"
  is_line <- grepl(re, lines)
  is_fill <- grepl(re_fill, lines)
  is_pl <- is_line | is_fill
  pts <- ifelse(is_line, paste0("M", trimws(sub(re, "\\1", lines))),
                gsub(" ?([MLZ]) ?", "\\1", sub(re_fill, "\\1", lines)))
  # lines carry "fill: none" themselves: svglite sets it with a ".svglite path"
  # rule, which does not reach the copies made by <use> (see reuse_paths())
  sty <- ifelse(is_line, paste0("l ", sub(re, "\\2", lines), " fill: none;"),
                ifelse(is_fill, paste("f", sub(re_fill, "\\2", lines)), NA_character_))
  out <- character()
  i <- 1L
  n <- length(lines)
  while (i <= n) {
    if (!is_pl[i]) {
      out <- c(out, lines[i])
      i <- i + 1L
      next
    }
    j <- i
    while (j < n && is_pl[j + 1L] && identical(sty[j + 1L], sty[i])) j <- j + 1L
    d <- paste0(pts[i:j], collapse = "")
    out <- c(out, sprintf("<path d='%s' style='%s' />", d, substring(sty[i], 3)))
    i <- j + 1L
  }
  out <- reuse_paths(out)
  re_d <- "^(<path (id='[^']*' )?d=')(M[-0-9. ,MLZ]*)(' .*)$"
  hit <- grepl(re_d, out)
  out[hit] <- vapply(out[hit], function(l) {
    paste0(sub(re_d, "\\1", l), relative_path(sub(re_d, "\\3", l)), sub(re_d, "\\4", l))
  }, "", USE.NAMES = FALSE)
  out
}

# The panels of a faceted map draw the same streets, shifted: a path that
# repeats an earlier one (same style, same shape) becomes a <use> of it.
reuse_paths <- function(lines) {
  re <- "^<path d='(M[^']*)' style='([^']*)' />$"
  seen <- list()
  for (i in grep(re, lines)) {
    d <- sub(re, "\\1", lines[i])
    if (nchar(d) < 2000) next
    sty <- sub(re, "\\2", lines[i])
    xy <- matrix(as.numeric(strsplit(trimws(gsub("[MLZ,]", " ", d)), " +")[[1]]),
                 ncol = 2, byrow = TRUE)
    hit <- 0L
    for (k in seq_along(seen)) {
      s <- seen[[k]]
      if (s$sty == sty && nrow(s$xy) == nrow(xy)) {
        off <- xy[1, ] - s$xy[1, ]
        if (max(abs(sweep(xy, 2, off) - s$xy)) < 0.25) { hit <- k; break }
      }
    }
    if (hit) {
      off <- xy[1, ] - seen[[hit]]$xy[1, ]
      lines[i] <- sprintf("<use href='#pb-p%d' transform='translate(%.1f %.1f)' />",
                          hit, off[1], off[2])
    } else {
      seen[[length(seen) + 1L]] <- list(sty = sty, xy = xy)
      lines[i] <- sprintf("<path id='pb-p%d' d='%s' style='%s' />", length(seen), d, sty)
    }
  }
  lines
}

# Write `plot` as SVG, then add: a title and description (alt text) for
# screen readers, the font stack of the brand, a rounded panel, and a
# stylesheet that swaps every brand colour for its dark counterpart under
# prefers-color-scheme: dark (attribute selectors on svglite's inline styles,
# with !important, which beats an inline style).
save_svg <- function(plot, name, width, height, title, desc) {
  path <- file.path(out_dir(), paste0(name, ".svg"))
  svglite::svglite(path, width = width, height = height, bg = light("panel"),
                   fix_text_size = FALSE)
  print(plot)
  grDevices::dev.off()
  svg <- paste(readLines(path, warn = FALSE), collapse = "\n")

  m <- dark_map()
  rules <- character()
  for (prop in c("fill", "stroke")) {
    for (col in names(m)) {
      if (grepl(paste0(prop, ": ", col, ";"), svg, fixed = TRUE)) {
        rules <- c(rules, sprintf("[style*=\"%s: %s;\"] { %s: %s !important; }",
                                  prop, col, prop, m[[col]]))
      }
    }
  }
  style <- paste0(
    "<style>\n",
    "text { font-family: 'Helvetica Neue', Helvetica, Arial, sans-serif !important; }\n",
    "@media (prefers-color-scheme: dark) {\n",
    paste(rules, collapse = "\n"), "\n}\n</style>")
  esc <- function(x) gsub("<", "&lt;", gsub("&", "&amp;", x, fixed = TRUE), fixed = TRUE)
  head <- sprintf('<title id="title">%s</title>\n<desc id="desc">%s</desc>\n%s',
                  esc(title), esc(desc), style)
  # the root element: role and labels; then title, description and style
  svg <- sub("<svg ", '<svg role="img" aria-labelledby="title desc" ', svg, fixed = TRUE)
  svg <- sub("(<svg [^>]*>)", paste0("\\1\n", gsub("\\\\", "\\\\\\\\", head)), svg)
  # svglite's background rectangle becomes the rounded panel of the diagrams
  vb <- as.numeric(strsplit(sub(".*viewBox='([^']*)'.*", "\\1",
                                regmatches(svg, regexpr("viewBox='[^']*'", svg))), " ")[[1]])
  svg <- sub("<rect width='100%' height='100%' style='stroke: none; fill: #FFFDF9;'/>",
             sprintf(paste0("<rect x='0.5' y='0.5' width='%.1f' height='%.1f' rx='9' style='fill: ",
                            light("panel"), "; stroke: ", light("border"), "; stroke-width: 1;'/>"),
                     vb[3] - 1, vb[4] - 1),
             svg, fixed = TRUE)
  # fewer bytes: svglite writes two decimals everywhere
  svg <- gsub("([0-9]+\\.[0-9])[0-9]\\b", "\\1", svg, perl = TRUE)
  writeLines(merge_polylines(strsplit(svg, "\n", fixed = TRUE)[[1]]), path)
  message(sprintf("%-28s %6.1f kB", basename(path), file.size(path) / 1024))
  invisible(path)
}
