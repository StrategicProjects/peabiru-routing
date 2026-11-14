## ----include = FALSE----------------------------------------------------------
knitr::opts_chunk$set(collapse = TRUE, comment = "#>")

## -----------------------------------------------------------------------------
library(peabiru)

## -----------------------------------------------------------------------------
data_path <- system.file("extdata/spo", package = "peabiru")
list.files(data_path)

network <- build_network(data_path, n_threads = 2)
network

## -----------------------------------------------------------------------------
points <- read.csv(file.path(data_path, "spo_points.csv"))
head(points)

## -----------------------------------------------------------------------------
walk <- travel_time_matrix(network, points, points, mode = "WALK",
                           n_threads = 2)
head(walk)

bike <- travel_time_matrix(network, points, points, mode = "BICYCLE",
                           n_threads = 2)
car <- travel_time_matrix(network, points, points, mode = "CAR",
                          n_threads = 2)
sapply(list(walk = walk, bike = bike, car = car),
       function(m) mean(m$travel_time_p50))

## -----------------------------------------------------------------------------
departure <- as.POSIXct("2019-05-13 14:00:00", tz = "America/Sao_Paulo")

ttm <- travel_time_matrix(
  network, points, points,
  mode = c("WALK", "TRANSIT"),
  departure_datetime = departure,
  time_window = 10,
  percentiles = c(25, 50, 75),
  seed = 42,
  n_threads = 2
)
head(ttm)

## ----fig.width = 5, fig.height = 4.2, fig.alt = "Scatter plot of the median travel time by walking and public transport against the walking time, for every pair of points: all points are on or below the diagonal."----
both <- merge(walk, ttm, by = c("from_id", "to_id"),
              suffixes = c("_walk", "_transit"))
mean(both$travel_time_p50_transit < both$travel_time_p50_walk)

plot(both$travel_time_p50_walk, both$travel_time_p50_transit,
     xlab = "walking (min)", ylab = "walk + transit, median (min)",
     pch = 16, col = "#b4532a80", asp = 1)
abline(0, 1, col = "grey50")

## -----------------------------------------------------------------------------
access <- accessibility(
  network, points, points,
  opportunities_colnames = "jobs",
  mode = c("WALK", "TRANSIT"),
  departure_datetime = departure,
  decay_function = "step",
  cutoffs = c(15, 30),
  seed = 42,
  n_threads = 2
)
head(access)

## -----------------------------------------------------------------------------
trips <- expanded_travel_time_matrix(
  network, points[1, ], points,
  mode = c("WALK", "TRANSIT"),
  departure_datetime = departure,
  time_window = 2,
  breakdown = TRUE,
  seed = 42,
  n_threads = 2
)
subset(trips, to_id == "89a8100c02fffff")

## -----------------------------------------------------------------------------
out <- tempfile("peabiru-")
dir.create(out)
travel_time_matrix(network, points, points, mode = "CAR", n_threads = 2,
                   output_dir = out) == out
length(list.files(out))
head(read.csv(file.path(out, list.files(out)[1])), 3)

## ----error = TRUE-------------------------------------------------------------
try({
fares <- setup_fare_structure(network, base_fare = 4.4)
fares$type <- "burlington"
travel_time_matrix(network, points, points, mode = c("WALK", "TRANSIT"),
                   departure_datetime = departure, fare_structure = fares)
})

