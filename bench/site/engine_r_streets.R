# Street travel-time matrices with dodgr and cppRouting, called by
# bench/site/run.py. Prints one JSON line
#   {"build_s", "times", "rows", "version", "notes", "origins_used", "threads_used"}
# and, with --result, writes the last timed result as
# from_id,to_id,travel_time (minutes, pairs reached within max_trip_duration).
#
# Rscript bench/site/engine_r_streets.R --engine dodgr|cppRouting --city poa
#   --dir DIR --points CSV --task ttm_walk --threads 1 --reps 3 --params JSON
#   [--origins N] [--result out.csv] [--osm highways.osm]
# Rscript bench/site/engine_r_streets.R --engine dodgr --probe
#
# The street network is what dodgr_streetnet_sc() would download from
# Overpass (every way with a highway tag, in silicate format), read from the
# city's .pbf instead: `osmium tags-filter <pbf> w/highway` to .osm XML (done
# once by run.py and passed with --osm), then osmdata::osmdata_sc(doc = <file>).
# dodgr::weight_streetnet() weights it with dodgr's profile ("foot",
# "bicycle", "motorcar"), with the walking and cycling speeds of the profile
# replaced by the benchmark's (3.6 and 12 km/h; dodgr's own are 5 and 15,
# 2 and 4 on steps and foot ways); the car keeps dodgr's speeds per road class and
# maxspeed. The largest connected component is kept, as dodgr's documentation
# recommends.
#
#   dodgr       dodgr_times(graph, from = <lon/lat>, to = <lon/lat>): fastest
#               routes by dodgr's weighted time, travel time summed along them
#   cppRouting  the same weighted dodgr graph as a cppRouting graph (cost =
#               dodgr's weighted time, aux = travel time), contracted with
#               cpp_contract(); get_distance_matrix(algorithm = "mch",
#               aggregate_aux = TRUE). Points snap with dodgr's
#               match_points_to_verts().
#
# Both are multithreaded through RcppParallel::setThreadOptions(). Snapping
# is inside the timed call. The build time covers osmdata_sc, weight_streetnet
# and, for cppRouting, makegraph and the contraction (and osmium when --osm is
# not given).

args <- commandArgs(trailingOnly = TRUE)
opt <- function(name, default = NULL) {
  i <- match(paste0("--", name), args)
  if (is.na(i)) {
    if (!is.null(default)) return(default)
    stop("missing --", name)
  }
  args[[i + 1]]
}
emit <- function(x) cat(jsonlite::toJSON(x, auto_unbox = TRUE, digits = NA), "\n")
engine <- opt("engine")
if (!engine %in% c("dodgr", "cppRouting")) stop("unknown engine ", engine)
need <- c("dodgr", "osmdata", "jsonlite", if (engine == "cppRouting") "cppRouting")
miss <- need[!vapply(need, requireNamespace, logical(1), quietly = TRUE)]
if (length(miss)) {
  cat("R packages not installed:", paste(miss, collapse = ", "), "\n")
  quit(status = 3)
}
pv <- function(p) as.character(utils::packageVersion(p))
version <- if (engine == "dodgr") {
  sprintf("dodgr %s (osmdata %s)", pv("dodgr"), pv("osmdata"))
} else {
  sprintf("cppRouting %s (dodgr %s, osmdata %s)", pv("cppRouting"), pv("dodgr"), pv("osmdata"))
}
if ("--probe" %in% args) {
  cat(version, "\n")
  quit(status = 0)
}

task <- opt("task")
threads <- as.integer(opt("threads"))
reps <- as.integer(opt("reps"))
P <- jsonlite::fromJSON(opt("params"))
profile <- c(ttm_walk = "foot", ttm_bicycle = "bicycle", ttm_car = "motorcar")[task]
if (is.na(profile)) {
  emit(list(skip = sprintf("%s routes on street graphs only; %s needs a public transport model",
                           engine, task)))
  quit(status = 0)
}
points <- utils::read.csv(opt("points"), colClasses = c(id = "character"))
n_orig <- as.integer(opt("origins", "0"))
if (is.na(n_orig) || n_orig <= 0) n_orig <- nrow(points)
orig <- points[seq_len(n_orig), ]
limit_s <- P$max_trip_duration * 60
RcppParallel::setThreadOptions(numThreads = threads)
dodgr::dodgr_cache_off()
notes <- character()

elapsed <- function(expr) {
  t0 <- proc.time()[["elapsed"]]
  force(expr)
  proc.time()[["elapsed"]] - t0
}

# ---------------------------------------------------------------- build
# --osm: the ways with a highway tag as .osm XML (run.py makes it once with
# osmium, untimed); without it, the conversion runs here, inside the build
osm <- opt("osm", "")
convert <- !nzchar(osm)
if (convert) {
  if (!nzchar(Sys.which("osmium"))) {
    cat("osmium (osmium-tool) is not on the PATH: it converts the .pbf for osmdata\n")
    quit(status = 3)
  }
  osm <- tempfile(fileext = ".osm")
}
pbf <- list.files(opt("dir"), pattern = "\\.pbf$", full.names = TRUE)[1]
wt_file <- tempfile(fileext = ".json")
build_s <- elapsed({
  if (convert) {
    rc <- system2("osmium", c("tags-filter", shQuote(pbf), "w/highway", "-o", shQuote(osm),
                              "-f", "osm", "--overwrite"), stdout = FALSE, stderr = FALSE)
    if (rc != 0) stop("osmium tags-filter failed")
  }
  sc <- osmdata::osmdata_sc(doc = osm)
  # dodgr's profiles with the benchmark's walking and cycling speeds
  dodgr::write_dodgr_wt_profile(sub("\\.json$", "", wt_file))
  wp <- jsonlite::read_json(wt_file, simplifyVector = TRUE)
  w <- wp$weighting_profiles
  if (profile == "foot") w$max_speed[w$name == "foot" & !is.na(w$max_speed)] <- P$walk_speed
  if (profile == "bicycle") {
    w$max_speed[w$name == "bicycle" & !is.na(w$max_speed)] <- P$bike_speed
  }
  wp$weighting_profiles <- w
  jsonlite::write_json(wp, wt_file, digits = NA, auto_unbox = TRUE, na = "null")
  graph <- dodgr::weight_streetnet(sc, wt_profile = profile, wt_profile_file = wt_file)
  graph <- graph[graph$component == 1, ]
  if (engine == "cppRouting") {
    verts <- dodgr::dodgr_vertices(graph)
    cg <- cppRouting::makegraph(
      data.frame(from = graph$.vx0, to = graph$.vx1, cost = graph$time_weighted),
      directed = TRUE, aux = graph$time)
    cg <- cppRouting::cpp_contract(cg, silent = TRUE)
  }
})
if (convert) unlink(osm)
notes <- c(notes, sprintf(
  "graph: %s directed edges, largest component of dodgr's %s profile; points snap to the nearest vertex",
  format(nrow(graph), big.mark = ","), profile))
if (profile == "foot") {
  notes <- c(notes, sprintf("every foot speed of dodgr's profile (5 km/h, 2 on steps) set to %g km/h",
                            P$walk_speed))
}
if (profile == "bicycle") {
  notes <- c(notes, sprintf("every bicycle speed of dodgr's profile (4 to 15 km/h by way type) set to %g km/h; no traffic-stress (LTS) limit",
                            P$bike_speed))
}
if (profile == "motorcar") {
  notes <- c(notes, "car speeds are dodgr's (maxspeed, else per road class); turn_penalty = FALSE (dodgr's default): no turn restrictions")
}
notes <- c(notes, "routes follow dodgr's weighted time (road-type preferences), times are the unweighted travel time along them; no time limit in the query (times above the limit are dropped afterwards)")

oxy <- as.matrix(orig[, c("lon", "lat")])
dxy <- as.matrix(points[, c("lon", "lat")])

route <- if (engine == "dodgr") {
  function() dodgr::dodgr_times(graph, from = oxy, to = dxy)
} else {
  function() {
    oi <- dodgr::match_points_to_verts(verts, oxy)
    di <- dodgr::match_points_to_verts(verts, dxy)
    o <- verts$id[oi]
    d <- verts$id[di]
    uo <- unique(o)
    ud <- unique(d)
    m <- cppRouting::get_distance_matrix(cg, from = uo, to = ud, algorithm = "mch",
                                         aggregate_aux = TRUE)
    m[match(o, rownames(m)), match(d, colnames(m)), drop = FALSE]
  }
}

res <- route() # warm-up
times <- numeric(reps)
for (i in seq_len(reps)) times[i] <- elapsed(res <- route())
res <- unname(as.matrix(res))
hit <- which(!is.na(res) & res <= limit_s, arr.ind = TRUE)
res_file <- opt("result", "")
if (nzchar(res_file)) {
  utils::write.csv(data.frame(from_id = orig$id[hit[, 1]], to_id = points$id[hit[, 2]],
                              travel_time = round(res[hit] / 60, 3)),
                   res_file, row.names = FALSE, quote = FALSE)
}
emit(list(build_s = build_s, times = I(times), rows = nrow(hit), version = version,
          notes = I(notes), origins_used = n_orig, threads_used = threads))
