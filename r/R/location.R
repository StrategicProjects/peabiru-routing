#' Choose where to open new facilities
#'
#' Chooses, among the candidate sites, where to open facilities so that the
#' demand points are served best, by one of four classic location models.
#' The travel times from every demand point to every site are computed with
#' [travel_time_matrix()] (or taken from `travel_times`), so any mode,
#' departure time and routing option of the matrix applies.
#'
#' @section Models:
#' - `"max_cover"`, maximal covering (Church and ReVelle, 1974): with
#'   `n_facilities` new facilities, cover the most demand within `cutoff`
#'   minutes.
#' - `"set_cover"`, location set covering (Toregas et al., 1971): the
#'   fewest new facilities that leave every demand point within `cutoff`
#'   minutes of one (every point some site can cover; the demand of the
#'   others is reported in `summary$unservable_demand`). With `coverage`
#'   below 1, the fewest that cover that share of the demand (partial set
#'   covering).
#' - `"p_median"` (Hakimi, 1964): `n_facilities` new facilities with the
#'   smallest demand-weighted sum of travel times to the nearest facility.
#' - `"p_center"` (Hakimi, 1964): `n_facilities` new facilities with the
#'   smallest travel time for the demand point farthest from its nearest
#'   facility.
#'
#' The `existing` facilities are already open and count first: only the
#' demand they leave uncovered, or the travel time they leave, drives the
#' choice of the new sites. They are never closed or moved.
#'
#' @section How the models are solved:
#' By heuristics, with no integer programming solver: a greedy
#' construction (each site the one that improves the objective most, given
#' the ones before it) followed, with `swap`, by vertex substitution (Teitz
#' and Bart, 1968): an open new site is swapped for a closed candidate
#' while that improves the objective, the best swap of all at each step.
#' For `"set_cover"` the greedy choice is the site covering the most
#' points not yet covered, and the improvement drops the sites the others
#' make redundant; for `"p_center"` greedy and swaps compare the largest
#' travel time and, on ties, the weighted sum. The solution is good and
#' reproducible, not proven optimal. For maximal covering the greedy
#' solution covers at least 63% (1 - 1/e) of the most that could be covered
#' of the demand not yet covered (Nemhauser, Wolsey and Fisher, 1978),
#' and `summary$upper_bound` says how good a given solution is: no choice
#' of `n_facilities` sites covers more demand than that, so the solution
#' reaches at least `covered / upper_bound` of the optimum (often all of
#' it).
#' Ties go to the candidate listed first, and the result does not depend
#' on `n_threads`.
#'
#' In `"set_cover"` and `"p_center"` a demand point with weight 0 is not
#' demand, and the others count the same whatever their weight (in
#' `"set_cover"` with `coverage` below 1 the weights count: the sites are
#' chosen by the demand each newly covers). In
#' `"p_median"` and `"p_center"` a demand point that reaches no open
#' facility (within `max_trip_duration`) is charged `unreachable_time`
#' minutes, and one that no site at all reaches is left out and reported.
#'
#' @section Facilities with a capacity:
#' With `capacity`, `"max_cover"` is the capacitated maximal covering
#' (Current and Storbeck, 1988): a facility serves at most its capacity of
#' the demand within `cutoff`, so a place with much demand may need
#' several. A candidate may then open more than once (`sites` has one row
#' per facility opened, and repeats the site), `n_facilities` counts
#' facilities, not sites, and may exceed the number of candidates.
#' Facilities are opened one at a time, each where it would serve the most
#' demand not yet served, taking it from the nearest demand points first; a
#' demand point may be served in part, or by more than one facility. The
#' existing facilities serve first, each pair of facility and demand point
#' in the order of their travel times. `swap` has no effect.
#' `summary$covered` is the demand served, `candidates` gains `units` (how
#' many facilities opened there) and `demand` gains `served` (the share of
#' the point's demand that is served, from 0 to 1), while `covered` keeps
#' its meaning: within the cutoff of a facility, whether or not it has room.
#' The result also has a fourth table, `flows`, with who serves whom: one
#' row per pair of demand point and facility with demand served between
#' them (`demand_id`, `facility_id`, and `served`, in the unit of the
#' demand), existing facilities included and the facilities opened at the
#' same site added up.
#' `summary$upper_bound` is a ceiling on what any choice of `n_facilities`
#' facilities could serve, given what the existing ones serve.
#'
#' @section The whole curve in one call:
#' With `swap = FALSE` the sites of `"max_cover"` are nested: the first
#' `k` rows of `sites` are the answer for `n_facilities = k`. Asking for
#' as many facilities as there are candidates (with `min_gain` to stop
#' where a site no longer pays) therefore gives, in `marginal` and
#' `cumulative`, the demand covered for every number of facilities. With
#' `swap = TRUE` the sites for one number of facilities need not include
#' those for a smaller number. [allocate_budget()] shares a number of
#' facilities among several such curves.
#'
#' @section Costs other than minutes:
#' The models do not depend on the unit of the table. `travel_times` may
#' hold any cost that is not negative, such as a distance in metres;
#' `cutoff`, `unreachable_time` and the `travel_time` column of the result
#' are then in that unit. [distance_matrix()] makes such a table from the
#' straight-line distances between two sets of points.
#'
#' @inheritParams travel_time_matrix
#' @param network The network of [build_network()]; may be `NULL` with
#'   `travel_times`.
#' @param demand The demand points: a `data.frame` with `id`, `lon` and
#'   `lat`, or a `POINT sf` with an `id` column, and, with
#'   `demand_colname`, their weights.
#' @param candidates The sites where a facility could open: a `data.frame`
#'   or `POINT sf` with `id`, `lon`, `lat`.
#' @param existing The facilities already open (`id`, `lon`, `lat`), or
#'   `NULL`. Their ids may not repeat those of the candidates.
#' @param model A string: `"max_cover"`, `"set_cover"`, `"p_median"` or
#'   `"p_center"`.
#' @param n_facilities How many new facilities to open. Fewer come back
#'   when no further site improves the objective. Ignored by
#'   `"set_cover"`.
#' @param cutoff The service standard of the covering models, in minutes:
#'   a point is covered by a site at most this far (travel time `<=
#'   cutoff`). Required by `"max_cover"` and `"set_cover"`, `NULL` for the
#'   other models.
#' @param demand_colname The name of the numeric column of `demand` with
#'   the weight of each point (residents, pupils...), finite and not
#'   negative; `NULL` for a weight of 1 each.
#' @param percentile The travel-time percentile used (public transport
#'   over a time window), a whole number from 1 to 99.
#' @param max_trip_duration The longest trip considered, in minutes.
#'   `NULL`: `cutoff` (rounded up) for the covering models, 120 for the
#'   others.
#' @param travel_times A table of travel times to use instead of routing:
#'   `from_id` (demand point), `to_id` (site) and `travel_time_pXX` for
#'   the `percentile` (or `travel_time`), in minutes, as
#'   [travel_time_matrix()] returns it, or `distance`, as
#'   [distance_matrix()] does. A pair that is absent or has a
#'   missing time is unreachable; rows of other points are ignored. With
#'   it, `network` and the routing arguments are not used and the points
#'   need only their `id`.
#' @param swap A logical: improve the greedy solution by vertex
#'   substitution (for `"set_cover"`, drop the sites the others make
#'   redundant).
#' @param min_gain `"max_cover"` stops opening sites when the best one
#'   left would cover less than this much demand.
#' @param coverage For `"set_cover"`, the share of the total demand to
#'   cover, what the existing facilities cover included: a number above 0
#'   and at most 1. With 1 (the default) every demand point that some site
#'   can cover is covered. Below 1 the answer is the fewest sites found
#'   that cover that share of the demand (partial set covering), chosen by
#'   the demand each newly covers; when the target is out of reach, all
#'   the demand that can be covered is.
#' @param capacity For `"max_cover"`, how much demand one facility can
#'   serve: a positive number, or the name of a numeric column of
#'   `candidates` (and of `existing`, when given) with the capacity at each
#'   site. `NULL` (the default): a facility serves all the demand within
#'   the cutoff. See the section on capacities.
#' @param unreachable_time For `"p_median"` and `"p_center"`, the minutes
#'   charged to a demand point that reaches no open facility. `NULL`:
#'   twice the longest travel time of the table.
#' @param n_threads The number of threads (`Inf`: all cores).
#' @param ... Any other argument of [travel_time_matrix()], by its full
#'   name: `time_window`, `walk_speed`, `max_walk_time`, `seed` and so on
#'   (not `percentiles` or `output_dir`).
#'
#' @return A list of three tables and a summary:
#' - `sites`: the new sites, ordered by what each adds: `id` (with `lon`
#'   and `lat` when the candidates have them), `order`, `marginal` (demand
#'   newly covered, or by how much the objective falls) and `cumulative`
#'   (demand covered so far, existing facilities included, or the
#'   objective so far).
#' - `candidates`: one row per candidate: `id`, `potential` (what it would
#'   add as the only new facility; for the covering models, the demand not
#'   covered today that it would cover alone) and `chosen`.
#' - `demand`: one row per demand point: `id`, `demand` (its weight),
#'   `facility_id` and `travel_time` (the nearest open facility, existing
#'   or new, and the minutes to it; `NA` when it reaches none) and
#'   `covered` (within the cutoff of a facility; for `"p_median"` and
#'   `"p_center"`, whether it reaches one).
#' - `summary`: a list with `model`, `n_sites`, `objective_before`
#'   (existing facilities only), `objective_greedy`, `objective` (demand
#'   covered, number of sites, weighted minutes or largest minutes, by
#'   model), `covered_before` and `covered` (covering models, else `NA`),
#'   `upper_bound` (`"max_cover"`, else `NA`: no choice of `n_facilities`
#'   sites covers more demand than this, so `covered / upper_bound` is a
#'   floor on the share of the optimum reached), `total_demand`, `unservable_demand` (demand no site can serve) and
#'   `n_swaps`.
#'
#'   `r doc_output_class()`
#'
#' @references
#' Church R, ReVelle C (1974). "The maximal covering location problem."
#' *Papers of the Regional Science Association*, **32**, 101-118.
#' \doi{10.1007/BF01942293}.
#'
#' Current JR, Storbeck JE (1988). "Capacitated covering models."
#' *Environment and Planning B: Planning and Design*, **15**(2), 153-163.
#' \doi{10.1068/b150153}.
#'
#' Hakimi SL (1964). "Optimum locations of switching centers and the
#' absolute centers and medians of a graph." *Operations Research*,
#' **12**(3), 450-459. \doi{10.1287/opre.12.3.450}.
#'
#' Nemhauser GL, Wolsey LA, Fisher ML (1978). "An analysis of
#' approximations for maximizing submodular set functions-I."
#' *Mathematical Programming*, **14**, 265-294. \doi{10.1007/BF01588971}.
#'
#' Teitz MB, Bart P (1968). "Heuristic methods for estimating the
#' generalized vertex median of a weighted graph." *Operations Research*,
#' **16**(5), 955-961. \doi{10.1287/opre.16.5.955}.
#'
#' Toregas C, Swain R, ReVelle C, Bergman L (1971). "The location of
#' emergency service facilities." *Operations Research*, **19**(6),
#' 1363-1373. \doi{10.1287/opre.19.6.1363}.
#' @seealso [floating_catchment()] for supply against demand around each
#'   place.
#' @family location functions
#' @export
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' points <- read.csv(file.path(data_path, "spo_points.csv"))
#'
#' # two new schools, so that the most residents are within a 12-minute
#' # walk of a school
#' schools <- points[points$schools > 0, ]
#' others <- points[points$schools == 0, ]
#' found <- locate_facilities(
#'   network, points, others, existing = schools,
#'   model = "max_cover", n_facilities = 2, cutoff = 12,
#'   demand_colname = "population", mode = "WALK", n_threads = 2
#' )
#' found$sites
#' found$summary$covered / found$summary$total_demand
locate_facilities <- function(network,
                              demand,
                              candidates,
                              existing = NULL,
                              model = "max_cover",
                              n_facilities = 1L,
                              cutoff = NULL,
                              demand_colname = NULL,
                              mode = "WALK",
                              mode_egress = "WALK",
                              departure_datetime = NULL,
                              percentile = 50L,
                              max_trip_duration = NULL,
                              travel_times = NULL,
                              swap = TRUE,
                              min_gain = 0,
                              coverage = 1,
                              capacity = NULL,
                              unreachable_time = NULL,
                              n_threads = Inf,
                              ...) {
  kind <- output_class()
  routed <- location_source(network, travel_times, list(...), "locate_facilities")
  models <- c("max_cover", "set_cover", "p_median", "p_center")
  cm_assert_and("model", check_string(model), check_names(model, subset.of = models))
  covering <- model %in% c("max_cover", "set_cover")
  assert_count(n_facilities, "n_facilities", positive = model != "set_cover")
  if (covering) {
    if (is.null(cutoff)) {
      stop("Assertion on cutoff failed: must not be NULL when model is ", model,
           " (the minutes within which a demand point is covered).", call. = FALSE)
    }
    assert_number(cutoff, "cutoff", lower = 0, finite = TRUE)
  } else if (!is.null(cutoff)) {
    stop("Assertion on cutoff failed: must be NULL when model is ", model,
         " (the model has no service standard).", call. = FALSE)
  }
  assert_flag(swap, "swap")
  assert_number(min_gain, "min_gain", lower = 0, finite = TRUE)
  assert_number(coverage, "coverage", lower = 0, upper = 1)
  if (coverage <= 0) {
    stop("Assertion on 'coverage' failed: Must be greater than 0.", call. = FALSE)
  }
  assert_number(unreachable_time, "unreachable_time", lower = 0, finite = TRUE,
                null_ok = TRUE)
  if (!is.null(unreachable_time) && unreachable_time <= 0) {
    stop("Assertion on 'unreachable_time' failed: Must be greater than 0.", call. = FALSE)
  }
  assert_integerish(percentile, "percentile", lower = 1, upper = 99, min_len = 1,
                    max_len = 1)
  if (!is.null(max_trip_duration)) {
    assert_count(max_trip_duration, "max_trip_duration", positive = TRUE)
  }
  threads <- assign_n_threads(n_threads)

  demand <- location_points(demand, "demand", routed)
  candidates <- location_points(candidates, "candidates", routed)
  if (!is.null(existing)) existing <- location_points(existing, "existing", routed)
  weights <- location_weights(demand, demand_colname, "demand", "demand_colname")
  n_existing <- if (is.null(existing)) 0L else nrow(existing)
  room <- location_capacity(capacity, existing, candidates, model)
  shared <- intersect(existing$id, candidates$id)
  if (length(shared) > 0) {
    stop("`candidates` and `existing` must not share ids. Shared id",
         if (length(shared) == 1L) "" else "s", ": ",
         paste0("\"", utils::head(shared, 10), "\"", collapse = ", "), ".",
         call. = FALSE)
  }
  if (nrow(candidates) == 0) {
    stop("`candidates` must have at least one site.", call. = FALSE)
  }

  # the sites of the table: the existing facilities, then the candidates
  cols <- if (routed) c("id", "lon", "lat") else "id"
  sites <- rbind(existing[cols], candidates[cols])
  if (is.null(max_trip_duration)) {
    max_trip_duration <- if (covering) max(1, ceiling(cutoff)) else 120
  }
  pairs <- location_pairs(network, demand, sites, travel_times, percentile,
                          max_trip_duration,
                          c(list(mode = mode, mode_egress = mode_egress,
                                 departure_datetime = departure_datetime,
                                 n_threads = n_threads), list(...)),
                          routed, "locate_facilities")
  res <- raise_if_stopped(rs_locate_facilities(
    nrow(demand), nrow(sites), pairs$i, pairs$j, pairs$minutes, weights,
    n_existing, model, as.integer(n_facilities),
    if (covering) as.double(cutoff) else 0, swap, as.double(min_gain),
    as.double(coverage), room,
    if (is.null(unreachable_time)) -1 else as.double(unreachable_time), threads
  ))

  chosen <- res$sites + 1L - n_existing
  coords <- function(df, rows = seq_len(nrow(df))) {
    df[rows, intersect(c("id", "lon", "lat"), names(df)), drop = FALSE]
  }
  minutes <- res$assigned_minutes
  capacitated <- length(room) > 0
  out <- list(
    sites = new_df(c(
      as.list(coords(candidates, chosen)),
      list(order = seq_along(chosen), marginal = res$marginal,
           cumulative = res$cumulative)
    )),
    candidates = new_df(c(
      as.list(coords(candidates)),
      list(potential = res$potential[n_existing + seq_len(nrow(candidates))],
           chosen = seq_len(nrow(candidates)) %in% chosen),
      if (capacitated) list(units = tabulate(chosen, nrow(candidates)))
    )),
    demand = new_df(c(
      as.list(coords(demand)),
      list(demand = weights,
           facility_id = sites$id[res$assigned_site + 1L],
           travel_time = minutes,
           covered = if (covering) !is.na(minutes) & minutes <= cutoff else !is.na(minutes)),
      if (capacitated) list(served = ifelse(weights > 0, res$served / weights, 0))
    )),
    summary = list(
      model = model,
      n_sites = length(chosen),
      objective_before = res$objective_before,
      objective_greedy = res$objective_greedy,
      objective = res$objective,
      covered_before = if (covering) res$covered_before else NA_real_,
      covered = if (covering) res$covered else NA_real_,
      upper_bound = if (model == "max_cover") res$upper_bound else NA_real_,
      total_demand = res$total_demand,
      unservable_demand = res$unservable_demand,
      n_swaps = res$n_swaps
    )
  )
  if (capacitated) {
    out$flows <- new_df(list(
      demand_id = demand$id[res$flow_demand + 1L],
      facility_id = sites$id[res$flow_site + 1L],
      served = res$flow_served
    ))
  }
  as_output_list(out, kind, c("sites", "candidates", "demand", "flows"))
}

#' Supply against demand around each place: the two-step floating catchment area
#'
#' Step 1: the supply of each facility (places, staff, beds) is divided by
#' the demand within its catchment. Step 2: each demand point adds up the
#' ratios of the facilities within its catchment. The result, per demand
#' point, is supply per unit of demand, which accounts both for how near
#' the facilities are and for how many others compete for them (Luo and
#' Wang, 2003).
#'
#' With `decay_function = "step"` a pair of demand point and facility
#' counts in full up to `cutoff` minutes (travel time `<= cutoff`) and not
#' at all beyond: the two-step floating catchment area in its original
#' form. Any other decay function gives the enhanced form, in which nearer
#' pairs weigh more, in both steps (Luo and Qi, 2009, with a continuous
#' weight in place of their travel-time zones); the functions and their
#' values are those of [accessibility()], and the trips beyond the cutoff
#' still count with their weight, up to `max_trip_duration`.
#'
#' The sum over the demand points of demand times `accessibility` equals
#' the supply of the facilities that have any demand in their catchment.
#' The travel times are computed with [travel_time_matrix()], from each
#' demand point to each facility, or taken from `travel_times`.
#'
#' @inheritParams locate_facilities
#' @param supply The facilities: a `data.frame` or `POINT sf` with `id`,
#'   `lon`, `lat` and, with `supply_colname`, their capacities.
#' @param cutoff The catchment in minutes; `NULL` with
#'   `"fixed_exponential"`.
#' @param decay_function A string: `"step"`, `"exponential"`,
#'   `"fixed_exponential"`, `"linear"` or `"logistic"` (see
#'   [accessibility()]).
#' @param decay_value The parameter of `"fixed_exponential"`, `"linear"`
#'   and `"logistic"`, as in [accessibility()]; `NULL` for the others.
#' @param demand_colname,supply_colname The names of the numeric columns
#'   with the weight of each demand point and the capacity of each
#'   facility; `NULL` for 1 each.
#' @param max_trip_duration The longest trip considered, in minutes.
#'   `NULL`: `cutoff` (rounded up) for `"step"`, 120 for the other
#'   functions.
#'
#' @return A list of two tables:
#' - `demand`: one row per demand point: `id` (with `lon` and `lat` when
#'   given), `demand` (its weight) and `accessibility` (supply per unit of
#'   demand).
#' - `supply`: one row per facility: `id`, `supply`, `catchment_demand`
#'   (the weighted demand in its catchment) and `ratio` (supply divided by
#'   that demand; 0 when the catchment holds no demand).
#'
#'   `r doc_output_class()`
#'
#' @references
#' Luo W, Wang F (2003). "Measures of spatial accessibility to health care
#' in a GIS environment: synthesis and a case study in the Chicago
#' region." *Environment and Planning B: Planning and Design*, **30**(6),
#' 865-884. \doi{10.1068/b29120}.
#'
#' Luo W, Qi Y (2009). "An enhanced two-step floating catchment area
#' (E2SFCA) method for measuring spatial accessibility to primary care
#' physicians." *Health & Place*, **15**(4), 1100-1107.
#' \doi{10.1016/j.healthplace.2009.06.002}.
#' @seealso [locate_facilities()] to choose where to open facilities.
#' @family location functions
#' @export
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' network <- build_network(data_path, n_threads = 2)
#' points <- read.csv(file.path(data_path, "spo_points.csv"))
#'
#' # schools per resident within a 20-minute walk
#' schools <- points[points$schools > 0, ]
#' fca <- floating_catchment(
#'   network, points, schools, cutoff = 20,
#'   demand_colname = "population", supply_colname = "schools",
#'   mode = "WALK", n_threads = 2
#' )
#' head(fca$demand)
#' fca$supply
floating_catchment <- function(network,
                               demand,
                               supply,
                               cutoff = NULL,
                               decay_function = "step",
                               decay_value = NULL,
                               demand_colname = NULL,
                               supply_colname = NULL,
                               mode = "WALK",
                               mode_egress = "WALK",
                               departure_datetime = NULL,
                               percentile = 50L,
                               max_trip_duration = NULL,
                               travel_times = NULL,
                               ...) {
  kind <- output_class()
  routed <- location_source(network, travel_times, list(...), "floating_catchment")
  decay_list <- assign_decay_function(decay_function, decay_value)
  if (decay_function == "fixed_exponential") {
    if (!is.null(cutoff)) {
      stop("Assertion on cutoff failed: must be NULL when decay_function ",
           "is fixed_exponential.", call. = FALSE)
    }
    cutoff <- 0
  } else {
    if (is.null(cutoff)) {
      stop("Assertion on cutoff failed: must not be NULL when decay_function ",
           "is ", decay_function, ".", call. = FALSE)
    }
    assert_number(cutoff, "cutoff", lower = 0, finite = TRUE)
    if (cutoff <= 0) {
      stop("Assertion on 'cutoff' failed: Must be greater than 0.", call. = FALSE)
    }
  }
  assert_integerish(percentile, "percentile", lower = 1, upper = 99, min_len = 1,
                    max_len = 1)
  if (!is.null(max_trip_duration)) {
    assert_count(max_trip_duration, "max_trip_duration", positive = TRUE)
  }
  demand <- location_points(demand, "demand", routed)
  supply <- location_points(supply, "supply", routed)
  weights <- location_weights(demand, demand_colname, "demand", "demand_colname")
  capacity <- location_weights(supply, supply_colname, "supply", "supply_colname")
  if (is.null(max_trip_duration)) {
    max_trip_duration <- if (decay_function == "step") max(1, ceiling(cutoff)) else 120
  }
  pairs <- location_pairs(network, demand, supply, travel_times, percentile,
                          max_trip_duration,
                          c(list(mode = mode, mode_egress = mode_egress,
                                 departure_datetime = departure_datetime), list(...)),
                          routed, "floating_catchment")
  res <- raise_if_stopped(rs_floating_catchment(
    nrow(demand), nrow(supply), pairs$i, pairs$j, pairs$minutes, weights, capacity,
    as.double(cutoff), decay_list$fun, decay_list$value
  ))
  coords <- function(df) as.list(df[intersect(c("id", "lon", "lat"), names(df))])
  out <- list(
    demand = new_df(c(coords(demand),
                      list(demand = weights, accessibility = res$index))),
    supply = new_df(c(coords(supply),
                      list(supply = capacity, catchment_demand = res$catchment_demand,
                           ratio = res$ratio)))
  )
  as_output_list(out, kind, c("demand", "supply"))
}

# Whether the travel times of a location function are to be routed (TRUE)
# or come from `travel_times` (FALSE).
location_source <- function(network, travel_times, routing, fun) {
  if (!is.null(travel_times)) {
    if (length(routing) > 0) {
      stop(fun, "() got routing arguments with `travel_times`: ",
           paste(names(routing), collapse = ", "), ".", call. = FALSE)
    }
    return(FALSE)
  }
  refused <- intersect(names(routing), c("percentiles", "output_dir"))
  if (length(refused) > 0) {
    stop(fun, "() does not take `", refused[1], "`",
         if (refused[1] == "percentiles") " (use `percentile`)" else "", ".",
         call. = FALSE)
  }
  if (is.null(network)) {
    stop(fun, "() needs a `network` or a table of `travel_times`.", call. = FALSE)
  }
  assert_network(network)
  TRUE
}

# The points of a location function: checked as the points of a routing
# call when they are to be routed, else only their ids.
location_points <- function(df, name, routed) {
  if (routed) return(assign_points_input(df, name))
  if (!inherits(df, "data.frame")) {
    stop("'", name, "' must be a 'data.frame'.", call. = FALSE)
  }
  if (inherits(df, "sf")) df <- sf::st_drop_geometry(df)
  df <- as.data.frame(df)
  cm_assert(check_names(names(df), must.include = "id"), name)
  df$id <- as.character(df$id)
  if (anyDuplicated(df$id) > 0) {
    stop("`", name, "` must not contain duplicated ids.", call. = FALSE)
  }
  df
}

# The weight column of a table of points, or 1 per row without one.
location_weights <- function(df, colname, name, arg) {
  assert_string(colname, arg, null_ok = TRUE)
  if (is.null(colname)) return(rep(1, nrow(df)))
  if (!colname %in% names(df)) {
    stop("'", name, "' has no column '", colname, "' (the `", arg, "` argument).",
         call. = FALSE)
  }
  x <- df[[colname]]
  cm_assert(check_numeric(x), paste0(name, "$", colname))
  x <- as.double(x)
  if (any(!is.finite(x)) || any(x < 0)) {
    stop("Assertion on '", name, "$", colname, "' failed: Must be finite and not ",
         "negative, with no missing values.", call. = FALSE)
  }
  x
}

# The capacity of every site of the table (the existing facilities, then
# the candidates) from a number or the name of a column; none when NULL.
location_capacity <- function(capacity, existing, candidates, model) {
  if (is.null(capacity)) return(double())
  if (model != "max_cover") {
    stop("Assertion on capacity failed: must be NULL when model is ", model,
         " (only max_cover has capacities).", call. = FALSE)
  }
  if (is.character(capacity)) {
    assert_string(capacity, "capacity")
    one <- function(df, name) {
      if (is.null(df)) return(double())
      x <- location_weights(df, capacity, name, "capacity")
      if (any(x <= 0)) {
        stop("Assertion on '", name, "$", capacity, "' failed: Must be greater than 0.",
             call. = FALSE)
      }
      x
    }
    return(c(one(existing, "existing"), one(candidates, "candidates")))
  }
  assert_number(capacity, "capacity", lower = 0, finite = TRUE)
  if (capacity <= 0) {
    stop("Assertion on 'capacity' failed: Must be greater than 0.", call. = FALSE)
  }
  rep(as.double(capacity),
      (if (is.null(existing)) 0L else nrow(existing)) + nrow(candidates))
}

# The reachable pairs (0-based row, 0-based column, minutes) of the table
# the location models work on: from `travel_times` when given, else routed
# from the demand points to the sites.
location_pairs <- function(network, rows, cols, travel_times, percentile,
                           max_trip_duration, routing, routed, fun) {
  column <- sprintf("travel_time_p%02d", as.integer(percentile))
  if (routed) {
    # the points were checked, and warned about, already
    travel_times <- withCallingHandlers(
      do.call(travel_time_matrix, c(
        list(network, rows[c("id", "lon", "lat")], cols[c("id", "lon", "lat")],
             percentiles = percentile, max_trip_duration = max_trip_duration),
        routing
      )),
      warning = function(w) {
        if (grepl("missing lon/lat", conditionMessage(w), fixed = TRUE)) {
          invokeRestart("muffleWarning")
        }
      }
    )
  } else {
    if (!inherits(travel_times, "data.frame")) {
      stop("`travel_times` must be a 'data.frame'.", call. = FALSE)
    }
    if (!column %in% names(travel_times)) column <- "travel_time"
    if (!column %in% names(travel_times) && "distance" %in% names(travel_times)) {
      column <- "distance"
    }
    missing_cols <- setdiff(c("from_id", "to_id", column), names(travel_times))
    if (length(missing_cols) > 0) {
      stop("`travel_times` must have the columns 'from_id', 'to_id' and '",
           sprintf("travel_time_p%02d", as.integer(percentile)),
           "' (or 'travel_time'); missing: ",
           paste0("'", missing_cols, "'", collapse = ", "), ".", call. = FALSE)
    }
  }
  i <- match(as.character(travel_times[["from_id"]]), rows$id)
  j <- match(as.character(travel_times[["to_id"]]), cols$id)
  minutes <- as.double(travel_times[[column]])
  if (length(i) > 0 && !any(!is.na(i) & !is.na(j))) {
    stop(fun, "(): no row of `travel_times` joins a demand point ('from_id') ",
         "to a site ('to_id').", call. = FALSE)
  }
  keep <- !is.na(i) & !is.na(j) & is.finite(minutes)
  list(i = i[keep] - 1L, j = j[keep] - 1L, minutes = minutes[keep])
}

#' Straight-line distances between two sets of points
#'
#' Every pair of an origin and a destination at most `max_distance` metres
#' apart in a straight line, with the distance. It needs no network, and
#' its result is a table that [locate_facilities()] and
#' [floating_catchment()] take in `travel_times`, for the cases in which
#' the service standard is a distance (a water point within 1 km) or the
#' streets of the area are not mapped well enough to route on.
#'
#' The distance is the great-circle one on a sphere of radius 6,367,450 m,
#' the same that measures the streets of a network, and is the same on
#' every platform. A pair at distance 0 is a pair. Only the pairs within
#' `max_distance` are computed, so two large sets of points with a short
#' distance take little time and memory.
#'
#' @param origins,destinations The points: a `data.frame` with `id`, `lon`
#'   and `lat`, or a `POINT sf` with an `id` column.
#' @param max_distance The longest distance kept, in metres (the pairs
#'   with distance `<= max_distance`).
#'
#' @return A table with one row per pair, ordered as the origins and,
#'   within each, as the destinations: `from_id`, `to_id` and `distance`
#'   (metres).
#'
#'   `r doc_output_class()`
#'
#' @family facility location
#' @examples
#' data_path <- system.file("extdata/spo", package = "peabiru")
#' points <- read.csv(file.path(data_path, "spo_points.csv"))
#' near <- distance_matrix(points, points, max_distance = 800)
#' head(near)
#' # the two points with the most residents within 800 m of them
#' locate_facilities(NULL, points, points, model = "max_cover", n_facilities = 2,
#'                   cutoff = 800, demand_colname = "population",
#'                   travel_times = near)$sites
#' @export
distance_matrix <- function(origins, destinations, max_distance) {
  kind <- output_class()
  assert_number(max_distance, "max_distance", lower = 0, finite = TRUE)
  from <- location_points(origins, "origins", TRUE)
  to <- location_points(destinations, "destinations", TRUE)
  res <- raise_if_stopped(rs_distance_pairs(
    as.double(from$lon), as.double(from$lat), as.double(to$lon), as.double(to$lat),
    as.double(max_distance)
  ))
  as_output(new_df(list(
    from_id = from$id[res$from + 1L],
    to_id = to$id[res$to + 1L],
    distance = res$metres
  )), kind)
}

#' Share a budget of facilities among groups
#'
#' Given, for each group (a municipality, a district), what its first,
#' second, third... new facility would add, decides how many facilities
#' each group gets out of a common budget, and in what order.
#'
#' The curves come from [locate_facilities()] with `model = "max_cover"`
#' and `swap = FALSE`, one call per group asking for as many facilities as
#' there are candidates: the `marginal` column of `sites` is then what
#' each further facility adds, and it does not increase.
#'
#' With `criterion = "utilitarian"` each facility goes to the group where
#' the next one adds most. Because the gains of a group do not increase,
#' this covers the most demand possible, for the budget asked and for
#' every smaller one (marginal analysis; Fox, 1966). With `"maximin"` each
#' facility goes to the group with the lowest share of its demand covered
#' that still has a facility to open, which gives the highest possible
#' lowest share, again for every budget. Ties go to the larger gain, then
#' to the group listed first. The difference between the `covered` of the
#' two criteria for the same budget is what the fairer allocation costs
#' (Bertsimas, Farias and Trichakis, 2011).
#'
#' @param curves A `data.frame` with one row per facility of each group:
#'   `group` and `marginal` (what that facility adds, not negative), the
#'   rows of a group in the order of its facilities (or ordered by the
#'   column `order`, when there is one).
#' @param budget How many facilities to share out. Fewer are given when
#'   the groups have no more facilities that add anything.
#' @param groups A `data.frame` with one row per group: `group`, `total`
#'   (its demand) and, optionally, `covered` (the demand covered before
#'   any new facility; 0 without the column). `NULL` when the shares are
#'   not wanted; required by `"maximin"`. A group may have no row in
#'   `curves`.
#' @param criterion `"utilitarian"` or `"maximin"`.
#'
#' @return A list of two tables:
#' - `steps`: one row per facility, in the order they are given: `order`,
#'   `group`, `marginal`, `covered` (the demand covered in all the groups
#'   after it, what was covered before included) and `lowest_share` (the
#'   lowest share covered among the groups after it; `NA` without
#'   `groups`). The first `k` rows are the answer for a budget of `k`.
#' - `groups`: one row per group: `group`, `units` (how many facilities it
#'   gets), `gained`, `covered` and `share` (`NA` without `groups`).
#'
#'   `r doc_output_class()`
#'
#' @references
#' Bertsimas D, Farias VF, Trichakis N (2011). "The price of fairness."
#' *Operations Research*, **59**(1), 17-31. \doi{10.1287/opre.1100.0865}.
#'
#' Fox B (1966). "Discrete optimization via marginal analysis."
#' *Management Science*, **13**(3), 210-216. \doi{10.1287/mnsc.13.3.210}.
#'
#' @family facility location
#' @examples
#' curves <- data.frame(
#'   group = c("north", "north", "north", "south", "south"),
#'   marginal = c(500, 200, 50, 400, 300)
#' )
#' groups <- data.frame(group = c("north", "south"), covered = c(1000, 0),
#'                      total = c(2000, 1000))
#' allocate_budget(curves, budget = 3, groups = groups)$steps
#' allocate_budget(curves, budget = 3, groups = groups, criterion = "maximin")$steps
#' @export
allocate_budget <- function(curves, budget, groups = NULL, criterion = "utilitarian") {
  kind <- output_class()
  assert_count(budget, "budget", positive = TRUE)
  cm_assert_and("criterion", check_string(criterion),
                check_names(criterion, subset.of = c("utilitarian", "maximin")))
  if (!inherits(curves, "data.frame")) {
    stop("`curves` must be a 'data.frame'.", call. = FALSE)
  }
  curves <- as.data.frame(curves)
  cm_assert(check_names(names(curves), must.include = c("group", "marginal")), "curves")
  cm_assert(check_numeric(curves$marginal), "curves$marginal")
  if (anyNA(curves$group)) stop("`curves$group` must not have missing values.", call. = FALSE)
  if ("order" %in% names(curves)) {
    curves <- curves[order(match(curves$group, unique(curves$group)), curves$order), ]
  }
  own <- as.character(curves$group)
  if (is.null(groups)) {
    if (criterion == "maximin") {
      stop("Assertion on groups failed: must not be NULL when criterion is maximin ",
           "(the demand of each group, for its share covered).", call. = FALSE)
    }
    ids <- unique(own)
    covered <- rep(0, length(ids))
    total <- rep(NaN, length(ids))
  } else {
    if (!inherits(groups, "data.frame")) {
      stop("`groups` must be a 'data.frame'.", call. = FALSE)
    }
    groups <- as.data.frame(groups)
    cm_assert(check_names(names(groups), must.include = c("group", "total")), "groups")
    ids <- as.character(groups$group)
    if (anyDuplicated(ids) > 0) {
      stop("`groups` must not contain duplicated groups.", call. = FALSE)
    }
    absent <- setdiff(own, ids)
    if (length(absent) > 0) {
      stop("`curves` has groups that `groups` does not: ",
           paste0("\"", utils::head(absent, 10), "\"", collapse = ", "), ".", call. = FALSE)
    }
    covered <- if ("covered" %in% names(groups)) groups$covered else rep(0, length(ids))
    total <- groups$total
    cm_assert(check_numeric(covered, any.missing = FALSE), "groups$covered")
    cm_assert(check_numeric(total, any.missing = FALSE), "groups$total")
  }
  at <- match(own, ids)
  by_group <- order(at)  # stable: the rows of a group keep their order
  gains <- as.double(curves$marginal[by_group])
  up <- which(diff(at[by_group]) == 0 & diff(gains) > 1e-9 * abs(gains[-length(gains)]))
  if (length(up) > 0) {
    stop("The gains of a group may not increase: in group \"", ids[at[by_group][up[1]]],
         "\" a facility adds ", format(gains[up[1] + 1]), " after one that adds ",
         format(gains[up[1]]), " (the curves of max_cover solved with swap = FALSE do ",
         "not increase).", call. = FALSE)
  }
  res <- raise_if_stopped(rs_allocate_budget(
    gains, tabulate(at, length(ids)),
    as.double(covered), as.double(total), as.integer(budget), criterion
  ))
  gained <- vapply(seq_along(ids), function(g) sum(res$gain[res$group == g - 1L]), 0)
  known <- !is.null(groups)
  out <- list(
    steps = new_df(list(
      order = seq_along(res$group),
      group = ids[res$group + 1L],
      marginal = res$gain,
      covered = res$covered,
      lowest_share = if (known) res$lowest_share else rep(NA_real_, length(res$group))
    )),
    groups = new_df(list(
      group = ids,
      units = res$units,
      gained = gained,
      covered = as.double(covered) + gained,
      share = if (known) ifelse(total > 0, (covered + gained) / total, 1) else rep(NA_real_, length(ids))
    ))
  )
  as_output_list(out, kind, c("steps", "groups"))
}
