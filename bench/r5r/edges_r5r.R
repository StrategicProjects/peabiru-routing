# Export the edges of an r5r street network with their attributes, for
# comparison with `peabiru dump-edges` (bench/compare/edges.py).
#
# Usage (JAVA_HOME must point to a JDK 21):
#   Rscript bench/r5r/edges_r5r.R <network_dir> <out.csv>
options(java.parameters = paste0("-Xmx", Sys.getenv("R5R_XMX", "8G")))
suppressPackageStartupMessages({
  library(r5r)
  library(data.table)
  library(sf)
})
a <- commandArgs(TRUE)
net <- build_network(a[1], verbose = FALSE)
e <- street_network_to_sf(net)$edges
ends <- as.data.table(st_coordinates(e))[, .(lon0 = X[1], lat0 = Y[1], lon1 = X[.N], lat1 = Y[.N]), by = L1]
fwrite(cbind(as.data.table(st_drop_geometry(e)), ends[, -1]), a[2])
r5r::stop_r5(net)
