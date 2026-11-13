# Accessibility

[`accessibility()`](reference.md#peabiru.accessibility) counts the
opportunities (jobs, schools, people...) that can be reached from each
origin, either within cutoffs or weighted by a decay function of the travel
time.

```python
import datetime as dt

import peabiru

network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
points = peabiru.sample_points()
monday = dt.datetime(2019, 5, 13, 14, 0)
```

## Cumulative opportunities

The default decay function, `"step"`, counts the opportunities reachable
within each cutoff. The opportunity counts are numeric columns of
`destinations`, named in `opportunities_colnames`:

```python
access = peabiru.accessibility(
    network, points, points,
    opportunities_colnames=["jobs", "schools"],
    mode=["WALK", "TRANSIT"],
    departure_datetime=monday,
    cutoffs=[30, 15],
    percentiles=[50, 25],
    seed=42,
    n_threads=2,
)
print(access.head(8))
print(access.dtypes)
```

```text
                id opportunity  percentile  cutoff  accessibility
0  89a8100c00fffff        jobs          25      15        12452.0
1  89a8100c00fffff        jobs          25      30        95249.0
2  89a8100c00fffff        jobs          50      15        12452.0
3  89a8100c00fffff        jobs          50      30        95249.0
4  89a8100c00fffff     schools          25      15            1.0
5  89a8100c00fffff     schools          25      30            4.0
6  89a8100c00fffff     schools          50      15            1.0
7  89a8100c00fffff     schools          50      30            4.0
id                object
opportunity       object
percentile         int32
cutoff             int32
accessibility    float64
dtype: object
```

There is one row per origin, opportunity, percentile and cutoff, in that
order: origins and opportunities as given, percentiles and cutoffs
ascending. Missing opportunity counts count as zero, with a warning.

With the `"step"` function, trips longer than the largest cutoff
are not considered (`max_trip_duration` becomes `max(cutoffs)`), and
`max_trip_duration` may never be shorter than the largest cutoff.

## Decay functions

The other functions weight each opportunity by its travel time. Their
parameter is `decay_value`:

| `decay_function` | `cutoffs` | `decay_value` |
|---|---|---|
| `"step"` | required | `None` |
| `"exponential"` | required (each cutoff is a half-life) | `None` |
| `"linear"` | required | the width in minutes, a whole number from 1 to 59 |
| `"logistic"` | required | the standard deviation in minutes, at least 1 |
| `"fixed_exponential"` | `None` | the decay constant, between 0 and 1 |

```python
for fun, value in [("exponential", None), ("linear", 10), ("logistic", 5)]:
    a = peabiru.accessibility(
        network, points, points,
        opportunities_colnames="jobs",
        mode="WALK",
        decay_function=fun,
        decay_value=value,
        cutoffs=20,
        n_threads=2,
    )
    print(f"{fun:12s} {float(a['accessibility'].iloc[0])!r}")
```

```text
exponential  42932.32347019101
linear       18205.424166666664
logistic     20362.754369823153
```

`"fixed_exponential"` has no cutoff, so the result has no `cutoff` column:

```python
a = peabiru.accessibility(
    network, points, points,
    opportunities_colnames="jobs",
    mode="BICYCLE",
    decay_function="fixed_exponential",
    decay_value=0.002,
    n_threads=2,
)
print(a.head(3))
```

```text
                id opportunity  percentile  accessibility
0  89a8100c00fffff        jobs          50   24448.202280
1  89a8100c007ffff        jobs          50   21875.710706
2  89a8100c077ffff        jobs          50   24385.802656
```

Accessibility values are doubles (`float64`). The travel time given to the
decay function is the percentile in seconds, not rounded to minutes, and the
exponential is table-based (a translation of Apache Commons Math's
`FastMath.exp()`) rather than the C library's, so the values do not depend
on the platform's maths library.

## Street modes

With `"WALK"`, `"BICYCLE"` or `"CAR"` alone, each destination has one travel
time, so every percentile gets the same value. `departure_datetime` is then
not needed.

```python
by_car = peabiru.accessibility(
    network, points, points,
    opportunities_colnames="population",
    mode="CAR",
    cutoffs=[5, 10],
    n_threads=2,
)
wide = by_car.pivot(index="id", columns="cutoff", values="accessibility")
print(wide.head(3))
```

```text
cutoff                5        10
id                               
89a8100c007ffff  19546.0  41625.0
89a8100c00fffff  19490.0  41625.0
89a8100c023ffff  26728.0  41625.0
```
