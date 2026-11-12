# The small network shipped with the package: a crop of r5r's Sao Paulo
# sample (streets around Avenida Paulista and the SPTrans frequency-based
# GTFS), with 27 points.
fixture_path <- function() system.file("extdata/spo", package = "peabiru")

fixture_network <- local({
  network <- NULL
  function() {
    if (is.null(network)) network <<- build_network(fixture_path(), n_threads = 2)
    network
  }
})

fixture_points <- function() {
  read.csv(file.path(fixture_path(), "spo_points.csv"),
           colClasses = c(id = "character"))
}

# options(peabiru.output = kind) until the calling test (or function) ends.
local_output <- function(kind, frame = parent.frame()) {
  old <- options(peabiru.output = kind)
  do.call(on.exit, list(bquote(options(.(old))), add = TRUE, after = FALSE), envir = frame)
  invisible(old)
}

departure <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")

# sf::sf_use_s2(on) until the calling test (or function) ends.
local_s2 <- function(on, frame = parent.frame()) {
  old <- suppressMessages(sf::sf_use_s2(on))
  do.call(on.exit, list(bquote(suppressMessages(sf::sf_use_s2(.(old)))), add = TRUE, after = FALSE),
          envir = frame)
  invisible(old)
}

# options(peabiru.isochrone_repair = how) until the calling test (or function) ends.
local_isochrone_repair <- function(how, frame = parent.frame()) {
  old <- options(peabiru.isochrone_repair = how)
  do.call(on.exit, list(bquote(options(.(old))), add = TRUE, after = FALSE), envir = frame)
  invisible(old)
}
