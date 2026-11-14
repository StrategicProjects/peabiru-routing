"""locate_facilities() and floating_catchment(): facility location on the
travel times of a network."""

from __future__ import annotations

import math
import warnings
from typing import Any, Dict, NamedTuple, Optional, Tuple

import numpy as np
import pandas as pd

from . import _peabiru
from ._assign import assign_decay_function, assign_n_threads, assign_points_input
from ._checks import assert_count, assert_flag, assert_number, assert_string, core_errors, scalar
from ._network import assert_network
from ._routing import _f64, _i32, _strings, travel_time_matrix

MODELS = ("max_cover", "set_cover", "p_median", "p_center")
_NA = np.iinfo(np.int32).min


class FacilityLocation(NamedTuple):
    """The result of [`locate_facilities()`][peabiru.locate_facilities].
    Unpacks as a tuple.

    Attributes
    ----------
    sites : pandas.DataFrame
        The new sites, one row each, ordered by what each adds: ``id``
        (with ``lon`` and ``lat`` when the candidates have them),
        ``order`` (from 1), ``marginal`` (demand newly covered, or by how
        much the objective falls) and ``cumulative`` (demand covered so
        far, existing facilities included, or the objective so far).
    candidates : pandas.DataFrame
        One row per candidate: ``id``, ``potential`` (what it would add as
        the only new facility; for the covering models, the demand not
        covered today that it would cover alone) and ``chosen``.
    demand : pandas.DataFrame
        One row per demand point: ``id``, ``demand`` (its weight),
        ``facility_id`` and ``travel_time`` (the nearest open facility,
        existing or new, and the minutes to it; missing when it reaches
        none) and ``covered`` (within the cutoff of a facility; for
        ``"p_median"`` and ``"p_center"``, whether it reaches one).
    summary : dict
        ``model``, ``n_sites``, ``objective_before`` (existing facilities
        only), ``objective_greedy``, ``objective``, ``covered_before`` and
        ``covered`` (covering models; otherwise ``None``),
        ``total_demand``, ``unservable_demand`` (demand no site can serve)
        and ``n_swaps``.
    """

    sites: pd.DataFrame
    candidates: pd.DataFrame
    demand: pd.DataFrame
    summary: Dict[str, Any]


class FloatingCatchment(NamedTuple):
    """The result of [`floating_catchment()`][peabiru.floating_catchment].
    Unpacks as a tuple.

    Attributes
    ----------
    demand : pandas.DataFrame
        One row per demand point: ``id``, ``demand`` (its weight) and
        ``accessibility`` (supply per unit of demand).
    supply : pandas.DataFrame
        One row per facility: ``id``, ``supply``, ``catchment_demand``
        (the weighted demand in its catchment) and ``ratio`` (supply
        divided by that demand; 0 when the catchment holds no demand).
    """

    demand: pd.DataFrame
    supply: pd.DataFrame


class _Table(NamedTuple):
    ids: np.ndarray
    df: pd.DataFrame
    coords: bool


def _points(df: Any, name: str, routed: bool) -> _Table:
    """The ids of a set of points; with their coordinates checked when
    they are to be routed."""
    if routed:
        p = assign_points_input(df, name)
        return _Table(p.ids, p.df, True)
    if not isinstance(df, pd.DataFrame):
        raise TypeError(f"'{name}' must be a 'DataFrame'.")
    if "id" not in df.columns:
        raise ValueError(f"Assertion on '{name}' failed: Names must include the element 'id'.")
    ids = np.array(df["id"].astype(str).tolist(), dtype=object)
    if pd.Series(ids).duplicated().any():
        raise ValueError(f"`{name}` must not contain duplicated ids.")
    return _Table(ids, df, "lon" in df.columns and "lat" in df.columns)


def _weights(t: _Table, colname: Any, name: str, arg: str) -> np.ndarray:
    """The weight column of a table, or 1 per row without one."""
    assert_string(colname, arg, none_ok=True)
    if colname is None:
        return np.ones(len(t.ids), dtype=np.float64)
    colname = str(scalar(colname))
    if colname not in t.df.columns:
        raise ValueError(f"'{name}' has no column '{colname}' (the {arg} argument).")
    col = t.df[colname]
    if not pd.api.types.is_numeric_dtype(col) or pd.api.types.is_bool_dtype(col):
        raise TypeError(f"Assertion on '{name}${colname}' failed: Must be of type 'numeric'.")
    w = col.to_numpy(dtype=np.float64, na_value=np.nan)
    if not np.isfinite(w).all() or (w < 0).any():
        raise ValueError(
            f"Assertion on '{name}${colname}' failed: Must be finite and not negative, "
            "with no missing values."
        )
    return w


def _pairs(
    network: Any,
    rows: _Table,
    cols: _Table,
    travel_times: Any,
    percentile: Any,
    max_trip_duration: int,
    routing: Dict[str, Any],
    function: str,
) -> Tuple[bytes, bytes, bytes]:
    """The reachable pairs (row, column, minutes) of the table the models
    work on: from `travel_times` when given, else routed."""
    if travel_times is None:
        sites = pd.DataFrame(
            {"id": cols.ids, "lon": cols.df["lon"].to_numpy(), "lat": cols.df["lat"].to_numpy()}
        )
        with warnings.catch_warnings():
            # the points were checked (and warned about) already
            warnings.filterwarnings("ignore", message=".*missing lon/lat.*")
            warnings.filterwarnings("ignore", message=".*forcefully cast.*")
            travel_times = travel_time_matrix(
                network, rows.df, sites, percentiles=percentile,
                max_trip_duration=max_trip_duration, **routing,
            )
        column = [c for c in travel_times.columns if c.startswith("travel_time")][0]
    else:
        if not isinstance(travel_times, pd.DataFrame):
            raise TypeError("'travel_times' must be a 'DataFrame'.")
        column = f"travel_time_p{int(scalar(percentile)):02d}"
        if column not in travel_times.columns:
            column = "travel_time"
        missing = [c for c in ("from_id", "to_id", column) if c not in travel_times.columns]
        if missing:
            raise ValueError(
                "'travel_times' must have the columns 'from_id', 'to_id' and "
                f"'travel_time_p{int(scalar(percentile)):02d}' (or 'travel_time'); missing: "
                + ", ".join(f"'{c}'" for c in missing)
                + "."
            )
    row_of = pd.Series(np.arange(len(rows.ids)), index=pd.Index(rows.ids, dtype=object))
    col_of = pd.Series(np.arange(len(cols.ids)), index=pd.Index(cols.ids, dtype=object))
    i = travel_times["from_id"].astype(object).map(row_of).to_numpy(dtype=np.float64, na_value=np.nan)
    j = travel_times["to_id"].astype(object).map(col_of).to_numpy(dtype=np.float64, na_value=np.nan)
    t = pd.to_numeric(travel_times[column], errors="coerce").to_numpy(
        dtype=np.float64, na_value=np.nan
    )
    keep = np.isfinite(i) & np.isfinite(j) & np.isfinite(t)
    if len(travel_times) > 0 and not (np.isfinite(i) & np.isfinite(j)).any():
        raise ValueError(
            f"{function}(): no row of 'travel_times' joins a demand point ('from_id') "
            "to a site ('to_id')."
        )
    return (
        i[keep].astype(np.int32).tobytes(),
        j[keep].astype(np.int32).tobytes(),
        np.ascontiguousarray(t[keep]).tobytes(),
    )


def _check_source(network: Any, travel_times: Any, routing: Dict[str, Any], function: str) -> bool:
    """Whether the travel times are to be routed."""
    if travel_times is not None:
        if routing:
            raise TypeError(
                f"{function}() got routing arguments with 'travel_times': "
                + ", ".join(sorted(routing))
                + "."
            )
        return False
    for refused in ("percentiles", "output_dir"):
        if refused in routing:
            raise TypeError(
                f"{function}() does not take '{refused}'"
                + (" (use 'percentile')." if refused == "percentiles" else ".")
            )
    if network is None:
        raise ValueError(f"{function}() needs a 'network' or a table of 'travel_times'.")
    assert_network(network)
    return True


def _with_coords(t: _Table, data: Dict[str, Any], keep: Any = slice(None)) -> Dict[str, Any]:
    out: Dict[str, Any] = {"id": _strings(t.ids[keep])}
    if t.coords:
        out["lon"] = t.df["lon"].to_numpy(dtype=np.float64, na_value=np.nan)[keep]
        out["lat"] = t.df["lat"].to_numpy(dtype=np.float64, na_value=np.nan)[keep]
    out.update(data)
    return out


def locate_facilities(
    network: Any,
    demand: Any,
    candidates: Any,
    existing: Any = None,
    model: str = "max_cover",
    n_facilities: Any = 1,
    cutoff: Any = None,
    demand_colname: Optional[str] = None,
    mode: Any = "WALK",
    mode_egress: str = "WALK",
    departure_datetime: Optional[Any] = None,
    percentile: Any = 50,
    max_trip_duration: Any = None,
    travel_times: Any = None,
    swap: bool = True,
    min_gain: Any = 0,
    unreachable_time: Any = None,
    n_threads: Any = None,
    **routing: Any,
) -> FacilityLocation:
    """Where to open new facilities.

    Chooses, among the candidate sites, where to open facilities so that
    the demand points are served best, by one of four classic location
    models. The travel times from every demand point to every site are
    computed with [`travel_time_matrix()`][peabiru.travel_time_matrix]
    (or taken from ``travel_times``), so any mode, departure time and
    routing option of the matrix applies.

    The models:

    - ``"max_cover"``, maximal covering: with ``n_facilities`` new
      facilities, cover the most demand within ``cutoff`` minutes.
    - ``"set_cover"``, location set covering: the fewest new facilities
      that leave every demand point within ``cutoff`` minutes of one
      (every point some site can cover; the others are reported).
    - ``"p_median"``: ``n_facilities`` new facilities with the smallest
      demand-weighted sum of travel times to the nearest facility.
    - ``"p_center"``: ``n_facilities`` new facilities with the smallest
      travel time for the demand point farthest from its nearest
      facility.

    The ``existing`` facilities are already open and count first: only the
    demand they leave uncovered, or the travel time they leave, drives the
    choice of the new sites.

    The models are solved by heuristics: a greedy construction (each site
    the one that improves the objective most, given the ones before it)
    followed, with ``swap``, by vertex substitution (swap an open new site
    for a closed candidate while that improves the objective). The
    solution is good and reproducible, not proven optimal; for maximal
    covering the greedy solution covers at least 63% (1 - 1/e) of the
    most that could be covered of the demand not yet covered. Ties go to
    the candidate listed first, and the result does not depend on
    ``n_threads``. See the guide [Facility location](facility-location.md).

    Parameters
    ----------
    network : Network or None
        The network of [`build_network()`][peabiru.build_network]; may be
        ``None`` with ``travel_times``.
    demand : pandas.DataFrame or geopandas.GeoDataFrame
        The demand points: ``id``, ``lon``, ``lat`` (or point geometries
        in WGS 84) and, with ``demand_colname``, their weights.
    candidates : pandas.DataFrame or geopandas.GeoDataFrame
        The sites where a facility could open: ``id``, ``lon``, ``lat``.
    existing : pandas.DataFrame or geopandas.GeoDataFrame, optional
        The facilities already open: ``id``, ``lon``, ``lat``. Their ids
        may not repeat those of the candidates.
    model : str, default "max_cover"
        ``"max_cover"``, ``"set_cover"``, ``"p_median"`` or ``"p_center"``.
    n_facilities : int, default 1
        How many new facilities to open. Fewer come back when no further
        site improves the objective. Ignored by ``"set_cover"``.
    cutoff : number
        The service standard of the covering models, in minutes: a point
        is covered by a site at most this far (``travel time <= cutoff``).
        Required by ``"max_cover"`` and ``"set_cover"``.
    demand_colname : str, optional
        The column of ``demand`` with the weight of each point (residents,
        pupils...): finite and not negative. Without it every point
        weighs 1. In ``"set_cover"`` and ``"p_center"`` a point with
        weight 0 is not demand, and the others count the same.
    mode, mode_egress, departure_datetime
        How the demand points travel to the sites, as in
        [`travel_time_matrix()`][peabiru.travel_time_matrix] (default: on
        foot).
    percentile : int, default 50
        The travel-time percentile used (public transport over a time
        window).
    max_trip_duration : int, optional
        The longest trip considered, in minutes. Default: ``cutoff``
        (rounded up) for the covering models, 120 for the others. A demand
        point farther than this from every facility counts as reaching
        none.
    travel_times : pandas.DataFrame, optional
        A table of travel times to use instead of routing: ``from_id``
        (demand point), ``to_id`` (site) and ``travel_time_pXX`` for the
        ``percentile`` (or ``travel_time``), in minutes, as
        [`travel_time_matrix()`][peabiru.travel_time_matrix] returns it.
        A pair that is absent or has a missing time is unreachable; rows
        of other points are ignored. With it, ``network`` and the routing
        arguments are not used and the points need only their ``id``.
    swap : bool, default True
        Improve the greedy solution by vertex substitution (for
        ``"set_cover"``, drop the sites the others make redundant).
    min_gain : number, default 0
        ``"max_cover"`` stops opening sites when the best one left would
        cover less than this much demand.
    unreachable_time : number, optional
        For ``"p_median"`` and ``"p_center"``, the minutes charged to a
        demand point that reaches no open facility. Default: twice the
        longest travel time of the table.
    n_threads : int, optional
        Threads (default: all cores).
    **routing
        Any other argument of
        [`travel_time_matrix()`][peabiru.travel_time_matrix]:
        ``time_window``, ``walk_speed``, ``max_walk_time``, ``seed`` and
        so on (not ``percentiles`` or ``output_dir``).

    Returns
    -------
    FacilityLocation
        Four tables: ``sites``, ``candidates``, ``demand`` and
        ``summary`` (see [`FacilityLocation`][peabiru.FacilityLocation]).

    Examples
    --------
    Two new schools among the 27 sample points, so that the most
    residents are within a 15-minute walk of a school:

    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path())
    >>> points = peabiru.sample_points()
    >>> schools = points[points["schools"] > 0]
    >>> others = points[points["schools"] == 0]
    >>> found = peabiru.locate_facilities(
    ...     network, points, others, existing=schools,
    ...     model="max_cover", n_facilities=2, cutoff=15,
    ...     demand_colname="population", mode="WALK",
    ... )
    >>> list(found.sites.columns)
    ['id', 'lon', 'lat', 'order', 'marginal', 'cumulative']
    >>> len(found.sites) <= 2
    True
    >>> found.summary["covered"] >= found.summary["covered_before"]
    True
    """
    function = "locate_facilities"
    routed = _check_source(network, travel_times, routing, function)
    assert_string(model, "model")
    model = str(scalar(model))
    if model not in MODELS:
        raise ValueError(
            "Assertion on 'model' failed: Must be element of set "
            "{'max_cover','set_cover','p_median','p_center'}, but is " + repr(model) + "."
        )
    covering = model in ("max_cover", "set_cover")
    assert_count(n_facilities, "n_facilities", positive=model != "set_cover")
    if covering:
        if cutoff is None:
            raise ValueError(
                f"Assertion on 'cutoff' failed: Must not be None when model is {model} "
                "(the minutes within which a demand point is covered)."
            )
        assert_number(cutoff, "cutoff", lower=0, finite=True)
        cutoff = float(scalar(cutoff))
    elif cutoff is not None:
        raise ValueError(
            f"Assertion on 'cutoff' failed: Must be None when model is {model} "
            "(the model has no service standard)."
        )
    assert_flag(swap, "swap")
    assert_number(min_gain, "min_gain", lower=0, finite=True)
    assert_number(unreachable_time, "unreachable_time", lower=0, finite=True, none_ok=True)
    assert_count(percentile, "percentile", positive=True)
    if max_trip_duration is not None:
        assert_count(max_trip_duration, "max_trip_duration", positive=True)
    threads = assign_n_threads(n_threads)

    d = _points(demand, "demand", routed)
    c = _points(candidates, "candidates", routed)
    e = None if existing is None else _points(existing, "existing", routed)
    w = _weights(d, demand_colname, "demand", "demand_colname")
    n_existing = 0 if e is None else len(e.ids)
    if e is not None:
        shared = sorted(set(e.ids.tolist()) & set(c.ids.tolist()))
        if shared:
            raise ValueError(
                "`candidates` and `existing` must not share ids.\n✖ Shared id"
                + ("" if len(shared) == 1 else "s")
                + ": "
                + ", ".join(repr(s) for s in shared[:10])
                + "."
            )
    if len(c.ids) == 0:
        raise ValueError("`candidates` must have at least one site.")

    # the sites of the table: the existing facilities, then the candidates
    if e is None:
        sites = c
    else:
        frame = pd.DataFrame({"id": np.concatenate([e.ids, c.ids])})
        if routed:
            frame["lon"] = np.concatenate([e.df["lon"].to_numpy(), c.df["lon"].to_numpy()])
            frame["lat"] = np.concatenate([e.df["lat"].to_numpy(), c.df["lat"].to_numpy()])
        sites = _Table(frame["id"].to_numpy(dtype=object), frame, routed)
    if max_trip_duration is None:
        max_trip_duration = max(1, math.ceil(cutoff)) if covering else 120
    if routed:
        routing = dict(
            routing, mode=mode, mode_egress=mode_egress,
            departure_datetime=departure_datetime, n_threads=n_threads,
        )
    pi, pj, pt = _pairs(
        network, d, sites, travel_times, percentile, int(max_trip_duration), routing, function
    )
    with core_errors(function):
        r = _peabiru.locate_facilities(
            len(d.ids), len(sites.ids), pi, pj, pt, np.ascontiguousarray(w).tobytes(),
            list(range(n_existing)), model, int(scalar(n_facilities)),
            cutoff if covering else math.nan, bool(swap), float(scalar(min_gain)),
            None if unreachable_time is None else float(scalar(unreachable_time)), threads,
        )

    chosen = _i32(r["sites"]) - n_existing
    site_table = pd.DataFrame(
        _with_coords(
            c,
            {
                "order": np.arange(1, len(chosen) + 1, dtype=np.int32),
                "marginal": _f64(r["marginal"]),
                "cumulative": _f64(r["cumulative"]),
            },
            chosen,
        )
    )
    is_chosen = np.zeros(len(c.ids), dtype=bool)
    is_chosen[chosen] = True
    candidate_table = pd.DataFrame(
        _with_coords(c, {"potential": _f64(r["potential"])[n_existing:], "chosen": is_chosen})
    )
    assigned = _i32(r["assigned_site"])
    minutes = _f64(r["assigned_minutes"])
    none = assigned == _NA
    facility = np.empty(len(d.ids), dtype=object)
    facility[~none] = sites.ids[assigned[~none]]
    facility[none] = None
    covered = (minutes <= cutoff) if covering else ~none
    demand_table = pd.DataFrame(
        _with_coords(
            d,
            {
                "demand": w,
                "facility_id": _strings(facility),
                "travel_time": minutes,
                "covered": covered,
            },
        )
    )
    summary = {
        "model": model,
        "n_sites": int(len(chosen)),
        "objective_before": r["objective_before"],
        "objective_greedy": r["objective_greedy"],
        "objective": r["objective"],
        "covered_before": r["covered_before"] if covering else None,
        "covered": r["covered"] if covering else None,
        "total_demand": r["total_demand"],
        "unservable_demand": r["unservable_demand"],
        "n_swaps": r["n_swaps"],
    }
    return FacilityLocation(site_table, candidate_table, demand_table, summary)


def floating_catchment(
    network: Any,
    demand: Any,
    supply: Any,
    cutoff: Any = None,
    decay_function: str = "step",
    decay_value: Optional[float] = None,
    demand_colname: Optional[str] = None,
    supply_colname: Optional[str] = None,
    mode: Any = "WALK",
    mode_egress: str = "WALK",
    departure_datetime: Optional[Any] = None,
    percentile: Any = 50,
    max_trip_duration: Any = None,
    travel_times: Any = None,
    **routing: Any,
) -> FloatingCatchment:
    """Supply against demand around each place: the two-step floating
    catchment area.

    Step 1: the supply of each facility (places, staff, beds) is divided
    by the demand within its catchment. Step 2: each demand point adds up
    the ratios of the facilities within its catchment. The result, per
    demand point, is supply per unit of demand, which accounts both for
    how near the facilities are and for how many others compete for them.

    With ``decay_function="step"`` a pair of demand point and facility
    counts in full up to ``cutoff`` minutes and not at all beyond: the
    two-step floating catchment area in its original form. Any other decay
    function gives the enhanced form, in which nearer pairs weigh more, in
    both steps; the functions and their values are those of
    [`accessibility()`][peabiru.accessibility].

    The sum over the demand points of demand times ``accessibility``
    equals the supply of the facilities that have any demand in their
    catchment.

    Parameters
    ----------
    network : Network or None
        The network of [`build_network()`][peabiru.build_network]; may be
        ``None`` with ``travel_times``.
    demand : pandas.DataFrame or geopandas.GeoDataFrame
        The demand points: ``id``, ``lon``, ``lat`` and, with
        ``demand_colname``, their weights.
    supply : pandas.DataFrame or geopandas.GeoDataFrame
        The facilities: ``id``, ``lon``, ``lat`` and, with
        ``supply_colname``, their capacities.
    cutoff : number
        The catchment in minutes (``travel time <= cutoff`` for
        ``"step"``; the half-life, centre or median of the other
        functions). ``None`` with ``"fixed_exponential"``.
    decay_function : str, default "step"
        ``"step"``, ``"exponential"``, ``"fixed_exponential"``,
        ``"linear"`` or ``"logistic"``.
    decay_value : number, optional
        The parameter of ``"fixed_exponential"``, ``"linear"`` and
        ``"logistic"``, as in [`accessibility()`][peabiru.accessibility].
    demand_colname, supply_colname : str, optional
        The columns with the weight of each demand point and the capacity
        of each facility; without them each counts 1.
    mode, mode_egress, departure_datetime
        How the demand points travel to the facilities, as in
        [`travel_time_matrix()`][peabiru.travel_time_matrix] (default: on
        foot).
    percentile : int, default 50
        The travel-time percentile used.
    max_trip_duration : int, optional
        The longest trip considered, in minutes. Default: ``cutoff``
        (rounded up) for ``"step"``, 120 for the other functions, which
        still weight the trips beyond the cutoff.
    travel_times : pandas.DataFrame, optional
        A table of travel times to use instead of routing (see
        [`locate_facilities()`][peabiru.locate_facilities]).
    **routing
        Any other argument of
        [`travel_time_matrix()`][peabiru.travel_time_matrix].

    Returns
    -------
    FloatingCatchment
        Two tables: ``demand`` and ``supply`` (see
        [`FloatingCatchment`][peabiru.FloatingCatchment]).

    Examples
    --------
    Schools per resident within a 20-minute walk, for the 27 sample
    points:

    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path())
    >>> points = peabiru.sample_points()
    >>> schools = points[points["schools"] > 0]
    >>> fca = peabiru.floating_catchment(
    ...     network, points, schools, cutoff=20,
    ...     demand_colname="population", supply_colname="schools", mode="WALK",
    ... )
    >>> list(fca.demand.columns)
    ['id', 'lon', 'lat', 'demand', 'accessibility']
    >>> list(fca.supply.columns)
    ['id', 'lon', 'lat', 'supply', 'catchment_demand', 'ratio']
    """
    function = "floating_catchment"
    routed = _check_source(network, travel_times, routing, function)
    fun, value = assign_decay_function(decay_function, decay_value)
    if fun == "fixed_exponential":
        if cutoff is not None:
            raise ValueError(
                "Assertion on cutoff failed: must be None when decay_function "
                "is fixed_exponential."
            )
        cut = math.nan
    else:
        if cutoff is None:
            raise ValueError(
                "Assertion on cutoff failed: must not be None when decay_function "
                f"is {fun}."
            )
        assert_number(cutoff, "cutoff", lower=0, finite=True)
        cut = float(scalar(cutoff))
        if cut <= 0:
            raise ValueError("Assertion on 'cutoff' failed: Must be greater than 0.")
    assert_count(percentile, "percentile", positive=True)
    if max_trip_duration is not None:
        assert_count(max_trip_duration, "max_trip_duration", positive=True)
    d = _points(demand, "demand", routed)
    s = _points(supply, "supply", routed)
    w = _weights(d, demand_colname, "demand", "demand_colname")
    cap = _weights(s, supply_colname, "supply", "supply_colname")
    if max_trip_duration is None:
        max_trip_duration = max(1, math.ceil(cut)) if fun == "step" else 120
    if routed:
        routing = dict(
            routing, mode=mode, mode_egress=mode_egress,
            departure_datetime=departure_datetime,
        )
    pi, pj, pt = _pairs(
        network, d, s, travel_times, percentile, int(max_trip_duration), routing, function
    )
    with core_errors(function):
        r = _peabiru.floating_catchment(
            len(d.ids), len(s.ids), pi, pj, pt,
            np.ascontiguousarray(w).tobytes(), np.ascontiguousarray(cap).tobytes(),
            cut, fun, value,
        )
    demand_table = pd.DataFrame(
        _with_coords(d, {"demand": w, "accessibility": _f64(r["index"])})
    )
    supply_table = pd.DataFrame(
        _with_coords(
            s,
            {
                "supply": cap,
                "catchment_demand": _f64(r["catchment_demand"]),
                "ratio": _f64(r["ratio"]),
            },
        )
    )
    return FloatingCatchment(demand_table, supply_table)
