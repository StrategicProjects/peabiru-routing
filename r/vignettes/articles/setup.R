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
