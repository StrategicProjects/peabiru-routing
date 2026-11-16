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

## How close to the best possible

`summary.upper_bound` answers the question for maximal covering without solving
anything else: no choice of `nFacilities` sites can cover more demand than it,
so the optimum lies between `covered` and `upper_bound`, and
`covered / upper_bound` is a floor on the share of the optimum that the
solution reaches. The bound comes from the shape of the problem: the demand
covered by any set of sites, plus the `nFacilities` largest gains of the sites
outside it, is at least what the best set covers. It is taken twice, at the
existing facilities alone and at the solution, and the smaller of the two is
reported (never more than the demand some site can cover). When the two
numbers are equal the solution is proven optimal; when they are not, the
solution may still be optimal, as in the example, where the bound is
46,273 residents and the optimum 46,217: the bound is a ceiling, not the
optimum itself.

## The whole curve in one call

How much would a third school add, or a fourth? With `swap: false` the
sites of maximal covering are nested: the first `k` rows of `sites` are the
answer for `k` facilities. Asking for as many facilities as there are
candidates therefore gives the demand covered for every number of
facilities (`minGain` stops the list where a site no longer pays):

```js
{{#include ../examples/facility-location.mjs:bound}}
```

```text
{{#include ../examples/output/facility-location.txt:21:25}}
```

With swaps the sites for one number of facilities need not include those
for a smaller number, so the curve is then one call per number. The curves
of several areas are what it takes to share a budget of facilities among
them, which is the next section.

## Sharing a budget among areas

A state has so many facilities to build and many municipalities to build
them in. With the curve of each area, `allocateBudget()` says how many go to each, and
in what order, by one of two criteria:

- `criterion: "utilitarian"`: each facility goes where the next one adds most. Since what
  a further facility adds in an area never increases, this covers the most
  demand possible, for the budget asked and for every smaller one
  (marginal analysis; Fox, 1966).
- `criterion: "maximin"`: each facility goes to the area with the lowest share of its
  demand covered (that still has a facility worth opening), which gives
  the highest possible lowest share.

Here the sample points are split in two halves, east and west, each with
its curve for schools within an 8-minute walk (`swap: false`, `nFacilities` as many as
the candidates), and three schools are shared out:

```js
{{#include ../examples/facility-location.mjs:budget}}
```

```text
{{#include ../examples/output/facility-location.txt:35:36}}
```

The two criteria agree on the first school and part ways on the second: the
most coverage takes it east again, the fairer rule sends it west, where a
smaller share of the residents is covered. With three schools the fairer
allocation covers 527 residents fewer (41,632 against 42,159) and raises the
lowest share from 85.4% to 86.1%: that difference is the price of fairness
(Bertsimas, Farias and Trichakis, 2011), and the `steps` of the two
criteria give it for every budget.

### Weights, priority tiers and areas left out

Three columns of `groups` change who comes first, each named in an
argument:

- `weights`: how much the demand of each area counts, say 2 where the need
  is most urgent and 1 elsewhere. The first criterion then multiplies what
  a facility adds by the weight of its area (and still gives the largest
  weighted sum for every budget, since the weighted gains do not increase
  either); the second divides the share covered by the weight.
- `tiers`: a whole number per area, the lowest first. A facility only goes
  to an area when no area of a lower tier has one left that adds anything,
  and within the tier the criterion decides: with `"maximin"`, the worst
  covered among the most urgent first.
- `eligible`: `false` for the areas that get no facility. They stay in the
  result, with zero facilities, and count in `covered` and in
  `lowest_share`.

Nothing in the result is weighted: the gains, the demand covered and the
shares are residents, so the cost of a rule is the difference to the run
without it. The `groups` table of the result has the `priority` of each
area, the order in which the areas get their first facility, which sums up
the criterion and the rules in one number per area.

```js
{{#include ../examples/facility-location.mjs:rules}}
```

```text
{{#include ../examples/output/facility-location.txt:37}}
```

With the west counting double, the second school goes west: its 2,185
residents weigh 4,370, more than the 2,485 of a second school in the east.
The allocation is the one of the fairer criterion above, 527 residents
short of the most coverage. With the west in the first tier the three
schools go there, which covers all its residents and leaves the east where
it was, at 52.1%: 35,593 residents covered in all against 42,159, which is
what that rule costs. `priority` has the west first and the east with no
facility.

## Facilities with a capacity

Maximal covering assumes that one facility serves everyone within the
cutoff. When a facility serves only so many people, a place with much
demand needs several, and `capacity` says how many: the model is then the
capacitated maximal covering (Current and Storbeck, 1988). `capacity` is a
number, or the name of a column of the candidates (and of the existing
facilities) with the capacity at each site.

```js
{{#include ../examples/facility-location.mjs:capacity}}
```

```text
{{#include ../examples/output/facility-location.txt:27:31}}
```

With 3,000 places each, the six schools serve 16,623 residents of the
37,876 within 12 minutes of one, and the four new schools all go to the
same place, where the most unserved residents are.

What changes with a capacity:

- A candidate may open more than once: `sites` has one row per facility
  opened, `nFacilities` counts facilities and may exceed the number of
  candidates, and `candidates` gains `units`.
- Facilities open one at a time, each where it would serve the most demand
  not yet served, and take that demand from the nearest points first. A
  demand point may be served in part, or by more than one facility:
  `demand` gains `served`, the share of its demand that is served, while
  `covered` still says whether it is within the cutoff of a facility,
  with room or not.
- `flows` says who serves whom: one row per pair of demand point and
  facility with demand served between them, existing facilities included.
- The existing facilities have capacities too and serve first, each pair of
  facility and demand point in the order of their travel times.
- `swap` has no effect, and `upper_bound` is a ceiling on what any choice
  of `nFacilities` facilities could serve, given what the existing ones serve.

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

Covering every last resident is often what costs most. With `coverage` the
model stops at a share of the demand, and the sites are then chosen by the
demand each newly covers (the weights count, which they do not when
everything must be covered):

```js
{{#include ../examples/facility-location.mjs:partial}}
```

```text
{{#include ../examples/output/facility-location.txt:26:26}}
```

Four sites cover 94.6% of the residents; the last 5.4% take three more.
When the target cannot be reached, all the demand that can be covered is.

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
{{#include ../examples/output/facility-location.txt:20:20}}
```

The models do not depend on the unit of the table. It may hold any cost
that is not negative, such as a straight-line or network distance in
metres where the service standard is a distance: `cutoff`, `unreachableTime` and
the `travel_time` column of the result are then in that unit.

`distanceMatrix()` makes such a table from the straight-line distances between two sets
of points: every pair at most so many metres apart, with no network. Here,
the two sites that put the most residents within 500 m:

```js
{{#include ../examples/facility-location.mjs:distance}}
```

```text
{{#include ../examples/output/facility-location.txt:32:34}}
```

## What the models do not see

The models choose sites by travel time and demand alone. They do not know
the cost of a facility (nor its capacity, except in maximal covering with
`capacity`), whether land is available at a
candidate site, the demand that comes from outside the study area, or how
well the people already covered are served. Maximal covering treats 11
minutes and 1 minute alike, and p-median may leave a few people very far to
shorten the average: comparing two models on the same data is often more
informative than either alone.

## References

- Bertsimas, D., Farias, V. F. and Trichakis, N. (2011). The price of
  fairness. *Operations Research*, 59(1), 17-31.
  [doi:10.1287/opre.1100.0865](https://doi.org/10.1287/opre.1100.0865)
- Church, R. and ReVelle, C. (1974). The maximal covering location problem.
  *Papers of the Regional Science Association*, 32, 101-118.
  [doi:10.1007/BF01942293](https://doi.org/10.1007/BF01942293)
- Current, J. R. and Storbeck, J. E. (1988). Capacitated covering models.
  *Environment and Planning B: Planning and Design*, 15(2), 153-163.
  [doi:10.1068/b150153](https://doi.org/10.1068/b150153)
- Fox, B. (1966). Discrete optimization via marginal analysis. *Management
  Science*, 13(3), 210-216.
  [doi:10.1287/mnsc.13.3.210](https://doi.org/10.1287/mnsc.13.3.210)
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
