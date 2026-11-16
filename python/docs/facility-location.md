# Facility location

Travel times say how well a place is served today.
[`locate_facilities()`][peabiru.locate_facilities] turns the question
around: given the people to serve and the places where a facility could
open, *where should the next ones go?*
[`floating_catchment()`][peabiru.floating_catchment] measures the other side
of access, supply against the demand that competes for it. Both work on the
travel times of the network, by any mode, and both are peabiru's own
functions.

## The four models

`locate_facilities()` takes the demand points (with a weight each:
residents, pupils, patients), the candidate sites and, optionally, the
facilities that already exist, and solves one of four classic models:

| `model` | question | objective |
|---|---|---|
| `"max_cover"` | with `n_facilities` new facilities, how many people can be within `cutoff` minutes of one? | demand covered (to maximise) |
| `"set_cover"` | how few facilities put everyone within `cutoff` minutes? | number of new sites |
| `"p_median"` | where do `n_facilities` facilities make the average trip shortest? | demand-weighted minutes to the nearest facility |
| `"p_center"` | where do they make the worst trip shortest? | minutes of the demand point farthest from its nearest facility |

A demand point is *covered* by a site at most `cutoff` minutes away (the
cutoff included). The `existing` facilities count first: only the demand
they leave uncovered, or the travel time they leave, drives the choice of
the new sites, and they are never closed or moved.

## A worked example: two new schools

The sample network has 27 points with their population; six of them have a
school. Where would two more schools put the most residents within a
12-minute walk of one?

```python
import peabiru

network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
points = peabiru.sample_points()
schools = points[points["schools"] > 0]
others = points[points["schools"] == 0]

found = peabiru.locate_facilities(
    network, points, others, existing=schools,
    model="max_cover", n_facilities=2, cutoff=12,
    demand_colname="population", mode="WALK", n_threads=2,
)
print(found.sites[["id", "order", "marginal", "cumulative"]].to_string(index=False))
```

```text
             id  order  marginal  cumulative
89a8100c03bffff      1    5658.0     43534.0
89a8100c383ffff      2    2683.0     46217.0
```

The result has four parts. `sites` lists the new sites by what each adds:
`marginal` is the demand it newly covers and `cumulative` the demand covered
so far, existing schools included. `summary` has the totals:

```python
s = found.summary
print(f"covered today:  {s['covered_before']:.0f} of {s['total_demand']:.0f} residents "
      f"({100 * s['covered_before'] / s['total_demand']:.1f}%)")
print(f"greedy:         {s['objective_greedy']:.0f}")
print(f"after {s['n_swaps']} swap:   {s['covered']:.0f} "
      f"({100 * s['covered'] / s['total_demand']:.1f}%)")
```

```text
covered today:  37876 of 46273 residents (81.9%)
greedy:         46040
after 1 swap:   46217 (99.9%)
```

`candidates` has one row per candidate with its `potential`: the residents
not covered today that a school there would cover *alone*. It is the map to
draw before choosing anything, since it shows every good place and not only
the winners:

```python
best = found.candidates.sort_values("potential", ascending=False).head(4)
print(best[["id", "potential", "chosen"]].to_string(index=False))
```

```text
             id  potential  chosen
89a8100c033ffff     5764.0   False
89a8100c03bffff     5658.0    True
89a8100c007ffff     4829.0   False
89a8100c00fffff     3936.0   False
```

The candidate with the largest potential is not in the solution: the two
sites chosen overlap less with each other than it does with either. That is
the difference between ranking sites one by one and choosing a set.

`demand` has one row per demand point: its weight, the nearest open
facility (existing or new), the minutes to it and whether it is covered.

```python
print(found.demand[["id", "demand", "facility_id", "travel_time", "covered"]].head(4).to_string(index=False))
```

```text
             id  demand     facility_id  travel_time  covered
89a8100c00fffff  3648.0 89a8100c03bffff          8.0     True
89a8100c007ffff  3936.0 89a8100c03bffff          9.0     True
89a8100c077ffff  6514.0 89a8100c077ffff          0.0     True
89a8100c03bffff  3670.0 89a8100c03bffff          0.0     True
```

## How the models are solved

By heuristics, with no integer programming solver:

1. **Greedy construction.** Sites are opened one at a time, each the one
   that improves the objective most given the ones already open. For
   maximal covering this covers at least 63% (1 - 1/e) of the most that any
   choice of `n_facilities` sites could cover of the demand not covered
   today (Nemhauser, Wolsey and Fisher, 1978); in practice it is usually
   much closer.
2. **Vertex substitution** (`swap=True`, the default; Teitz and Bart,
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
  facility within `max_trip_duration` is charged `unreachable_time` minutes
  (by default twice the longest travel time of the table), so that serving
  a point always beats shortening the trip of another. A point that no site
  at all can serve is left out of the objective and reported in
  `summary["unservable_demand"]`.
- Ties go to the candidate listed first, and the result does not depend on
  `n_threads`.

## How close to the best possible

`summary["upper_bound"]` answers the question for maximal covering without solving
anything else: no choice of `n_facilities` sites can cover more demand than it,
so the optimum lies between `covered` and `upper_bound`, and
`covered / upper_bound` is a floor on the share of the optimum that the
solution reaches. The bound comes from the shape of the problem: the demand
covered by any set of sites, plus the `n_facilities` largest gains of the sites
outside it, is at least what the best set covers. It is taken twice, at the
existing facilities alone and at the solution, and the smaller of the two is
reported (never more than the demand some site can cover). When the two
numbers are equal the solution is proven optimal; when they are not, the
solution may still be optimal, as in the example, where the bound is
46,273 residents and the optimum 46,217: the bound is a ceiling, not the
optimum itself.

```python
print(s["covered"], s["upper_bound"])
```

```text
46217.0 46273.0
```

## The whole curve in one call

How much would a third school add, or a fourth? With `swap=False` the
sites of maximal covering are nested: the first `k` rows of `sites` are the
answer for `k` facilities. Asking for as many facilities as there are
candidates therefore gives the demand covered for every number of
facilities (`min_gain` stops the list where a site no longer pays):

```python
curve = peabiru.locate_facilities(
    network, points, others, existing=schools,
    model="max_cover", n_facilities=len(others), cutoff=12, swap=False,
    demand_colname="population", mode="WALK", n_threads=2,
)
print(curve.sites[["order", "marginal", "cumulative"]].to_string(index=False))
```

```text
 order  marginal  cumulative
     1    5764.0     43640.0
     2    2400.0     46040.0
     3     177.0     46217.0
     4      56.0     46273.0
```

With swaps the sites for one number of facilities need not include those
for a smaller number, so the curve is then one call per number. The curves
of several areas, stored like this, are what it takes to share a budget of
facilities among them.

## Facilities with a capacity

Maximal covering assumes that one facility serves everyone within the
cutoff. When a facility serves only so many people, a place with much
demand needs several, and `capacity` says how many: the model is then the
capacitated maximal covering (Current and Storbeck, 1988). `capacity` is a
number, or the name of a column of the candidates (and of the existing
facilities) with the capacity at each site.

```python
full = peabiru.locate_facilities(
    network, points, others, existing=schools,
    model="max_cover", n_facilities=4, cutoff=12, capacity=3000,
    demand_colname="population", mode="WALK", n_threads=2,
)
print(full.sites[["id", "order", "marginal", "cumulative"]].to_string(index=False))
print(full.summary["covered_before"], full.summary["upper_bound"])
```

```text
             id  order  marginal  cumulative
89a8100c00fffff      1    3000.0     19623.0
89a8100c00fffff      2    3000.0     22623.0
89a8100c00fffff      3    3000.0     25623.0
89a8100c00fffff      4    3000.0     28623.0
16623.0 28623.0
```

With 3,000 places each, the six schools serve 16,623 residents of the
37,876 within 12 minutes of one, and the four new schools all go to the
same place, where the most unserved residents are.

What changes with a capacity:

- A candidate may open more than once: `sites` has one row per facility
  opened, `n_facilities` counts facilities and may exceed the number of
  candidates, and `candidates` gains `units`.
- Facilities open one at a time, each where it would serve the most demand
  not yet served, and take that demand from the nearest points first. A
  demand point may be served in part, or by more than one facility:
  `demand` gains `served`, the share of its demand that is served, while
  `covered` still says whether it is within the cutoff of a facility,
  with room or not.
- The existing facilities have capacities too and serve first, each pair of
  facility and demand point in the order of their travel times.
- `swap` has no effect, and `upper_bound` is a ceiling on what any choice
  of `n_facilities` facilities could serve, given what the existing ones serve.

## The other models

How few schools would put every resident within a 10-minute walk, starting
from none?

```python
cover = peabiru.locate_facilities(
    network, points, points, model="set_cover", cutoff=10,
    demand_colname="population", mode="WALK", n_threads=2,
)
print(len(cover.sites), "sites;", cover.summary["unservable_demand"], "residents out of reach")
```

```text
7 sites; 0.0 residents out of reach
```

Covering every last resident is often what costs most. With `coverage` the
model stops at a share of the demand, and the sites are then chosen by the
demand each newly covers (the weights count, which they do not when
everything must be covered):

```python
part = peabiru.locate_facilities(
    network, points, points, model="set_cover", cutoff=10, coverage=0.9,
    demand_colname="population", mode="WALK", n_threads=2,
)
share = part.summary["covered"] / part.summary["total_demand"]
print(len(part.sites), "sites;", f"{100 * share:.1f}% of the residents")
```

```text
4 sites; 94.6% of the residents
```

Four sites cover 94.6% of the residents; the last 5.4% take three more.
When the target cannot be reached, all the demand that can be covered is.

Two sites that make the average walk shortest, and two that make the
longest walk shortest:

```python
for model in ("p_median", "p_center"):
    r = peabiru.locate_facilities(
        network, points, points, model=model, n_facilities=2,
        demand_colname="population", mode="WALK", max_trip_duration=60, n_threads=2,
    )
    mean = (r.demand["demand"] * r.demand["travel_time"]).sum() / r.demand["demand"].sum()
    print(f"{model}: {', '.join(r.sites['id'])}; "
          f"mean {mean:.1f} min, longest {r.demand['travel_time'].max():.0f} min")
```

```text
p_median: 89a8100c027ffff, 89a8100c03bffff; mean 9.7 min, longest 32 min
p_center: 89a8100c02fffff, 89a8100c067ffff; mean 13.3 min, longest 19 min
```

The p-median pair gives the shorter average and the p-center pair the
shorter worst case: the two models answer different questions.

Any mode works, since the travel times come from
[`travel_time_matrix()`][peabiru.travel_time_matrix]: pass `mode`,
`departure_datetime` and any other routing argument.

```python
import datetime as dt

transit = peabiru.locate_facilities(
    network, points, points, model="p_median", n_facilities=3,
    demand_colname="population", mode=["WALK", "TRANSIT"],
    departure_datetime=dt.datetime(2019, 5, 13, 14, 0), seed=1,
    max_trip_duration=60, n_threads=2,
)
print(list(transit.sites["id"]))
```

```text
['89a8100c03bffff', '89a8100c1dbffff', '89a8100c15bffff']
```

## Supply against demand: the floating catchment area

Being near a school is not the same as having a place in it.
`floating_catchment()` computes the two-step floating catchment area (Luo
and Wang, 2003):

1. each facility's supply is divided by the demand within its catchment;
2. each demand point adds up the ratios of the facilities within its
   catchment.

The result is supply per unit of demand at each demand point: here, schools
per resident within a 20-minute walk.

```python
fca = peabiru.floating_catchment(
    network, points, schools, cutoff=20,
    demand_colname="population", supply_colname="schools", mode="WALK", n_threads=2,
)
print(fca.supply[["id", "supply", "catchment_demand"]].to_string(index=False))
per_10k = 10_000 * fca.demand["accessibility"]
print(f"schools per 10,000 residents: from {per_10k.min():.2f} to {per_10k.max():.2f}")
print(f"shared out: {(fca.demand['demand'] * fca.demand['accessibility']).sum():.1f} schools")
```

```text
             id  supply  catchment_demand
89a8100c077ffff     1.0           22033.0
89a8100c1dbffff     2.0           17501.0
89a8100c39bffff     1.0           10593.0
89a8100c1cbffff     1.0           21263.0
89a8100c153ffff     1.0           22664.0
89a8100c14fffff     1.0           11967.0
schools per 10,000 residents: from 0.45 to 3.34
shared out: 7.0 schools
```

The index shares out the supply: the sum over the demand points of demand
times `accessibility` is the supply of the facilities that have any demand
in their catchment.

With `decay_function="step"` a pair counts in full up to `cutoff` minutes
and not at all beyond. Any other decay function gives the *enhanced* form
(Luo and Qi, 2009), in which nearer pairs weigh more, in both steps. The
functions and their `decay_value` are those of
[`accessibility()`](accessibility.md); the weight falls continuously with
travel time, where Luo and Qi used a few travel-time zones.

```python
smooth = peabiru.floating_catchment(
    network, points, schools, cutoff=20, decay_function="logistic", decay_value=5,
    demand_colname="population", supply_colname="schools", mode="WALK", n_threads=2,
)
per_10k = 10_000 * smooth.demand["accessibility"]
print(f"schools per 10,000 residents: from {per_10k.min():.2f} to {per_10k.max():.2f}")
```

```text
schools per 10,000 residents: from 0.69 to 2.74
```

## From a table of travel times

Both functions take a table of travel times instead of a network: `from_id`
(demand point), `to_id` (site) and the minutes in `travel_time_pXX` or
`travel_time`, as `travel_time_matrix()` returns it. A pair that is absent
is unreachable. The points then need only their `id`, so the same matrix
can serve several models, or travel times from another source can be used.

```python
import pandas as pd

sites = pd.concat([schools, others])
times = peabiru.travel_time_matrix(
    network, points, sites, mode="WALK", max_trip_duration=12, n_threads=2,
)
again = peabiru.locate_facilities(
    None, points[["id", "population"]], others[["id"]], existing=schools[["id"]],
    model="max_cover", n_facilities=2, cutoff=12, demand_colname="population",
    travel_times=times,
)
print(list(again.sites["id"]) == list(found.sites["id"]))
```

```text
True
```

The models do not depend on the unit of the table. It may hold any cost
that is not negative, such as a straight-line or network distance in
metres where the service standard is a distance: `cutoff`, `unreachable_time` and
the `travel_time` column of the result are then in that unit.

`distance_matrix()` makes such a table from the straight-line distances between two sets
of points: every pair at most so many metres apart, with no network. Here,
the two sites that put the most residents within 500 m:

```python
near = peabiru.distance_matrix(points, points, max_distance=500)
print(near.head(3).to_string(index=False))
by_distance = peabiru.locate_facilities(
    None, points, points, model="max_cover", n_facilities=2, cutoff=500,
    demand_colname="population", travel_times=near,
)
print(by_distance.sites[["id", "order", "marginal", "cumulative"]].to_string(index=False))
```

```text
        from_id           to_id   distance
89a8100c00fffff 89a8100c00fffff   0.000000
89a8100c00fffff 89a8100c007ffff 351.804965
89a8100c00fffff 89a8100c077ffff 340.917934
             id  order  marginal  cumulative
89a8100c03bffff      1   19490.0     19490.0
89a8100c037ffff      2   13847.0     33337.0
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

- Church, R. and ReVelle, C. (1974). The maximal covering location problem.
  *Papers of the Regional Science Association*, 32, 101-118.
  [doi:10.1007/BF01942293](https://doi.org/10.1007/BF01942293)
- Current, J. R. and Storbeck, J. E. (1988). Capacitated covering models.
  *Environment and Planning B: Planning and Design*, 15(2), 153-163.
  [doi:10.1068/b150153](https://doi.org/10.1068/b150153)
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
