# The r5r side of the parity tests (test-parity.R, test-fares.R): an r5r and
# a peabiru network on r5r's Sao Paulo sample. Skipped on CRAN and wherever
# r5r cannot start (it needs a JDK 21 and the R5 jar).

r5r_network <- local({
  net <- NULL
  function() {
    if (!is.null(net)) return(net)
    skip_on_cran()
    skip_if_not_installed("r5r")
    src <- system.file("extdata/spo", package = "r5r")
    skip_if(!file.exists(file.path(src, "spo_osm.pbf")), "r5r sample data not found")
    dir <- file.path(tempdir(), "peabiru-parity-spo")
    unlink(dir, recursive = TRUE)
    dir.create(dir)
    file.copy(file.path(src, c("spo_osm.pbf", "spo.zip")), dir)
    if (is.null(getOption("java.parameters"))) options(java.parameters = "-Xmx2G")
    r5 <- tryCatch(
      suppressMessages(r5r::build_network(dir)),
      error = function(e) NULL
    )
    skip_if(is.null(r5), "r5r could not build a network (Java 21 or the R5 jar missing?)")
    pb <- build_network(dir, n_threads = 2)
    points <- read.csv(file.path(src, "spo_hexgrid.csv"), colClasses = c(id = "character"))
    net <<- list(r5 = r5, pb = pb, points = points)
    net
  }
})

expect_same_as_r5r <- function(fun, ...) {
  n <- r5r_network()
  pb <- getExportedValue("peabiru", fun)(n$pb, ..., n_threads = 2)
  r5 <- getExportedValue("r5r", fun)(n$r5, ..., n_threads = 2, progress = FALSE)
  expect_identical(pb, as.data.frame(r5))
}
