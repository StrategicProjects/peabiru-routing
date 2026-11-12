# Shared by the pkgdown articles: knitr options, the sample network, and
# fig(), which shows a diagram, chart or figure of the documentation site
# (figures: site/assets/figures, drawn by site/figures/figures.R).
#
# The SVG files live once, in site/assets/ at the root of the repository, and
# are shared by every documentation site. pkgdown only publishes images that
# sit below the article, so fig() copies the file into figures/ (ignored by
# git) before pointing knitr at it.

knitr::opts_chunk$set(collapse = TRUE, comment = "#>")

fig_path <- function(name) {
  site <- normalizePath(file.path("..", "..", "..", "site", "assets"), mustWork = FALSE)
  src <- file.path(site, c("diagrams", "charts", "figures"), name)
  src <- src[file.exists(src)]
  if (length(src) == 0) stop("no diagram, chart or figure named ", name, " under ", site)
  dir.create("figures", showWarnings = FALSE)
  file.copy(src[[1]], file.path("figures", name), overwrite = TRUE)
  file.path("figures", name)
}
fig <- function(name) knitr::include_graphics(fig_path(name))

library(peabiru)
data_path <- system.file("extdata/spo", package = "peabiru")
network <- build_network(data_path, n_threads = 2)
points <- read.csv(file.path(data_path, "spo_points.csv"))
departure <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")

# r5r_sample(files): files of the Porto Alegre sample of r5r 2.5.0, downloaded
# once into a temporary directory, which is returned. For the articles that
# need timetables (the bundled Sao Paulo feed is frequency-based), an
# elevation raster or r5r's scenario tables.
r5r_sample <- function(files = c("poa_osm.pbf", "poa_eptc.zip", "poa_trensurb.zip"),
                       dir = file.path(tempdir(), "poa")) {
  url <- "https://raw.githubusercontent.com/ipea/r5r/v2.5.0/r-package/inst/extdata/poa/"
  dir.create(dir, showWarnings = FALSE)
  for (f in files) {
    if (!file.exists(file.path(dir, basename(f)))) {
      download.file(paste0(url, f), file.path(dir, basename(f)), mode = "wb", quiet = TRUE)
    }
  }
  dir
}
