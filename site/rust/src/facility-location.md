# Facility location

`locate_facilities` chooses where to open facilities, and
`floating_catchment` compares supply with demand around each place. They
are what `locate_facilities()` and `floating_catchment()` (in JavaScript,
`locateFacilities()` and `floatingCatchment()`) compute in the packages,
and they are peabiru's own functions.

Both work on a `CostTable`: the minutes from each demand point (row) to
each site (column), a pair that is absent being unreachable. They never
look at how the table was computed, so the travel times can come from a
street matrix, a public transport matrix, or anything else:

- `CostTable::from_minutes(n_demand, n_sites, minutes)` takes the whole
  minutes of a matrix of this crate, `None` for an unreachable pair: the
  slice `StreetMatrix::as_slice()` gives when the demand points are the
  origins and the sites the destinations, or a `TransitMatrix` computed for
  one percentile;
- `CostTable::from_pairs(n_demand, n_sites, demand, site, minutes)` takes
  the reachable pairs one by one, and `CostTable::from_dense` a full matrix
  of `f64` (NaN or infinite: unreachable).

## The four models

`locate_facilities(&costs, &demand, &params)` takes the weight of each
demand point (residents, pupils, patients) and a `LocationParams`:

| `LocationModel` | question | objective |
|---|---|---|
| `MaxCover` | with `n_facilities` new facilities, how much demand can be within `cutoff` minutes of one? | demand covered (to maximise) |
| `SetCover` | how few facilities put every demand point within `cutoff` minutes? | number of new sites |
| `PMedian` | where do `n_facilities` facilities make the average trip shortest? | demand-weighted minutes to the nearest facility |
| `PCenter` | where do they make the worst trip shortest? | minutes of the demand point farthest from its nearest facility |

A demand point is *covered* by a site at most `cutoff` minutes away (the
cutoff included). The sites listed in `existing` (column indices) are
already open and count first: only the demand they leave uncovered, or the
travel time they leave, drives the choice, and they are never closed or
moved. Every other site of the table is a candidate.

The `LocationSolution` holds what a caller needs to draw a map:

- `sites`, the new sites ordered by what each adds, with `marginal` (demand
  newly covered, or by how much the objective falls) and `cumulative`
  (demand covered so far, existing facilities included, or the objective so
  far);
- `potential`, per site, what it would add as the only new facility: for
  the covering models, the demand not covered by the existing facilities
  that it would cover alone;
- `assigned_site` and `assigned_minutes`, per demand point, the nearest
  open facility;
- `objective_before` (existing facilities only), `objective_greedy` and
  `objective`; `covered_before` and `covered` for the covering models;
- `unservable` and `unservable_demand`: the demand points with weight that
  no site of the table can serve (within the cutoff, or at all for
  `PMedian` and `PCenter`), which are left out of the objectives.

## How the models are solved

By heuristics, with no integer programming solver and no dependency:

1. **Greedy construction.** Sites are opened one at a time, each the one
   that improves the objective most given the ones already open. For
   maximal covering the candidates are kept in a queue with their last
   known gain, which can only fall as sites open, and only the top of the
   queue is recomputed. The greedy solution covers at least 63% (1 - 1/e)
   of the most that any choice of `n_facilities` sites could cover of the
   demand the existing facilities leave uncovered (Nemhauser, Wolsey and
   Fisher, 1978).
2. **Vertex substitution** (`swap`, on by default; Teitz and Bart, 1968).
   An open new site is swapped for a closed candidate while that improves
   the objective, taking the best swap of all at each step, and each swap
   accepted is confirmed by recomputing the objective. The result is never
   worse than the greedy one.

Details by model:

- `SetCover` opens the site that covers the most demand points not yet
  covered until every point some site can cover is covered, then drops the
  sites the others make redundant, the last chosen first.
- `PCenter` compares the largest travel time and, on ties, the
  demand-weighted sum, in the greedy step and in the swaps.
- In `SetCover` and `PCenter` a point with weight 0 is not demand, and the
  others count the same whatever their weight.
- In `PMedian` and `PCenter`, a demand point that reaches no open facility
  is charged `unreachable_time` minutes (by default twice the longest time
  of the table), so that serving a point always beats shortening the trip
  of another.
- `MaxCover` stops when the best site left would cover nothing (or less
  than `min_gain`), and the travel-time models when no site improves the
  objective: fewer than `n_facilities` sites may come back.

The solutions are good and reproducible, not proven optimal. Ties go to the
lowest site index; the candidates are evaluated in parallel only where each
one's value is computed on its own, and every sum is taken in a fixed
order, so the result does not depend on `n_threads`. The call's
`CancelToken` is checked between candidates. A greedy round and a pass of
swaps of the travel-time models cost one pass over the demand points per
candidate.

The crate's tests compare the heuristics with an exhaustive search on 300
random instances of up to 12 sites and 3 new facilities: the greedy bound
holds, the improvement is never worse than greedy, and one new facility is
always optimal.

## Floating catchment area

`floating_catchment(&costs, &demand, &supply, &params)` computes the
two-step floating catchment area (Luo and Wang, 2003): each facility's
supply is divided by the demand within its catchment, and each demand point
adds up the ratios of the facilities within its catchment. The
`FloatingCatchment` holds the `index` per demand point (supply per unit of
demand), and per facility its `ratio` and `catchment_demand`; the sum of
demand times index equals the total supply less `unreached_supply`, the
supply of the facilities whose catchment holds no demand.

With `DecayFunction::Step` (the default) a pair counts in full up to
`cutoff` minutes and not at all beyond. Any other `DecayFunction` gives the
enhanced form (Luo and Qi, 2009), in which nearer pairs weigh more, in both
steps: the weights are those of [accessibility](accessibility.md), computed
from the minutes of the table, and every pair of the table counts with its
weight, so the table should reach as far as the weights matter. The weight
falls continuously with travel time, where Luo and Qi used a few
travel-time zones.

## Example

Two new schools on the sample network, so that the most residents are
within a 12-minute walk of a school, and schools per resident within a
20-minute walk:

```rust,ignore
{{#include ../../../crates/peabiru-core/examples/facility_location.rs}}
```

```text
covered today: 37876 of 46273 residents; greedy: 46040; after 1 swap(s): 46217
site 1: 89a8100c03bffff (+5658 residents, 43534 so far)
site 2: 89a8100c383ffff (+2683 residents, 46217 so far)
largest potential: 89a8100c033ffff (5764 residents)
89a8100c00fffff: 0.45 schools per 10,000 residents; shared out in all: 7.0 schools
```

On the command line, the same is `peabiru locate` and `peabiru catchment`
(see [The command line](cli.md)).

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
