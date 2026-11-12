# Scenarios: r5r's set_new_congestion() and set_new_lts(), with the checks of
# congestion_poly2geojson(), dt_to_speed_map(), lts_lines2shp() and
# dt_to_lts_map(). The result is the list the engine reads
# (src/rust/src/scenario.rs): the data frame columns as text, as r5r hands
# them to R5 with as.character(), and the sf geometries flattened.

# checkmate's name of the type of `x` in its messages
cm_type <- function(x) {
  if (is.null(x)) return("NULL")
  if (is.object(x)) return(class(x)[1L])
  switch(typeof(x), closure = "function", builtin = "function", typeof(x))
}

cm_fail <- function(name, msg) {
  stop("Assertion on '", name, "' failed: ", msg, ".", call. = FALSE)
}

# checkmate::assert_class(x, "data.frame", null.ok = TRUE)
cm_data_frame <- function(x, name) {
  if (!is.null(x) && !inherits(x, "data.frame")) {
    cl <- class(x)
    cm_fail(name, paste0("Must inherit from class 'data.frame', but has class",
                         if (length(cl) > 1L) "es" else "", " ",
                         paste0("'", cl, "'", collapse = ",")))
  }
}

# checkmate::assert_numeric(x, lower = 0, finite = TRUE, null.ok = FALSE),
# r5r's check of carspeed_scale; then a clear error for what that check lets
# through and r5r fails on with an internal R error (`carspeed_scale != 1`
# of NA, or of a length other than 1).
cm_scale <- function(x, name = "carspeed_scale") {
  # checkmate takes a logical vector of NAs as numeric
  if (!is.numeric(x) && !(is.logical(x) && all(is.na(x)))) {
    cm_fail(name, paste0("Must be of type 'numeric', not '", cm_type(x), "'"))
  }
  low <- which(x < 0)
  if (length(low)) cm_fail(name, paste0("Element ", low[1L], " is not >= 0"))
  if (any(is.infinite(x))) cm_fail(name, "Must be finite")
  if (length(x) != 1L || is.na(x)) {
    stop("'", name, "' must be a single number that is not NA (r5r 2.5.0 fails ",
         "on this value with an internal R error)", call. = FALSE)
  }
}

# checkmate::assert_names(x, must.include = must)
cm_names <- function(x, must, name) {
  missing <- setdiff(must, x)
  if (length(missing) > 0L) {
    q <- function(v) paste0("{", paste0("'", v, "'", collapse = ","), "}")
    cm_fail(name, paste0("Names must include the elements ", q(must),
                         ", but is missing elements ", q(missing)))
  }
}

# the missing-value checks of checkmate's assert_numeric(), assert_integer()
# and assert_character() with any.missing = FALSE (and all.missing = FALSE)
cm_missing <- function(x, name, all_missing = TRUE) {
  if (!all_missing && all(is.na(x))) cm_fail(name, "Contains only missing values")
  if (anyNA(x)) {
    cm_fail(name, paste0("Contains missing values (element ", which(is.na(x))[1L], ")"))
  }
}

cm_typed <- function(x, ok, type, name, all_missing = TRUE, lower = -Inf,
                     upper = Inf) {
  if (!ok) cm_fail(name, paste0("Must be of type '", type, "', not '", cm_type(x), "'"))
  cm_missing(x, name, all_missing)
  if (!is.numeric(x)) return(invisible(x))
  low <- which(x < lower)
  if (length(low)) cm_fail(name, paste0("Element ", low[1L], " is not >= ", lower))
  high <- which(x > upper)
  if (length(high)) cm_fail(name, paste0("Element ", high[1L], " is not <= ", upper))
}

# checkmate::assert_subset(types, choices, empty.ok = FALSE)
cm_geometry_types <- function(x, choices, name) {
  types <- unique(as.character(sf::st_geometry_type(x)))
  q <- function(v) paste0("{", paste0("'", v, "'", collapse = ","), "}")
  if (length(types) == 0L) {
    cm_fail(name, paste0("Must be a subset of ", q(choices), ", not empty"))
  }
  extra <- setdiff(types, choices)
  if (length(extra)) {
    cm_fail(name, paste0("Must be a subset of ", q(choices),
                         ", but has additional elements ", q(extra)))
  }
}


# The coordinates of an sfc as flat vectors: x, y, the points of each ring
# (or line), and, for polygons, the polygon of each ring and the feature of
# each polygon (0-based).
flatten_polygons <- function(geometry) {
  x <- y <- numeric(0)
  ring_len <- ring_polygon <- polygon_feature <- integer(0)
  n_poly <- 0L
  for (f in seq_along(geometry)) {
    g <- geometry[[f]]
    polys <- if (inherits(g, "MULTIPOLYGON")) unclass(g) else list(unclass(g))
    for (p in polys) {
      for (ring in p) {
        x <- c(x, ring[, 1])
        y <- c(y, ring[, 2])
        ring_len <- c(ring_len, nrow(ring))
        ring_polygon <- c(ring_polygon, n_poly)
      }
      polygon_feature <- c(polygon_feature, f - 1L)
      n_poly <- n_poly + 1L
    }
  }
  list(poly_x = x, poly_y = y, ring_len = as.integer(ring_len),
       ring_polygon = as.integer(ring_polygon),
       polygon_feature = as.integer(polygon_feature))
}

flatten_lines <- function(geometry) {
  m <- lapply(geometry, unclass)
  list(line_x = unlist(lapply(m, function(l) l[, 1])),
       line_y = unlist(lapply(m, function(l) l[, 2])),
       line_len = vapply(m, nrow, integer(1)))
}

# r5r's warning when a scenario is used: R5's scenario copy of the network
# has no slope costs (r5r issue #555). r5r gives it for every scenario; here
# only on a network with elevation, where it is true.
scenario_elevation_warning <- function(r5r_network) {
  if (!is.null(r5r_network) && !identical(r5r_network$elevation, "NONE")) {
    warning("A scenario was used for this calculation. This may affect results ",
            "for WALK or BICYCLE due to missing elevation. See issue #555.",
            call. = FALSE)
  }
}

# r5r set_new_congestion() and set_new_lts(): the scenario of a routing call,
# NULL when it changes nothing.
scenario_spec <- function(new_carspeeds, carspeed_scale, new_lts,
                          r5r_network = NULL) {
  cm_data_frame(new_carspeeds, "new_carspeeds")
  cm_scale(carspeed_scale)
  spec <- list(car = "none", carspeed_scale = as.double(carspeed_scale),
               lts = "none")
  changed <- FALSE
  if (!is.null(new_carspeeds) || carspeed_scale != 1) {
    changed <- TRUE
    scenario_elevation_warning(r5r_network)
    if (inherits(new_carspeeds, "sf")) {
      # congestion_poly2geojson()
      x <- new_carspeeds
      cm_names(names(x), c("poly_id", "scale", "priority", "geometry"),
               "names(new_speeds_poly)")
      cm_typed(x$poly_id, is.character(x$poly_id), "character",
               "new_speeds_poly$poly_id")
      cm_typed(x$scale, is.numeric(x$scale), "numeric", "new_speeds_poly$scale")
      cm_typed(x$priority, is.integer(x$priority), "integer",
               "new_speeds_poly$priority")
      cm_geometry_types(x, c("POLYGON", "MULTIPOLYGON"),
                        "unique(as.character(sf::st_geometry_type(new_speeds_poly)))")
      # r5r requires WGS 84; any other CRS is reprojected, with a message
      x <- as_wgs84(x, "new_carspeeds")
      # r5r writes the polygons to GeoJSON for R5, where GDAL keeps 15
      # decimals of each coordinate (and trims what looks like round-off):
      # the same round trip gives R5's coordinates
      geojson <- tempfile(fileext = ".geojson")
      on.exit(unlink(geojson), add = TRUE)
      sf::st_write(x, geojson, quiet = TRUE)
      written <- sf::st_read(geojson, quiet = TRUE)
      spec$car <- "polygon"
      spec <- c(spec, flatten_polygons(sf::st_geometry(written)),
                list(scale = as.double(x$scale), priority = as.double(x$priority)))
    } else if (!is.null(new_carspeeds)) {
      # dt_to_speed_map()
      dt <- new_carspeeds
      cm_names(names(dt), c("osm_id", "max_speed", "speed_type"), "names(dt)")
      cm_typed(dt$osm_id, is.numeric(dt$osm_id), "numeric", "dt$osm_id",
               all_missing = FALSE)
      cm_typed(dt$max_speed, is.numeric(dt$max_speed), "numeric", "dt$max_speed",
               all_missing = FALSE)
      if (!(length(unique(dt$speed_type)) == 1 &&
            dt$speed_type[1] %in% c("km/h", "scale"))) {
        cm_fail(paste0("length(unique(dt$speed_type)) == 1 && dt$speed_type[1] %in% ",
                       "c(\"km/h\", \"scale\")"), "Must be TRUE")
      }
      spec$car <- if (dt$speed_type[1] == "km/h") "km/h" else "scale"
      spec$car_ids <- as.character(dt$osm_id)
      spec$car_values <- as.character(dt$max_speed)
    }
  }
  # set_new_lts(), after set_new_congestion()
  cm_data_frame(new_lts, "new_lts")
  if (!is.null(new_lts)) {
    changed <- TRUE
    scenario_elevation_warning(r5r_network)
    if (inherits(new_lts, "sf")) {
      # lts_lines2shp()
      x <- new_lts
      cm_names(names(x), c("line_id", "lts", "priority", "geometry"),
               "names(new_lts_lines)")
      cm_typed(x$line_id, is.character(x$line_id), "character",
               "new_lts_lines$line_id")
      cm_typed(x$lts, is.integer(x$lts), "integer", "new_lts_lines$lts",
               lower = 1, upper = 4)
      cm_typed(x$priority, is.integer(x$priority), "integer",
               "new_lts_lines$priority")
      cm_geometry_types(x, "LINESTRING",
                        "unique(as.character(sf::st_geometry_type(new_lts_lines)))")
      x <- as_wgs84(x, "new_lts")
      spec$lts <- "line"
      spec <- c(spec, flatten_lines(sf::st_geometry(x)),
                list(line_lts = as.integer(x$lts)))
    } else {
      # dt_to_lts_map()
      dt <- new_lts
      cm_names(names(dt), c("osm_id", "lts"), "names(dt)")
      cm_typed(dt$osm_id, is.numeric(dt$osm_id), "numeric", "dt$osm_id",
               all_missing = FALSE)
      cm_typed(dt$lts, is.integer(dt$lts), "integer", "dt$lts", all_missing = FALSE)
      spec$lts <- "way"
      spec$lts_ids <- as.character(dt$osm_id)
      spec$lts_values <- as.character(dt$lts)
    }
  }
  if (changed) spec else NULL
}
