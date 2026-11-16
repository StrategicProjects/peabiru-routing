school_split <- function() {
  pts <- fixture_points()
  list(points = pts, schools = pts[pts$schools > 0, ], others = pts[pts$schools == 0, ])
}

test_that("max_cover returns its tables and the recorded solution", {
  s <- school_split()
  found <- locate_facilities(
    fixture_network(), s$points, s$others, existing = s$schools,
    model = "max_cover", n_facilities = 2, cutoff = 12,
    demand_colname = "population", mode = "WALK", n_threads = 2
  )
  expect_named(found, c("sites", "candidates", "demand", "summary"))
  expect_identical(class(found$sites), c("tbl_df", "tbl", "data.frame"))
  expect_named(found$sites, c("id", "lon", "lat", "order", "marginal", "cumulative"))
  expect_named(found$candidates, c("id", "lon", "lat", "potential", "chosen"))
  expect_named(found$demand, c("id", "lon", "lat", "demand", "facility_id",
                               "travel_time", "covered"))
  expect_identical(nrow(found$candidates), nrow(s$others))
  expect_identical(nrow(found$demand), nrow(s$points))
  # recorded: greedy covers 46040 residents, one swap takes it to 46217,
  # which an exhaustive search over the 210 pairs confirms as the optimum
  expect_identical(found$sites$id, c("89a8100c03bffff", "89a8100c383ffff"))
  expect_identical(found$sites$marginal, c(5658, 2683))
  expect_identical(found$sites$cumulative, c(43534, 46217))
  expect_identical(found$summary$covered_before, 37876)
  expect_identical(found$summary$objective_greedy, 46040)
  expect_identical(found$summary$objective, 46217)
  expect_identical(found$summary$n_swaps, 1L)
  expect_identical(found$summary$total_demand, 46273)
  expect_identical(found$candidates$potential[1:4], c(3936, 4829, 5658, 5764))
  expect_identical(found$candidates$id[found$candidates$chosen], sort(found$sites$id))
  expect_identical(sum(found$demand$demand[found$demand$covered]), 46217)

  # against the travel-time matrix: every pair of sites
  sites <- rbind(s$schools, s$others)
  ttm <- travel_time_matrix(fixture_network(), s$points, sites, mode = "WALK",
                            max_trip_duration = 12, n_threads = 2)
  ttm <- ttm[!is.na(ttm$travel_time_p50), ]
  covered <- function(ids) {
    near <- unique(ttm$from_id[ttm$to_id %in% c(s$schools$id, ids)])
    as.numeric(sum(s$points$population[s$points$id %in% near]))
  }
  pairs <- utils::combn(s$others$id, 2, simplify = FALSE)
  expect_identical(max(vapply(pairs, covered, 0)), 46217)
  expect_identical(covered(character()), 37876)

  # the same from the table, without a network or coordinates
  again <- locate_facilities(
    NULL, s$points[c("id", "population")], s$others["id"], existing = s$schools["id"],
    model = "max_cover", n_facilities = 2, cutoff = 12,
    demand_colname = "population", travel_times = ttm
  )
  expect_named(again$sites, c("id", "order", "marginal", "cumulative"))
  expect_identical(again$sites$id, found$sites$id)
  expect_identical(again$summary, found$summary)
})

test_that("the other models give the recorded solutions", {
  pts <- fixture_points()
  net <- fixture_network()
  sc <- locate_facilities(net, pts, pts, model = "set_cover", cutoff = 10,
                          demand_colname = "population", mode = "WALK", n_threads = 2)
  expect_identical(sc$summary$objective, 7)
  expect_identical(nrow(sc$sites), 7L)
  expect_true(all(sc$demand$covered))
  expect_identical(sc$sites$cumulative[7], 46273)

  med <- locate_facilities(net, pts, pts, model = "p_median", n_facilities = 2,
                           demand_colname = "population", mode = "WALK",
                           max_trip_duration = 60, n_threads = 2)
  expect_identical(med$sites$id, c("89a8100c027ffff", "89a8100c03bffff"))
  expect_identical(med$summary$objective_greedy, 560703)
  expect_identical(med$summary$objective, 446711)
  expect_identical(med$summary$objective,
                   sum(med$demand$demand * med$demand$travel_time))
  expect_true(is.na(med$summary$covered))
  expect_identical(med$demand$facility_id[1:2], rep("89a8100c03bffff", 2))
  expect_identical(med$demand$travel_time[1:2], c(8, 9))

  cen <- locate_facilities(net, pts, pts, model = "p_center", n_facilities = 2,
                           demand_colname = "population", mode = "WALK",
                           max_trip_duration = 60, n_threads = 2)
  expect_identical(cen$sites$id, c("89a8100c02fffff", "89a8100c067ffff"))
  expect_identical(cen$summary$objective, 19)
  expect_identical(max(cen$demand$travel_time), 19)

  # by public transport, and the same with one thread and with several
  tr <- function(threads) {
    locate_facilities(net, pts, pts, model = "p_median", n_facilities = 3,
                      demand_colname = "population", mode = c("WALK", "TRANSIT"),
                      departure_datetime = departure, seed = 1,
                      max_trip_duration = 60, n_threads = threads)
  }
  one <- tr(1)
  expect_identical(one$sites$id,
                   c("89a8100c03bffff", "89a8100c1dbffff", "89a8100c15bffff"))
  expect_identical(one$summary$objective, 351640)
  expect_identical(tr(several_threads()), one)
})

test_that("the tables take the class of options(peabiru.output = )", {
  local_output("data.frame")
  s <- school_split()
  found <- locate_facilities(fixture_network(), s$points, s$others, model = "max_cover",
                             cutoff = 10, mode = "WALK", n_threads = 2)
  expect_identical(class(found$sites), "data.frame")
  expect_identical(class(found$demand), "data.frame")
  # without demand_colname every point weighs 1
  expect_identical(found$summary$total_demand, 27)
  fca <- floating_catchment(fixture_network(), s$points, s$schools, cutoff = 10,
                            mode = "WALK", n_threads = 2)
  expect_identical(class(fca$supply), "data.frame")
})

test_that("floating_catchment computes the two steps", {
  times <- data.frame(from_id = c("a", "a", "b", "b"), to_id = c("f", "g", "f", "g"),
                      travel_time = c(5, 20, 12, 8))
  demand <- data.frame(id = c("a", "b"), residents = c(100, 200))
  supply <- data.frame(id = c("f", "g"), places = c(10, 30))
  fca <- floating_catchment(NULL, demand, supply, cutoff = 15,
                            demand_colname = "residents", supply_colname = "places",
                            travel_times = times)
  expect_named(fca, c("demand", "supply"))
  expect_named(fca$demand, c("id", "demand", "accessibility"))
  expect_named(fca$supply, c("id", "supply", "catchment_demand", "ratio"))
  expect_identical(fca$supply$catchment_demand, c(300, 200))
  expect_equal(fca$supply$ratio, c(10 / 300, 30 / 200), tolerance = 1e-15)
  expect_equal(fca$demand$accessibility, c(10 / 300, 10 / 300 + 30 / 200),
               tolerance = 1e-15)

  s <- school_split()
  run <- function(...) {
    floating_catchment(fixture_network(), s$points, s$schools,
                       demand_colname = "population", supply_colname = "schools",
                       mode = "WALK", n_threads = 2, ...)
  }
  step <- run(cutoff = 20)
  expect_identical(step$supply$catchment_demand,
                   c(22033, 17501, 10593, 21263, 22664, 11967))
  expect_equal(step$demand$accessibility[1:3],
               c(4.538646575591159e-05, 4.538646575591159e-05, 0.00013978842931675365),
               tolerance = 1e-14)
  # the index shares out the supply: 7 schools
  expect_equal(sum(step$demand$demand * step$demand$accessibility), 7, tolerance = 1e-12)
  logistic <- run(cutoff = 20, decay_function = "logistic", decay_value = 5)
  expect_equal(logistic$demand$accessibility[1:2],
               c(6.917279110814355e-05, 7.752430155338007e-05), tolerance = 1e-13)
  expect_equal(sum(logistic$demand$demand * logistic$demand$accessibility), 7,
               tolerance = 1e-12)
  fixed <- run(decay_function = "fixed_exponential", decay_value = 0.001)
  expect_true(all(fixed$demand$accessibility > 0))
})

test_that("bad arguments are refused", {
  s <- school_split()
  net <- fixture_network()
  expect_error(locate_facilities(net, s$points, s$others, model = "median", cutoff = 10),
               "model")
  expect_error(locate_facilities(net, s$points, s$others, model = "max_cover"),
               "cutoff failed: must not be NULL")
  expect_error(locate_facilities(net, s$points, s$others, model = "p_median", cutoff = 10),
               "cutoff failed: must be NULL")
  expect_error(locate_facilities(net, s$points, s$others, cutoff = 10, n_facilities = 0),
               "n_facilities")
  expect_error(locate_facilities(net, s$points, s$others, cutoff = 10, n_facilities = 22,
                                 mode = "WALK"),
               "only 21 candidate sites")
  expect_error(locate_facilities(net, s$points, s$points, existing = s$schools, cutoff = 10),
               "must not share ids")
  expect_error(locate_facilities(net, s$points, s$others, cutoff = 10,
                                 demand_colname = "residents"),
               "no column 'residents'")
  expect_error(locate_facilities(NULL, s$points, s$others, cutoff = 10),
               "needs a `network` or a table of `travel_times`")
  expect_error(locate_facilities(NULL, s$points, s$others, cutoff = 10, seed = 1,
                                 travel_times = data.frame()),
               "routing arguments")
  expect_error(locate_facilities(net, s$points, s$others, cutoff = 10, percentiles = 25),
               "use `percentile`")
  # `mode` is an argument of its own, never a short form of `model`
  found <- locate_facilities(net, s$points, s$others, cutoff = 10, mode = "BICYCLE",
                             n_threads = 2)
  expect_identical(found$summary$model, "max_cover")
  bad <- s$points
  bad$population <- -1
  expect_error(locate_facilities(net, bad, s$others, cutoff = 10,
                                 demand_colname = "population"),
               "not negative")
  expect_error(floating_catchment(net, s$points, s$schools, mode = "WALK"),
               "cutoff failed: must not be NULL")
  expect_error(floating_catchment(net, s$points, s$schools, cutoff = 10,
                                  decay_function = "fixed_exponential", decay_value = 0.001),
               "cutoff failed: must be NULL")
})

test_that("set covering takes a coverage target", {
  pts <- fixture_points()
  net <- fixture_network()
  cover <- function(...) {
    locate_facilities(net, pts, pts, model = "set_cover", cutoff = 10,
                      demand_colname = "population", mode = "WALK", n_threads = 2, ...)
  }
  part <- cover(coverage = 0.9)
  expect_identical(nrow(part$sites), 4L)
  expect_identical(part$summary$covered, 43759)
  expect_gte(part$summary$covered, 0.9 * part$summary$total_demand)
  expect_identical(sum(part$demand$demand[part$demand$covered]), 43759)
  expect_identical(cover(coverage = 1)$sites, cover()$sites)
  expect_identical(part$summary$upper_bound, NA_real_)
  expect_error(cover(coverage = 0), "'coverage' failed: Must be greater than 0")
  expect_error(cover(coverage = 1.5), "'coverage' failed")
  expect_error(cover(coverage = "a"), "'coverage' failed")
})

test_that("maximal covering reports a bound on the optimum", {
  pts <- fixture_points()
  net <- fixture_network()
  schools <- pts[pts$schools > 0, ]
  others <- pts[pts$schools == 0, ]
  found <- locate_facilities(net, pts, others, existing = schools, n_facilities = 2,
                             cutoff = 12, demand_colname = "population", mode = "WALK",
                             n_threads = 2)
  expect_identical(found$summary$covered, 46217)
  expect_identical(found$summary$upper_bound, 46273)
  # without swaps the sites are nested: the curve for every number of sites
  curve <- locate_facilities(net, pts, others, existing = schools,
                             n_facilities = nrow(others), cutoff = 12, swap = FALSE,
                             demand_colname = "population", mode = "WALK", n_threads = 2)
  expect_identical(curve$sites$cumulative, c(43640, 46040, 46217, 46273))
  expect_identical(curve$summary$upper_bound, curve$summary$covered)
  two <- locate_facilities(net, pts, others, existing = schools, n_facilities = 2,
                           cutoff = 12, swap = FALSE, demand_colname = "population",
                           mode = "WALK", n_threads = 2)
  expect_identical(two$sites$id, curve$sites$id[1:2])
})

test_that("maximal covering takes capacities", {
  pts <- fixture_points()
  net <- fixture_network()
  schools <- pts[pts$schools > 0, ]
  others <- pts[pts$schools == 0, ]
  locate <- function(...) {
    locate_facilities(net, pts, others, existing = schools, cutoff = 12,
                      demand_colname = "population", mode = "WALK", n_threads = 2, ...)
  }
  found <- locate(n_facilities = 4, capacity = 3000)
  expect_identical(found$sites$id, rep("89a8100c00fffff", 4))
  expect_identical(found$sites$marginal, rep(3000, 4))
  expect_identical(found$summary$covered_before, 16623)
  expect_identical(found$summary$covered, 28623)
  expect_identical(found$summary$upper_bound, 28623)
  expect_identical(found$candidates$units[found$candidates$chosen], 4L)
  expect_identical(sum(found$demand$demand * found$demand$served), 28623)
  expect_true(all(found$demand$served >= 0 & found$demand$served <= 1))
  expect_true(all(found$demand$served[!found$demand$covered] == 0))
  # who serves whom: the flows add up to the service of each point and site
  flows <- found$flows
  expect_named(flows, c("demand_id", "facility_id", "served"))
  expect_identical(sum(flows$served), 28623)
  expect_identical(sum(flows$served[flows$facility_id == "89a8100c00fffff"]), 12000)
  expect_true(all(tapply(flows$served, flows$facility_id, sum)[schools$id] <= 3000))
  by_point <- tapply(flows$served, flows$demand_id, sum)
  expect_equal(as.vector(by_point[found$demand$id[found$demand$served > 0]]),
               with(found$demand, (demand * served)[served > 0]))
  expect_false(anyDuplicated(flows[c("demand_id", "facility_id")]) > 0)
  # more facilities than candidates: as many as have someone to serve
  all_served <- locate(n_facilities = 30, capacity = 3000)
  expect_identical(nrow(all_served$sites), 12L)
  expect_identical(all_served$summary$covered, 46273)
  expect_true(all(all_served$demand$served == 1))
  # a column of capacities, in the candidates and in the existing facilities
  schools$places <- 3000
  others$places <- 3000
  by_column <- locate_facilities(net, pts, others, existing = schools, cutoff = 12,
                                 n_facilities = 4, capacity = "places",
                                 demand_colname = "population", mode = "WALK",
                                 n_threads = 2)
  expect_identical(by_column$sites, found$sites)
  expect_false("units" %in% names(locate(n_facilities = 2)$candidates))
  expect_null(locate(n_facilities = 2)$flows)
  expect_error(locate(capacity = 0), "'capacity' failed: Must be greater than 0")
  expect_error(locate(capacity = "seats"), "'existing' has no column 'seats'")
  expect_error(locate_facilities(net, pts, others, model = "p_median", capacity = 10),
               "capacity failed: must be NULL when model is p_median")
})

test_that("distance_matrix gives the pairs within a distance", {
  pts <- fixture_points()
  near <- distance_matrix(pts, pts, max_distance = 500)
  expect_named(near, c("from_id", "to_id", "distance"))
  expect_identical(nrow(near), 151L)
  expect_identical(sum(near$from_id == near$to_id), 27L)
  expect_true(all(near$distance[near$from_id == near$to_id] == 0))
  expect_equal(sum(near$distance), 43426.8219512176, tolerance = 1e-12)
  expect_lte(max(near$distance), 500)
  expect_identical(near$from_id[1:3], rep(pts$id[1], 3))
  expect_identical(nrow(distance_matrix(pts, pts[0, ], 500)), 0L)
  found <- locate_facilities(NULL, pts, pts, model = "max_cover", n_facilities = 2,
                             cutoff = 500, demand_colname = "population",
                             travel_times = near)
  expect_identical(found$sites$id, c("89a8100c03bffff", "89a8100c037ffff"))
  expect_identical(found$summary$covered, 33337)
  expect_error(distance_matrix(pts, pts, -1), "'max_distance' failed")
  expect_error(distance_matrix(pts, pts), "max_distance")
})

test_that("allocate_budget shares facilities among groups", {
  curves <- data.frame(group = c("north", "north", "north", "south", "south"),
                       marginal = c(500, 200, 50, 400, 300))
  groups <- data.frame(group = c("north", "south"), covered = c(1000, 0),
                       total = c(2000, 1000))
  most <- allocate_budget(curves, 3, groups)
  expect_named(most, c("steps", "groups"))
  expect_identical(most$steps$group, c("north", "south", "south"))
  expect_identical(most$steps$covered, c(1500, 1900, 2200))
  expect_identical(most$groups$units, c(1L, 2L))
  expect_identical(most$groups$share, c(0.75, 0.7))
  fair <- allocate_budget(curves, 3, groups, criterion = "maximin")
  expect_identical(fair$steps$group, c("south", "south", "north"))
  expect_identical(fair$steps$lowest_share, c(0.4, 0.5, 0.7))
  # rows in any order with `order`; more budget than facilities; no groups
  mixed <- cbind(curves, order = c(1, 2, 3, 1, 2))[c(5, 3, 1, 4, 2), ]
  every <- allocate_budget(mixed, 9)
  expect_identical(every$steps$marginal, c(500, 400, 300, 200, 50))
  expect_true(all(is.na(every$steps$lowest_share)))
  expect_identical(every$groups$group, c("south", "north"))
  # a group with no facility to open still counts in the shares
  alone <- allocate_budget(curves[1:3, ], 1,
                           data.frame(group = c("north", "other"), total = c(1000, 10)))
  expect_identical(alone$steps$lowest_share, 0)
  expect_error(allocate_budget(curves, 3, criterion = "maximin"), "groups failed: must not be NULL")
  expect_error(allocate_budget(curves, 0), "'budget' failed")
  expect_error(allocate_budget(curves[c(2, 1), ], 2), "may not increase")
  expect_error(allocate_budget(curves, 2, groups[1, ]), "groups that `groups` does not")
  expect_error(allocate_budget(curves, 2, criterion = "fair"), "'criterion' failed")
})

test_that("allocate_budget follows weights, tiers and groups left out", {
  curves <- data.frame(group = c("north", "north", "north", "south", "south", "east"),
                       marginal = c(500, 200, 50, 400, 300, 100))
  groups <- data.frame(group = c("north", "south", "east"), covered = c(1000, 0, 450),
                       total = c(2000, 1000, 600), weight = c(1, 2, 6),
                       tier = c(1, 2, 0), open = c(TRUE, FALSE, TRUE))
  plain <- allocate_budget(curves, 9, groups)
  expect_identical(plain$groups$priority, 1:3)
  heavy <- allocate_budget(curves, 4, groups, weights = "weight")
  expect_identical(heavy$steps$group, c("south", "south", "east", "north"))
  expect_identical(heavy$steps$marginal, c(400, 300, 100, 500))
  expect_identical(heavy$steps$covered, c(1850, 2150, 2250, 2750))
  expect_identical(heavy$groups$priority, c(3L, 1L, 2L))
  fair <- allocate_budget(curves, 3, groups, criterion = "maximin", weights = "weight")
  expect_identical(fair$steps$group, c("south", "east", "south"))
  expect_identical(fair$steps$lowest_share, c(0.4, 0.4, 0.5))
  first <- allocate_budget(curves, 5, groups, tiers = "tier")
  expect_identical(first$steps$group, c("east", "north", "north", "north", "south"))
  expect_identical(first$groups$priority, c(2L, 3L, 1L))
  out <- allocate_budget(curves, 9, groups, criterion = "maximin", eligible = "open")
  expect_identical(out$steps$group, c("north", "north", "east", "north"))
  expect_identical(out$groups$units, c(3L, 0L, 1L))
  expect_identical(out$groups$priority, c(1L, NA, 2L))
  expect_identical(out$steps$lowest_share, rep(0, 4))
  expect_identical(out$groups$share[2], 0)
  # weights of 1, one tier and every group open change nothing
  groups$one <- 1; groups$zero <- 0; groups$all <- TRUE
  expect_identical(allocate_budget(curves, 9, groups, weights = "one", tiers = "zero",
                                   eligible = "all"), plain)
  expect_error(allocate_budget(curves, 2, weights = "weight"), "weights failed: must be NULL without")
  expect_error(allocate_budget(curves, 2, groups, weights = "w"), "has no column \"w\"")
  expect_error(allocate_budget(curves, 2, groups, weights = "zero"), "greater than 0")
  expect_error(allocate_budget(curves, 2, groups, tiers = "weight", eligible = "tier"), "Must be logical")
  groups$half <- 0.5
  expect_error(allocate_budget(curves, 2, groups, tiers = "half"), "whole numbers from 0")
})
