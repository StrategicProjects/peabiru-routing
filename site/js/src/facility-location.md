# Facility location

Travel times say how well a place is served today. `locateFacilities()`
turns the question around: given the people to serve and the places where a
facility could open, *where should the next ones go?* `floatingCatchment()`
measures the other side of access, supply against the demand that competes
for it. Both work on the travel times of the network, by any mode; they are
peabiru's own functions, the same as `locate_facilities()` and
`floating_catchment()` in R and Python, with the same results.

## The four models

`locateFacilities(network, demand, candidates, options)` takes the demand
points (with a weight each: residents, pupils, patients), the candidate
sites and, in `existing`, the facilities already open, and solves one of
four classic models:

| `model` | question | objective |
|---|---|---|
| `"max_cover"` | with `nFacilities` new facilities, how many people can be within `cutoff` minutes of one? | demand covered (to maximise) |
| `"set_cover"` | how few facilities put everyone within `cutoff` minutes? | number of new sites |
| `"p_median"` | where do `nFacilities` facilities make the average trip shortest? | demand-weighted minutes to the nearest facility |
| `"p_center"` | where do they make the worst trip shortest? | minutes of the demand point farthest from its nearest facility |

A demand point is *covered* by a site at most `cutoff` minutes away (the
cutoff included). The `existing` facilities count first: only the demand
they leave uncovered, or the travel time they leave, drives the choice of
the new sites, and they are never closed or moved.

## A worked example: two new schools

The sample network has 27 points with their population; six of them have a
school. Where would two more schools put the most residents within a
12-minute walk of one?

```js
{{#include ../examples/facility-location.mjs:cover}}
```

```text
{{#include ../examples/output/facility-location.txt:1:4}}
```

The result has four parts, each a table as an object of columns (and
`summary`, an object of numbers). `sites` lists the new sites by what each
adds: `marginal` is the demand it newly covers and `cumulative` the demand
covered so far, existing schools included. `candidates` has one row per
candidate with its `potential`: the residents not covered today that a
school there would cover *alone*. It is the map to draw before choosing
anything, since it shows every good place and not only the winners:

```js
{{#include ../examples/facility-location.mjs:potential}}
```

```text
{{#include ../examples/output/facility-location.txt:5:8}}
```

The candidate with the largest potential is not in the solution: the two
sites chosen overlap less with each other than it does with either. That is
the difference between ranking sites one by one and choosing a set.
`demand` has one row per demand point: its weight, the nearest open
facility (`facility_id`), the minutes to it (`travel_time`) and whether it
is `covered`.

## How the models are solved

By heuristics, with no integer programming solver:

1. **Greedy construction.** Sites are opened one at a time, each the one
   that improves the objective most given the ones already open. For
   maximal covering this covers at least 63% (1 - 1/e) of the most that any
   choice of `nFacilities` sites could cover of the demand not covered
   today (Nemhauser, Wolsey and Fisher, 1978); in practice it is usually
   much closer.
2. **Vertex substitution** (`swap: true`, the default; Teitz and Bart,
   1968). An open new site is swapped for a closed candidate while that
   improves the objective, taking the best swap of all at each step. The
   result is never worse than the greedy one, and `summary` reports both
   (`objective_greedy`, `objective`, `n_swaps`).

In the example the greedy pair covers 46,040 residents and one swap takes
it to 46,217, which is the optimum: an exhaustive search over the 210 pairs
of candidates finds nothing better. That is not guaranteed in general; the
solutions are good and reproducible, not proven optimal.

Details by model:

- `"set_cover"` opens the site that covers the most demand points not yet
  covered until every point some site can cover is covered, then drops the
  sites the others make redundant.
- `"p_center"` compares the largest travel time and, on ties, the
  demand-weighted sum, in the greedy step and in the swaps.
- In `"set_cover"` and `"p_center"` a point with weight 0 is not demand,
  and the others count the same whatever their weight.
- In `"p_median"` and `"p_center"`, a demand point that reaches no open
  facility within `maxTripDuration` is charged `unreachableTime` minutes
  (by default twice the longest travel time of the table). A point that no
  site at all can serve is left out of the objective and reported in
  `summary.unservable_demand`.
- Ties go to the candidate listed first.

The travel times are routed by the network's workers, like any matrix; the
solver itself runs on the calling thread and is not interrupted by
`signal`.

## The other models

```js
{{#include ../examples/facility-location.mjs:models}}
```

```text
{{#include ../examples/output/facility-location.txt:9:11}}
```

The p-median pair gives the shorter average and the p-center pair the
shorter worst case: the two models answer different questions. Any mode
works, since the travel times come from `travelTimeMatrix()`: pass `mode`,
`departureDatetime` and any other routing option (all but `percentiles`,
which is `percentile` here, and `outputDir`).

## Supply against demand: the floating catchment area

Being near a school is not the same as having a place in it.
`floatingCatchment(network, demand, supply, options)` computes the two-step
floating catchment area (Luo and Wang, 2003): each facility's supply is
divided by the demand within its catchment, and each demand point adds up
the ratios of the facilities within its catchment. The result is supply per
unit of demand at each demand point: here, schools per resident within a
20-minute walk.

```js
{{#include ../examples/facility-location.mjs:catchment}}
```

```text
{{#include ../examples/output/facility-location.txt:12:19}}
```

The index shares out the supply: the sum over the demand points of demand
times `accessibility` is the supply of the facilities that have any demand
in their catchment.

With `decayFunction: "step"` a pair counts in full up to `cutoff` minutes
and not at all beyond. Any other decay function gives the *enhanced* form
(Luo and Qi, 2009), in which nearer pairs weigh more, in both steps. The
functions and their `decayValue` are those of `accessibility()`; the weight
falls continuously with travel time, where Luo and Qi used a few
travel-time zones.

## From a table of travel times

Both functions take a table of travel times in `travelTimes` instead of a
network: `from_id` (demand point), `to_id` (site) and the minutes in
`travel_time_pXX` or `travel_time`, as `travelTimeMatrix()` returns it. A
pair that is absent is unreachable. The points then need only their `id`.

```js
{{#include ../examples/facility-location.mjs:table}}
```

```text
{{#include ../examples/output/facility-location.txt:20}}
```

## What the models do not see

The models choose sites by travel time and demand alone. They do not know
the capacity or the cost of a facility, whether land is available at a
candidate site, the demand that comes from outside the study area, or how
well the people already covered are served. Maximal covering treats 11
minutes and 1 minute alike, and p-median may leave a few people very far to
shorten the average: comparing two models on the same data is often more
informative than either alone.

## References

- Church, R. and ReVelle, C. (1974). The maximal covering location problem.
  *Papers of the Regional Science Association*, 32, 101-118.
  [doi:10.1007/BF01942293](https://doi.org/10.1007/BF01942293)
- Hakimi, S. L. (1964). Optimum locations of switching centers and the
  absolute centers and medians of a graph. *Operations Research*, 12(3),
  450-459. [doi:10.1287/opre.12.3.450](https://doi.org/10.1287/opre.12.3.450)
- Luo, W. and Qi, Y. (2009). An enhanced two-step floating catchment area
  (E2SFCA) method for measuring spatial accessibility to primary care
  physicians. *Health & Place*, 15(4), 1100-1107.
  [doi:10.1016/j.healthplace.2009.06.002](https://doi.org/10.1016/j.healthplace.2009.06.002)
- Luo, W. and Wang, F. (2003). Measures of spatial accessibility to health
  care in a GIS environment: synthesis and a case study in the Chicago
  region. *Environment and Planning B: Planning and Design*, 30(6), 865-884.
  [doi:10.1068/b29120](https://doi.org/10.1068/b29120)
- Nemhauser, G. L., Wolsey, L. A. and Fisher, M. L. (1978). An analysis of
  approximations for maximizing submodular set functions - I. *Mathematical
  Programming*, 14, 265-294.
  [doi:10.1007/BF01588971](https://doi.org/10.1007/BF01588971)
- Teitz, M. B. and Bart, P. (1968). Heuristic methods for estimating the
  generalized vertex median of a weighted graph. *Operations Research*,
  16(5), 955-961. [doi:10.1287/opre.16.5.955](https://doi.org/10.1287/opre.16.5.955)
- Toregas, C., Swain, R., ReVelle, C. and Bergman, L. (1971). The location
  of emergency service facilities. *Operations Research*, 19(6), 1363-1373.
  [doi:10.1287/opre.19.6.1363](https://doi.org/10.1287/opre.19.6.1363)
