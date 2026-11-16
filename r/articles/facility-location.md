# Facility location

Travel times say how well a place is served today.
[`locate_facilities()`](../reference/locate_facilities.md) turns the
question around: given the people to serve and the places where a
facility could open, *where should the next ones go?*
[`floating_catchment()`](../reference/floating_catchment.md) measures
the other side of access, supply against the demand that competes for
it. Both work on the travel times of the network, by any mode, and both
are peabiru’s own functions.

## The four models

[`locate_facilities()`](../reference/locate_facilities.md) takes the
demand points (with a weight each: residents, pupils, patients), the
candidate sites and, optionally, the facilities that already exist, and
solves one of four classic models:

| `model` | question | objective |
|----|----|----|
| `"max_cover"` | with `n_facilities` new facilities, how many people can be within `cutoff` minutes of one? | demand covered (to maximise) |
| `"set_cover"` | how few facilities put everyone within `cutoff` minutes? | number of new sites |
| `"p_median"` | where do `n_facilities` facilities make the average trip shortest? | demand-weighted minutes to the nearest facility |
| `"p_center"` | where do they make the worst trip shortest? | minutes of the demand point farthest from its nearest facility |

A demand point is *covered* by a site at most `cutoff` minutes away (the
cutoff included). The `existing` facilities count first: only the demand
they leave uncovered, or the travel time they leave, drives the choice
of the new sites, and they are never closed or moved.

## A worked example: two new schools

The sample network has 27 points with their population; six of them have
a school. Where would two more schools put the most residents within a
12-minute walk of one?

\
`schools`` ``<-`` ``points``[``points``$``schools`` ``>`` ``0``, ``]`\
`others`` ``<-`` ``points``[``points``$``schools`` ``==`` ``0``, ``]`\
\
`found`` ``<-`` `[`locate_facilities`](../reference/locate_facilities.md)`(`\
`  ``network``, ``points``, ``others``, existing ``=`` ``schools``,`\
`  model ``=`` ``"max_cover"``, n_facilities ``=`` ``2``, cutoff ``=`` ``12``,`\
`  demand_colname ``=`` ``"population"``, mode ``=`` ``"WALK"``, n_threads ``=`` ``2`\
`)`\
`found``$``sites`\
`#> ``# A tibble: 2 × 6`\
`#>   id                lon   lat order marginal cumulative`\
`#>   ``<chr>``           ``<dbl>`` ``<dbl>`` ``<int>``    ``<dbl>``      ``<dbl>`\
`#> ``1`` 89a8100c03bffff -``46.6`` -``23.6``     1     ``5``658      ``43``534`\
`#> ``2`` 89a8100c383ffff -``46.6`` -``23.5``     2     ``2``683      ``46``217`

The result is a list of three tables and a summary. `sites` lists the
new sites by what each adds: `marginal` is the demand it newly covers
and `cumulative` the demand covered so far, existing schools included.
The summary has the totals:

\
`s`` ``<-`` ``found``$``summary`\
[`round`](https://rdrr.io/r/base/Round.html)`(``100`` ``*`` `[`c`](https://rdrr.io/r/base/c.html)`(``today ``=`` ``s``$``covered_before``, greedy ``=`` ``s``$``objective_greedy``,`\
`              solution ``=`` ``s``$``covered``)`` ``/`` ``s``$``total_demand``, ``1``)`\
`#>    today   greedy solution `\
`#>     81.9     99.5     99.9`\
`s``$``n_swaps`\
`#> [1] 1`

`candidates` has one row per candidate with its `potential`: the
residents not covered today that a school there would cover *alone*. It
is the map to draw before choosing anything, since it shows every good
place and not only the winners.

\
[`head`](https://rdrr.io/r/utils/head.html)`(``found``$``candidates``[`[`order`](https://rdrr.io/r/base/order.html)`(``-``found``$``candidates``$``potential``)``,`\
`                      `[`c`](https://rdrr.io/r/base/c.html)`(``"id"``, ``"potential"``, ``"chosen"``)``]``, ``4``)`\
`#> ``# A tibble: 4 × 3`\
`#>   id              potential chosen`\
`#>   ``<chr>``               ``<dbl>`` ``<lgl>`` `\
`#> ``1`` 89a8100c033ffff      ``5``764 FALSE `\
`#> ``2`` 89a8100c03bffff      ``5``658 TRUE  `\
`#> ``3`` 89a8100c007ffff      ``4``829 FALSE `\
`#> ``4`` 89a8100c00fffff      ``3``936 FALSE`

The candidate with the largest potential is not in the solution: the two
sites chosen overlap less with each other than it does with either. That
is the difference between ranking sites one by one and choosing a set.

![A map of the 27 sample points. Candidate sites are circles whose size
grows with the residents a school there would newly cover; the six
existing schools are squares; the two sites chosen are filled circles,
both on the east side of the area, one to the north and one to the
south.](facility-location_files/figure-html/map-1.png)

`demand` has one row per demand point: its weight, the nearest open
facility (existing or new), the minutes to it and whether it is covered.

\
[`head`](https://rdrr.io/r/utils/head.html)`(``found``$``demand``[`[`c`](https://rdrr.io/r/base/c.html)`(``"id"``, ``"demand"``, ``"facility_id"``, ``"travel_time"``, ``"covered"``)``]``, ``4``)`\
`#> ``# A tibble: 4 × 5`\
`#>   id              demand facility_id     travel_time covered`\
`#>   ``<chr>``            ``<dbl>`` ``<chr>``                 ``<dbl>`` ``<lgl>``  `\
`#> ``1`` 89a8100c00fffff   ``3``648 89a8100c03bffff           8 TRUE   `\
`#> ``2`` 89a8100c007ffff   ``3``936 89a8100c03bffff           9 TRUE   `\
`#> ``3`` 89a8100c077ffff   ``6``514 89a8100c077ffff           0 TRUE   `\
`#> ``4`` 89a8100c03bffff   ``3``670 89a8100c03bffff           0 TRUE`

## How the models are solved

By heuristics, with no integer programming solver:

1.  **Greedy construction.** Sites are opened one at a time, each the
    one that improves the objective most given the ones already open.
    For maximal covering this covers at least 63% (1 - 1/e) of the most
    that any choice of `n_facilities` sites could cover of the demand
    not covered today (Nemhauser, Wolsey and Fisher, 1978); in practice
    it is usually much closer.
2.  **Vertex substitution** (`swap = TRUE`, the default; Teitz and Bart,
    1968). An open new site is swapped for a closed candidate while that
    improves the objective, taking the best swap of all at each step.
    The result is never worse than the greedy one, and `summary` reports
    both (`objective_greedy`, `objective`, `n_swaps`).

In the example the greedy pair covers 46,040 residents and one swap
takes it to 46,217, which is the optimum here, as an exhaustive search
over the 210 pairs of candidates shows:

\
`ttm`` ``<-`` `[`travel_time_matrix`](../reference/travel_time_matrix.md)`(``network``, ``points``, `[`rbind`](https://rdrr.io/r/base/cbind.html)`(``schools``, ``others``)``, mode ``=`` ``"WALK"``,`\
`                          max_trip_duration ``=`` ``12``, n_threads ``=`` ``2``)`\
`covered`` ``<-`` ``function``(``new``)`` ``{`\
`  ``near`` ``<-`` `[`unique`](https://rdrr.io/r/base/unique.html)`(``ttm``$``from_id``[``ttm``$``to_id`` `[`%in%`](https://rdrr.io/r/base/match.html)` `[`c`](https://rdrr.io/r/base/c.html)`(``schools``$``id``, ``new``)``]``)`\
`  `[`sum`](https://rdrr.io/r/base/sum.html)`(``points``$``population``[``points``$``id`` `[`%in%`](https://rdrr.io/r/base/match.html)` ``near``]``)`\
`}`\
[`max`](https://rdrr.io/r/base/Extremes.html)`(`[`apply`](https://rdrr.io/r/base/apply.html)`(`[`combn`](https://rdrr.io/r/utils/combn.html)`(``others``$``id``, ``2``)``, ``2``, ``covered``)``)`\
`#> [1] 46217`

That is not guaranteed in general: the solutions are good and
reproducible, not proven optimal.

Details by model:

- `"set_cover"` opens the site that covers the most demand points not
  yet covered until every point some site can cover is covered, then
  drops the sites the others make redundant.
- `"p_center"` compares the largest travel time and, on ties, the
  demand-weighted sum, in the greedy step and in the swaps.
- In `"set_cover"` and `"p_center"` a point with weight 0 is not demand,
  and the others count the same whatever their weight.
- In `"p_median"` and `"p_center"`, a demand point that reaches no open
  facility within `max_trip_duration` is charged `unreachable_time`
  minutes (by default twice the longest travel time of the table), so
  that serving a point always beats shortening the trip of another. A
  point that no site at all can serve is left out of the objective and
  reported in `summary$unservable_demand`.
- Ties go to the candidate listed first, and the result does not depend
  on `n_threads`.

## How close to the best possible

`summary$upper_bound` answers the question for maximal covering without
solving anything else: no choice of `n_facilities` sites can cover more
demand than it, so the optimum lies between `covered` and `upper_bound`,
and `covered / upper_bound` is a floor on the share of the optimum that
the solution reaches. The bound comes from the shape of the problem: the
demand covered by any set of sites, plus the `n_facilities` largest
gains of the sites outside it, is at least what the best set covers. It
is taken twice, at the existing facilities alone and at the solution,
and the smaller of the two is reported (never more than the demand some
site can cover). When the two numbers are equal the solution is proven
optimal; when they are not, the solution may still be optimal, as in the
example, where the bound is 46,273 residents and the optimum 46,217: the
bound is a ceiling, not the optimum itself.

\
[`c`](https://rdrr.io/r/base/c.html)`(``covered ``=`` ``s``$``covered``, upper_bound ``=`` ``s``$``upper_bound``)`\
`#>     covered upper_bound `\
`#>       46217       46273`

## The whole curve in one call

How much would a third school add, or a fourth? With `swap = FALSE` the
sites of maximal covering are nested: the first `k` rows of `sites` are
the answer for `k` facilities. Asking for as many facilities as there
are candidates therefore gives the demand covered for every number of
facilities (`min_gain` stops the list where a site no longer pays):

\
`curve`` ``<-`` `[`locate_facilities`](../reference/locate_facilities.md)`(`\
`  ``network``, ``points``, ``others``, existing ``=`` ``schools``, model ``=`` ``"max_cover"``,`\
`  n_facilities ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``others``)``, cutoff ``=`` ``12``, swap ``=`` ``FALSE``,`\
`  demand_colname ``=`` ``"population"``, mode ``=`` ``"WALK"``, n_threads ``=`` ``2`\
`)`\
`curve``$``sites``[`[`c`](https://rdrr.io/r/base/c.html)`(``"order"``, ``"marginal"``, ``"cumulative"``)``]`\
`#> ``# A tibble: 4 × 3`\
`#>   order marginal cumulative`\
`#>   ``<int>``    ``<dbl>``      ``<dbl>`\
`#> ``1``     1     ``5``764      ``43``640`\
`#> ``2``     2     ``2``400      ``46``040`\
`#> ``3``     3      177      ``46``217`\
`#> ``4``     4       56      ``46``273`

With swaps the sites for one number of facilities need not include those
for a smaller number, so the curve is then one call per number. The
curves of several areas are what it takes to share a budget of
facilities among them, which is the next section.

## Sharing a budget among areas

A state has so many facilities to build and many municipalities to build
them in. With the curve of each area,
[`allocate_budget()`](../reference/allocate_budget.md) says how many go
to each, and in what order, by one of two criteria:

- `criterion = "utilitarian"`: each facility goes where the next one
  adds most. Since what a further facility adds in an area never
  increases, this covers the most demand possible, for the budget asked
  and for every smaller one (marginal analysis; Fox, 1966).
- `criterion = "maximin"`: each facility goes to the area with the
  lowest share of its demand covered (that still has a facility worth
  opening), which gives the highest possible lowest share.

Here the sample points are split in two halves, east and west, each with
its curve for schools within an 8-minute walk (`swap = FALSE`,
`n_facilities` as many as the candidates), and three schools are shared
out:

\
`points``$``side`` ``<-`` `[`ifelse`](https://rdrr.io/r/base/ifelse.html)`(``points``$``lon`` ``<`` ``stats``::`[`median`](https://rdrr.io/r/stats/median.html)`(``points``$``lon``)``, ``"west"``, ``"east"``)`\
`by_side`` ``<-`` `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`split`](https://rdrr.io/r/base/split.html)`(``points``, ``points``$``side``)``, ``function``(``part``)`` ``{`\
`  `[`locate_facilities`](../reference/locate_facilities.md)`(`\
`    ``network``, ``part``, ``part``[``part``$``schools`` ``==`` ``0``, ``]``, existing ``=`` ``part``[``part``$``schools`` ``>`` ``0``, ``]``,`\
`    model ``=`` ``"max_cover"``, n_facilities ``=`` `[`sum`](https://rdrr.io/r/base/sum.html)`(``part``$``schools`` ``==`` ``0``)``, cutoff ``=`` ``8``,`\
`    swap ``=`` ``FALSE``, demand_colname ``=`` ``"population"``, mode ``=`` ``"WALK"``, n_threads ``=`` ``2`\
`  ``)`\
`}``)`\
`curves`` ``<-`` `[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`names`](https://rdrr.io/r/base/names.html)`(``by_side``)``, ``function``(``side``)`` ``{`\
`  `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(``group ``=`` ``side``, ``by_side``[[``side``]``]``$``sites``[`[`c`](https://rdrr.io/r/base/c.html)`(``"order"``, ``"marginal"``)``]``)`\
`}``)``)`\
`groups`` ``<-`` `[`data.frame`](https://rdrr.io/r/base/data.frame.html)`(`\
`  group ``=`` `[`names`](https://rdrr.io/r/base/names.html)`(``by_side``)``,`\
`  covered ``=`` `[`vapply`](https://rdrr.io/r/base/lapply.html)`(``by_side``, ``function``(``f``)`` ``f``$``summary``$``covered_before``, ``0``)``,`\
`  total ``=`` `[`vapply`](https://rdrr.io/r/base/lapply.html)`(``by_side``, ``function``(``f``)`` ``f``$``summary``$``total_demand``, ``0``)`\
`)`\
[`allocate_budget`](../reference/allocate_budget.md)`(``curves``, budget ``=`` ``3``, groups ``=`` ``groups``)``$``steps`\
`#> ``# A tibble: 3 × 5`\
`#>   order group marginal covered lowest_share`\
`#>   ``<int>`` ``<chr>``    ``<dbl>``   ``<dbl>``        ``<dbl>`\
`#> ``1``     1 east      ``7``584   ``37``489        0.763`\
`#> ``2``     2 east      ``2``485   ``39``974        0.763`\
`#> ``3``     3 west      ``2``185   ``42``159        0.854`\
[`allocate_budget`](../reference/allocate_budget.md)`(``curves``, budget ``=`` ``3``, groups ``=`` ``groups``, criterion ``=`` ``"maximin"``)``$``steps`\
`#> ``# A tibble: 3 × 5`\
`#>   order group marginal covered lowest_share`\
`#>   ``<int>`` ``<chr>``    ``<dbl>``   ``<dbl>``        ``<dbl>`\
`#> ``1``     1 east      ``7``584   ``37``489        0.763`\
`#> ``2``     2 west      ``2``185   ``39``674        0.854`\
`#> ``3``     3 west      ``1``958   ``41``632        0.861`

The two criteria agree on the first school and part ways on the second:
the most coverage takes it east again, the fairer rule sends it west,
where a smaller share of the residents is covered. With three schools
the fairer allocation covers 527 residents fewer (41,632 against 42,159)
and raises the lowest share from 85.4% to 86.1%: that difference is the
price of fairness (Bertsimas, Farias and Trichakis, 2011), and the
`steps` of the two criteria give it for every budget.

### Weights, priority tiers and areas left out

Three columns of `groups` change who comes first, each named in an
argument:

- `weights`: how much the demand of each area counts, say 2 where the
  need is most urgent and 1 elsewhere. The first criterion then
  multiplies what a facility adds by the weight of its area (and still
  gives the largest weighted sum for every budget, since the weighted
  gains do not increase either); the second divides the share covered by
  the weight.
- `tiers`: a whole number per area, the lowest first. A facility only
  goes to an area when no area of a lower tier has one left that adds
  anything, and within the tier the criterion decides: with `"maximin"`,
  the worst covered among the most urgent first.
- `eligible`: `FALSE` for the areas that get no facility. They stay in
  the result, with zero facilities, and count in `covered` and in
  `lowest_share`.

Nothing in the result is weighted: the gains, the demand covered and the
shares are residents, so the cost of a rule is the difference to the run
without it. The `groups` table of the result has the `priority` of each
area, the order in which the areas get their first facility, which sums
up the criterion and the rules in one number per area.

\
`groups``$``weight`` ``<-`` `[`c`](https://rdrr.io/r/base/c.html)`(``east ``=`` ``1``, west ``=`` ``2``)``[``groups``$``group``]`\
`groups``$``tier`` ``<-`` `[`c`](https://rdrr.io/r/base/c.html)`(``east ``=`` ``1``, west ``=`` ``0``)``[``groups``$``group``]`\
[`allocate_budget`](../reference/allocate_budget.md)`(``curves``, budget ``=`` ``3``, groups ``=`` ``groups``, weights ``=`` ``"weight"``)``$``steps`\
`#> ``# A tibble: 3 × 5`\
`#>   order group marginal covered lowest_share`\
`#>   ``<int>`` ``<chr>``    ``<dbl>``   ``<dbl>``        ``<dbl>`\
`#> ``1``     1 east      ``7``584   ``37``489        0.763`\
`#> ``2``     2 west      ``2``185   ``39``674        0.854`\
`#> ``3``     3 west      ``1``958   ``41``632        0.861`\
[`allocate_budget`](../reference/allocate_budget.md)`(``curves``, budget ``=`` ``3``, groups ``=`` ``groups``, tiers ``=`` ``"tier"``)``$``groups`\
`#> ``# A tibble: 2 × 6`\
`#>   group units gained covered share priority`\
`#>   ``<chr>`` ``<int>``  ``<dbl>``   ``<dbl>`` ``<dbl>``    ``<int>`\
`#> ``1`` east      0      0   ``11``627 0.521       ``NA`\
`#> ``2`` west      3   ``5``688   ``23``966 1            1`

With the west counting double, the second school goes west: its 2,185
residents weigh 4,370, more than the 2,485 of a second school in the
east. The allocation is the one of the fairer criterion above, 527
residents short of the most coverage. With the west in the first tier
the three schools go there, which covers all its residents and leaves
the east where it was, at 52.1%: 35,593 residents covered in all against
42,159, which is what that rule costs. `priority` has the west first and
the east with no facility.

## Facilities with a capacity

Maximal covering assumes that one facility serves everyone within the
cutoff. When a facility serves only so many people, a place with much
demand needs several, and `capacity` says how many: the model is then
the capacitated maximal covering (Current and Storbeck, 1988).
`capacity` is a number, or the name of a column of the candidates (and
of the existing facilities) with the capacity at each site.

\
`full`` ``<-`` `[`locate_facilities`](../reference/locate_facilities.md)`(`\
`  ``network``, ``points``, ``others``, existing ``=`` ``schools``, model ``=`` ``"max_cover"``,`\
`  n_facilities ``=`` ``4``, cutoff ``=`` ``12``, capacity ``=`` ``3000``,`\
`  demand_colname ``=`` ``"population"``, mode ``=`` ``"WALK"``, n_threads ``=`` ``2`\
`)`\
`full``$``sites``[`[`c`](https://rdrr.io/r/base/c.html)`(``"id"``, ``"order"``, ``"marginal"``, ``"cumulative"``)``]`\
`#> ``# A tibble: 4 × 4`\
`#>   id              order marginal cumulative`\
`#>   ``<chr>``           ``<int>``    ``<dbl>``      ``<dbl>`\
`#> ``1`` 89a8100c00fffff     1     ``3``000      ``19``623`\
`#> ``2`` 89a8100c00fffff     2     ``3``000      ``22``623`\
`#> ``3`` 89a8100c00fffff     3     ``3``000      ``25``623`\
`#> ``4`` 89a8100c00fffff     4     ``3``000      ``28``623`\
[`c`](https://rdrr.io/r/base/c.html)`(``served_today ``=`` ``full``$``summary``$``covered_before``, upper_bound ``=`` ``full``$``summary``$``upper_bound``)`\
`#> served_today  upper_bound `\
`#>        16623        28623`

With 3,000 places each, the six schools serve 16,623 residents of the
37,876 within 12 minutes of one, and the four new schools all go to the
same place, where the most unserved residents are.

What changes with a capacity:

- A candidate may open more than once: `sites` has one row per facility
  opened, `n_facilities` counts facilities and may exceed the number of
  candidates, and `candidates` gains `units`.
- Facilities open one at a time, each where it would serve the most
  demand not yet served, and take that demand from the nearest points
  first. A demand point may be served in part, or by more than one
  facility: `demand` gains `served`, the share of its demand that is
  served, while `covered` still says whether it is within the cutoff of
  a facility, with room or not.
- `flows` says who serves whom: one row per pair of demand point and
  facility with demand served between them, existing facilities
  included.
- The existing facilities have capacities too and serve first, each pair
  of facility and demand point in the order of their travel times.
- `swap` has no effect, and `upper_bound` is a ceiling on what any
  choice of `n_facilities` facilities could serve, given what the
  existing ones serve.

## The other models

How few schools would put every resident within a 10-minute walk,
starting from none?

\
`cover`` ``<-`` `[`locate_facilities`](../reference/locate_facilities.md)`(`\
`  ``network``, ``points``, ``points``, model ``=`` ``"set_cover"``, cutoff ``=`` ``10``,`\
`  demand_colname ``=`` ``"population"``, mode ``=`` ``"WALK"``, n_threads ``=`` ``2`\
`)`\
[`nrow`](https://rdrr.io/r/base/nrow.html)`(``cover``$``sites``)`\
`#> [1] 7`\
`cover``$``summary``$``unservable_demand`\
`#> [1] 0`

Covering every last resident is often what costs most. With `coverage`
the model stops at a share of the demand, and the sites are then chosen
by the demand each newly covers (the weights count, which they do not
when everything must be covered):

\
`part`` ``<-`` `[`locate_facilities`](../reference/locate_facilities.md)`(`\
`  ``network``, ``points``, ``points``, model ``=`` ``"set_cover"``, cutoff ``=`` ``10``, coverage ``=`` ``0.9``,`\
`  demand_colname ``=`` ``"population"``, mode ``=`` ``"WALK"``, n_threads ``=`` ``2`\
`)`\
[`c`](https://rdrr.io/r/base/c.html)`(``sites ``=`` `[`nrow`](https://rdrr.io/r/base/nrow.html)`(``part``$``sites``)``, share ``=`` `[`round`](https://rdrr.io/r/base/Round.html)`(``part``$``summary``$``covered`` ``/`` ``part``$``summary``$``total_demand``, ``3``)``)`\
`#> sites share `\
`#> 4.000 0.946`

Four sites cover 94.6% of the residents; the last 5.4% take three more.
When the target cannot be reached, all the demand that can be covered
is.

Two sites that make the average walk shortest, and two that make the
longest walk shortest:

\
`compare`` ``<-`` `[`lapply`](https://rdrr.io/r/base/lapply.html)`(`[`c`](https://rdrr.io/r/base/c.html)`(``p_median ``=`` ``"p_median"``, p_center ``=`` ``"p_center"``)``, ``function``(``model``)`` ``{`\
`  ``r`` ``<-`` `[`locate_facilities`](../reference/locate_facilities.md)`(`\
`    ``network``, ``points``, ``points``, model ``=`` ``model``, n_facilities ``=`` ``2``,`\
`    demand_colname ``=`` ``"population"``, mode ``=`` ``"WALK"``, max_trip_duration ``=`` ``60``,`\
`    n_threads ``=`` ``2`\
`  ``)`\
`  `[`c`](https://rdrr.io/r/base/c.html)`(``mean ``=`` `[`weighted.mean`](https://rdrr.io/r/stats/weighted.mean.html)`(``r``$``demand``$``travel_time``, ``r``$``demand``$``demand``)``,`\
`    longest ``=`` `[`max`](https://rdrr.io/r/base/Extremes.html)`(``r``$``demand``$``travel_time``)``)`\
`}``)`\
[`round`](https://rdrr.io/r/base/Round.html)`(`[`do.call`](https://rdrr.io/r/base/do.call.html)`(``rbind``, ``compare``)``, ``1``)`\
`#>          mean longest`\
`#> p_median  9.7      32`\
`#> p_center 13.3      19`

The p-median pair gives the shorter average and the p-center pair the
shorter worst case: the two models answer different questions.

Any mode works, since the travel times come from
[`travel_time_matrix()`](../reference/travel_time_matrix.md): pass
`mode`, `departure_datetime` and any other routing argument.

\
`transit`` ``<-`` `[`locate_facilities`](../reference/locate_facilities.md)`(`\
`  ``network``, ``points``, ``points``, model ``=`` ``"p_median"``, n_facilities ``=`` ``3``,`\
`  demand_colname ``=`` ``"population"``, mode ``=`` `[`c`](https://rdrr.io/r/base/c.html)`(``"WALK"``, ``"TRANSIT"``)``,`\
`  departure_datetime ``=`` ``departure``, seed ``=`` ``1``, max_trip_duration ``=`` ``60``,`\
`  n_threads ``=`` ``2`\
`)`\
`transit``$``sites``$``id`\
`#> [1] "89a8100c03bffff" "89a8100c1dbffff" "89a8100c15bffff"`

## Supply against demand: the floating catchment area

Being near a school is not the same as having a place in it.
[`floating_catchment()`](../reference/floating_catchment.md) computes
the two-step floating catchment area (Luo and Wang, 2003):

1.  each facility’s supply is divided by the demand within its
    catchment;
2.  each demand point adds up the ratios of the facilities within its
    catchment.

The result is supply per unit of demand at each demand point: here,
schools per resident within a 20-minute walk.

\
`fca`` ``<-`` `[`floating_catchment`](../reference/floating_catchment.md)`(`\
`  ``network``, ``points``, ``schools``, cutoff ``=`` ``20``,`\
`  demand_colname ``=`` ``"population"``, supply_colname ``=`` ``"schools"``,`\
`  mode ``=`` ``"WALK"``, n_threads ``=`` ``2`\
`)`\
`fca``$``supply``[`[`c`](https://rdrr.io/r/base/c.html)`(``"id"``, ``"supply"``, ``"catchment_demand"``, ``"ratio"``)``]`\
`#> ``# A tibble: 6 × 4`\
`#>   id              supply catchment_demand     ratio`\
`#>   ``<chr>``            ``<dbl>``            ``<dbl>``     ``<dbl>`\
`#> ``1`` 89a8100c077ffff      1            ``22``033 0.000``045``4`\
`#> ``2`` 89a8100c1dbffff      2            ``17``501 0.000``114`` `\
`#> ``3`` 89a8100c39bffff      1            ``10``593 0.000``094``4`\
`#> ``4`` 89a8100c1cbffff      1            ``21``263 0.000``047``0`\
`#> ``5`` 89a8100c153ffff      1            ``22``664 0.000``044``1`\
`#> ``6`` 89a8100c14fffff      1            ``11``967 0.000``083``6`\
`# schools per 10,000 residents`\
[`round`](https://rdrr.io/r/base/Round.html)`(`[`range`](https://rdrr.io/r/base/range.html)`(``10000`` ``*`` ``fca``$``demand``$``accessibility``)``, ``2``)`\
`#> [1] 0.45 3.34`

The index shares out the supply: the sum over the demand points of
demand times `accessibility` is the supply of the facilities that have
any demand in their catchment.

\
[`sum`](https://rdrr.io/r/base/sum.html)`(``fca``$``demand``$``demand`` ``*`` ``fca``$``demand``$``accessibility``)`\
`#> [1] 7`\
[`sum`](https://rdrr.io/r/base/sum.html)`(``schools``$``schools``)`\
`#> [1] 7`

With `decay_function = "step"` a pair counts in full up to `cutoff`
minutes and not at all beyond. Any other decay function gives the
*enhanced* form (Luo and Qi, 2009), in which nearer pairs weigh more, in
both steps. The functions and their `decay_value` are those of
[`accessibility()`](../reference/accessibility.md) (see
`vignette("accessibility")`); the weight falls continuously with travel
time, where Luo and Qi used a few travel-time zones.

\
`smooth`` ``<-`` `[`floating_catchment`](../reference/floating_catchment.md)`(`\
`  ``network``, ``points``, ``schools``, cutoff ``=`` ``20``,`\
`  decay_function ``=`` ``"logistic"``, decay_value ``=`` ``5``,`\
`  demand_colname ``=`` ``"population"``, supply_colname ``=`` ``"schools"``,`\
`  mode ``=`` ``"WALK"``, n_threads ``=`` ``2`\
`)`\
[`round`](https://rdrr.io/r/base/Round.html)`(`[`range`](https://rdrr.io/r/base/range.html)`(``10000`` ``*`` ``smooth``$``demand``$``accessibility``)``, ``2``)`\
`#> [1] 0.69 2.74`

## From a table of travel times

Both functions take a table of travel times instead of a network:
`from_id` (demand point), `to_id` (site) and the minutes in
`travel_time_pXX` or `travel_time`, as
[`travel_time_matrix()`](../reference/travel_time_matrix.md) returns it.
A pair that is absent is unreachable. The points then need only their
`id`, so the same matrix can serve several models, or travel times from
another source can be used. Here, the matrix computed above:

\
`again`` ``<-`` `[`locate_facilities`](../reference/locate_facilities.md)`(`\
`  ``NULL``, ``points``[`[`c`](https://rdrr.io/r/base/c.html)`(``"id"``, ``"population"``)``]``, ``others``[``"id"``]``, existing ``=`` ``schools``[``"id"``]``,`\
`  model ``=`` ``"max_cover"``, n_facilities ``=`` ``2``, cutoff ``=`` ``12``,`\
`  demand_colname ``=`` ``"population"``, travel_times ``=`` ``ttm`\
`)`\
[`identical`](https://rdrr.io/r/base/identical.html)`(``again``$``sites``$``id``, ``found``$``sites``$``id``)`\
`#> [1] TRUE`

The models do not depend on the unit of the table. It may hold any cost
that is not negative, such as a straight-line or network distance in
metres where the service standard is a distance: `cutoff`,
`unreachable_time` and the `travel_time` column of the result are then
in that unit.

[`distance_matrix()`](../reference/distance_matrix.md) makes such a
table from the straight-line distances between two sets of points: every
pair at most so many metres apart, with no network. Here, the two sites
that put the most residents within 500 m:

\
`near`` ``<-`` `[`distance_matrix`](../reference/distance_matrix.md)`(``points``, ``points``, max_distance ``=`` ``500``)`\
[`head`](https://rdrr.io/r/utils/head.html)`(``near``, ``3``)`\
`#> ``# A tibble: 3 × 3`\
`#>   from_id         to_id           distance`\
`#>   ``<chr>``           ``<chr>``              ``<dbl>`\
`#> ``1`` 89a8100c00fffff 89a8100c00fffff       0 `\
`#> ``2`` 89a8100c00fffff 89a8100c007ffff     352.`\
`#> ``3`` 89a8100c00fffff 89a8100c077ffff     341.`\
`by_distance`` ``<-`` `[`locate_facilities`](../reference/locate_facilities.md)`(`\
`  ``NULL``, ``points``, ``points``, model ``=`` ``"max_cover"``, n_facilities ``=`` ``2``, cutoff ``=`` ``500``,`\
`  demand_colname ``=`` ``"population"``, travel_times ``=`` ``near`\
`)`\
`by_distance``$``sites``[`[`c`](https://rdrr.io/r/base/c.html)`(``"id"``, ``"order"``, ``"marginal"``, ``"cumulative"``)``]`\
`#> ``# A tibble: 2 × 4`\
`#>   id              order marginal cumulative`\
`#>   ``<chr>``           ``<int>``    ``<dbl>``      ``<dbl>`\
`#> ``1`` 89a8100c03bffff     1    ``19``490      ``19``490`\
`#> ``2`` 89a8100c037ffff     2    ``13``847      ``33``337`

## What the models do not see

The models choose sites by travel time and demand alone. They do not
know the cost of a facility (nor its capacity, except in maximal
covering with `capacity`), whether land is available at a candidate
site, the demand that comes from outside the study area, or how well the
people already covered are served. Maximal covering treats 11 minutes
and 1 minute alike, and p-median may leave a few people very far to
shorten the average: comparing two models on the same data is often more
informative than either alone.

## References

- Bertsimas, D., Farias, V. F. and Trichakis, N. (2011). The price of
  fairness. *Operations Research*, 59(1), 17-31.
  [doi:10.1287/opre.1100.0865](https://doi.org/10.1287/opre.1100.0865)
- Church, R. and ReVelle, C. (1974). The maximal covering location
  problem. *Papers of the Regional Science Association*, 32, 101-118.
  [doi:10.1007/BF01942293](https://doi.org/10.1007/BF01942293)
- Current, J. R. and Storbeck, J. E. (1988). Capacitated covering
  models. *Environment and Planning B: Planning and Design*, 15(2),
  153-163. [doi:10.1068/b150153](https://doi.org/10.1068/b150153)
- Fox, B. (1966). Discrete optimization via marginal analysis.
  *Management Science*, 13(3), 210-216.
  [doi:10.1287/mnsc.13.3.210](https://doi.org/10.1287/mnsc.13.3.210)
- Hakimi, S. L. (1964). Optimum locations of switching centers and the
  absolute centers and medians of a graph. *Operations Research*, 12(3),
  450-459.
  [doi:10.1287/opre.12.3.450](https://doi.org/10.1287/opre.12.3.450)
- Luo, W. and Qi, Y. (2009). An enhanced two-step floating catchment
  area (E2SFCA) method for measuring spatial accessibility to primary
  care physicians. *Health & Place*, 15(4), 1100-1107.
  [doi:10.1016/j.healthplace.2009.06.002](https://doi.org/10.1016/j.healthplace.2009.06.002)
- Luo, W. and Wang, F. (2003). Measures of spatial accessibility to
  health care in a GIS environment: synthesis and a case study in the
  Chicago region. *Environment and Planning B: Planning and Design*,
  30(6), 865-884. [doi:10.1068/b29120](https://doi.org/10.1068/b29120)
- Nemhauser, G. L., Wolsey, L. A. and Fisher, M. L. (1978). An analysis
  of approximations for maximizing submodular set functions - I.
  *Mathematical Programming*, 14, 265-294.
  [doi:10.1007/BF01588971](https://doi.org/10.1007/BF01588971)
- Teitz, M. B. and Bart, P. (1968). Heuristic methods for estimating the
  generalized vertex median of a weighted graph. *Operations Research*,
  16(5), 955-961.
  [doi:10.1287/opre.16.5.955](https://doi.org/10.1287/opre.16.5.955)
- Toregas, C., Swain, R., ReVelle, C. and Bergman, L. (1971). The
  location of emergency service facilities. *Operations Research*,
  19(6), 1363-1373.
  [doi:10.1287/opre.19.6.1363](https://doi.org/10.1287/opre.19.6.1363)
