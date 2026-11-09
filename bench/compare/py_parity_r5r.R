# r5r reference results for bench/compare/py_parity.py.
#
# Usage: Rscript bench/compare/py_parity_r5r.R <scenarios.json> <out_dir>
#
# For each scenario of the JSON file (written by py_parity.py) this script
# builds the origin and destination points from r5r's hexgrid of the city,
# writes them to <out_dir>/<city>__<scenario>__{origins,destinations}.csv
# with every double printed with 17 significant digits (so that Python
# routes exactly the doubles R parsed), runs the r5r function and writes the
# result to <out_dir>/<city>__<scenario>__r5r.csv (doubles with 17
# significant digits, NA as "NA") and the R type of each column to
# <out_dir>/<city>__<scenario>__types.csv. A failed r5r call writes
# <out_dir>/<city>__<scenario>__error.txt instead.
#
# Needs a JDK 21 (JAVA_HOME) for r5r; R5R_XMX sets the JVM heap (default 6G).

xmx <- Sys.getenv("R5R_XMX", "6G")
options(java.parameters = paste0("-Xmx", xmx))
suppressPackageStartupMessages({
  library(r5r)
  library(jsonlite)
})

args <- commandArgs(trailingOnly = TRUE)
stopifnot(length(args) == 2)
spec <- fromJSON(args[1], simplifyVector = TRUE, simplifyDataFrame = FALSE,
                 simplifyMatrix = FALSE)
out_dir <- args[2]
dir.create(out_dir, showWarnings = FALSE, recursive = TRUE)
n_threads <- as.integer(spec$n_threads)

fmt_col <- function(x) {
  if (is.double(x)) {
    ifelse(is.na(x), "NA", sprintf("%.17g", x))
  } else {
    ifelse(is.na(x), "NA", as.character(x))
  }
}

write_exact <- function(df, path) {
  out <- as.data.frame(lapply(df, fmt_col), stringsAsFactors = FALSE,
                       optional = TRUE)
  names(out) <- names(df)
  write.table(out, path, sep = ",", quote = TRUE, row.names = FALSE,
              qmethod = "double", na = "NA")
}

# Points of a scenario: pieces of the hexgrid (1-based rows) and special
# points, optionally with integer ids.
make_points <- function(points, pieces, int_ids) {
  parts <- lapply(pieces, function(p) {
    if (!is.null(p$rows)) {
      points[p$rows[1]:p$rows[2], ]
    } else if (!is.null(p$index)) {
      points[p$index, ]
    } else if (identical(p$special, "na_point")) {
      transform(points[1, ], id = "na_point", lon = NA_real_)
    } else if (identical(p$special, "far_point")) {
      transform(points[1, ], id = "far_point", lon = 0, lat = 0)
    } else {
      stop("unknown point piece")
    }
  })
  df <- do.call(rbind, parts)
  rownames(df) <- NULL
  if (isTRUE(int_ids)) df$id <- seq_len(nrow(df))
  df
}

to_r <- function(name, v) {
  if (is.list(v) && !is.null(v$datetime)) {
    return(as.POSIXct(v$datetime, tz = v$tz))
  }
  if (is.character(v) && length(v) == 1 && v == "Inf") return(Inf)
  v
}

for (city in names(spec$cities)) {
  cs <- spec$cities[[city]]
  points <- read.csv(cs$hexgrid, colClasses = c(id = "character"))
  nets <- list(
    street = suppressMessages(r5r::build_network(cs$street)),
    transit = suppressMessages(r5r::build_network(cs$transit))
  )
  for (s in spec$scenarios) {
    stem <- file.path(out_dir, paste0(city, "__", s$name, "__"))
    o <- make_points(points, s$origins, s$int_ids)
    d <- make_points(points, s$destinations, s$int_ids)
    write_exact(o, paste0(stem, "origins.csv"))
    write_exact(d, paste0(stem, "destinations.csv"))
    a <- lapply(names(s$args), function(k) to_r(k, s$args[[k]]))
    names(a) <- names(s$args)
    call_args <- c(list(nets[[s$network]], origins = o, destinations = d), a,
                   list(n_threads = n_threads, progress = FALSE))
    t0 <- proc.time()[["elapsed"]]
    res <- tryCatch(
      suppressWarnings(as.data.frame(do.call(getExportedValue("r5r", s$fun), call_args))),
      error = function(e) e
    )
    secs <- proc.time()[["elapsed"]] - t0
    if (inherits(res, "error")) {
      writeLines(conditionMessage(res), paste0(stem, "error.txt"))
      message(sprintf("%s/%s: r5r error: %s", city, s$name, conditionMessage(res)))
      next
    }
    write_exact(res, paste0(stem, "r5r.csv"))
    write.csv(data.frame(column = names(res), type = vapply(res, typeof, "")),
              paste0(stem, "types.csv"), row.names = FALSE)
    message(sprintf("%s/%s: %d rows in %.2f s", city, s$name, nrow(res), secs))
  }
  rm(nets)
  r5r::stop_r5()
  gc()
}
message("done")
