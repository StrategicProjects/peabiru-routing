"""The routing functions of r5r 2.5.0: travel-time matrices, accessibility,
expanded and arrival matrices."""

from __future__ import annotations

import datetime as _dt
import math
import os
from typing import Any, Dict, List, Optional, Sequence, Union

import numpy as np
import pandas as pd

from . import _peabiru
from ._assign import (
    Points,
    assign_cutoffs,
    assign_decay_function,
    assign_departure,
    assign_max_street_time,
    assign_max_trip_duration,
    assign_mode,
    assign_n_threads,
    assign_opportunities,
    assign_percentiles,
    assign_points_input,
    check_draws,
    check_fares,
    check_max_lts,
    check_messages,
    check_output_dir,
    check_scenarios,
    check_speed,
    check_transit_availability_on_date,
    minus_minutes,
    seconds_to_hhmmss,
    seed_arg,
    swap_od,
)
from ._checks import (
    assert_count,
    assert_flag,
    assert_number,
    core_errors,
    fail_assert,
    unsupported,
)
from ._network import Network, assert_network

Mode = Union[str, Sequence[str]]
# Numbers, and also the integer arguments: as in r5r, a whole-valued float
# such as 60.0 is accepted (and math.inf for n_threads and the street time
# limits); the checks reject a fractional value at run time.
Number = Union[int, float]
# output_dir: a directory, as a string or a path-like object
PathArg = Union[str, "os.PathLike[str]"]

_NA = _peabiru.NA_I32


def _i32(b: bytes) -> np.ndarray:
    return np.frombuffer(b, dtype=np.int32)


def _f64(b: bytes) -> np.ndarray:
    return np.frombuffer(b, dtype=np.float64)


def _strings(values: np.ndarray) -> pd.Series:
    """A column of strings, always of object dtype (``None`` for missing),
    whatever pandas' default string dtype is."""
    return pd.Series(values, dtype=object, copy=False)


def _nullable_int(values: np.ndarray, mask: np.ndarray) -> pd.arrays.IntegerArray:
    return pd.arrays.IntegerArray(values.astype(np.int32), mask)


def _csv_output(
    out_dir: Optional[str], o: Points, d: Points, third: Optional[List[str]] = None
) -> Any:
    """The ``output`` argument of the extension's routing functions:
    ``None`` to return the result, or where to write r5r's CSV files (one
    per origin, named after its id) and with which ids (``third`` replaces
    the destination ids: the opportunity names of accessibility). r5r's
    Java side fails on a missing id; so does this."""
    if out_dir is None:
        return None
    if any(i is None for i in o.ids) or any(i is None for i in d.ids):
        raise ValueError(
            "With 'output_dir', the ids of 'origins' and 'destinations' cannot be missing."
        )
    return (out_dir, list(o.ids), list(d.ids) if third is None else third)


def _street_limits(
    max_walk_time: Any,
    max_bike_time: Any,
    max_car_time: Any,
    walk_speed: Any,
    bike_speed: Any,
    max_trip_duration: Any,
    mode_list: Any,
) -> Dict[str, int]:
    walk = assign_max_street_time(max_walk_time, walk_speed, max_trip_duration, "walk")
    bike = assign_max_street_time(max_bike_time, bike_speed, max_trip_duration, "bike")
    car = assign_max_street_time(max_car_time, None, max_trip_duration, "car")
    mtd = assign_max_trip_duration(max_trip_duration, mode_list, walk, bike, car)
    return {"walk": walk, "bike": bike, "car": car, "trip": mtd}


def _common_checks(
    draws_per_minute: Any,
    seed: Any,
    walk_speed: Any,
    bike_speed: Any,
    max_rides: Any,
    max_lts: Any,
    n_threads: Any,
    verbose: Any,
    progress: Any,
) -> Dict[str, Any]:
    check_draws(draws_per_minute)
    s = seed_arg(seed)
    check_speed(walk_speed, "walk")
    check_speed(bike_speed, "bike")
    assert_count(max_rides, "max_rides", positive=True)
    check_max_lts(max_lts)
    threads = assign_n_threads(n_threads)
    check_messages(verbose, progress)
    return {"seed": s, "threads": threads}


def travel_time_matrix(
    r5r_network: Network,
    origins: pd.DataFrame,
    destinations: pd.DataFrame,
    mode: Mode = "WALK",
    mode_egress: str = "WALK",
    departure_datetime: Optional[Any] = None,
    time_window: Number = 10,
    percentiles: Union[int, Sequence[int]] = 50,
    max_walk_time: Number = math.inf,
    max_bike_time: Number = math.inf,
    max_car_time: Number = math.inf,
    max_trip_duration: Number = 120,
    walk_speed: float = 3.6,
    bike_speed: float = 12,
    max_rides: Number = 3,
    max_lts: Number = 2,
    fare_structure: Any = None,
    max_fare: Number = math.inf,
    new_carspeeds: Optional[pd.DataFrame] = None,
    carspeed_scale: float = 1,
    new_lts: Optional[pd.DataFrame] = None,
    draws_per_minute: Number = 5,
    seed: Optional[Number] = None,
    n_threads: Optional[Number] = None,
    verbose: bool = False,
    progress: bool = True,
    output_dir: Optional[PathArg] = None,
) -> Union[pd.DataFrame, PathArg]:
    """Travel-time matrix between origins and destinations.

    Travel times between every origin and every destination, by one street
    mode (``"WALK"``, ``"BICYCLE"`` or ``"CAR"``) or by public transport
    with walk access and egress (``["WALK", "TRANSIT"]``). Arguments,
    defaults and the result are those of ``r5r::travel_time_matrix()``
    (r5r 2.5.0), and so are the values: peabiru reproduces R5's routing
    exactly.

    Not supported yet, and an [`UnsupportedError`][peabiru.UnsupportedError]
    when requested: transit with ``"BICYCLE"`` or ``"CAR"`` access or a
    non-walk ``mode_egress``; a subset of transit modes (such as ``"BUS"``);
    fares (``fare_structure``, ``max_fare``); scenarios (``new_carspeeds``,
    ``carspeed_scale``, ``new_lts``).

    Parameters
    ----------
    r5r_network : Network
        A network built with [`build_network()`][peabiru.build_network].
    origins : pandas.DataFrame
        Points with columns ``id``, ``lon`` and ``lat`` (WGS 84), or a
        ``geopandas.GeoDataFrame`` of points in EPSG:4326 with an ``id``
        column. Ids that are not strings are converted with ``str()``, with
        a warning; points with missing coordinates are not routed, with a
        warning; duplicated ids are an error.
    destinations : pandas.DataFrame
        Points, as ``origins``.
    mode : str or sequence of str, default "WALK"
        One of ``"WALK"``, ``"BICYCLE"`` or ``"CAR"``, or ``"TRANSIT"``
        (alone or with ``"WALK"``).
    mode_egress : str, default "WALK"
        The mode from the last stop to the destination. Only ``"WALK"`` is
        supported with transit.
    departure_datetime : datetime.datetime, optional
        The departure date and time. Its date and clock time are taken as
        they read, in its own time zone when it is timezone-aware. Required
        with transit.
    time_window : int, default 10
        The window of departure minutes, starting at
        ``departure_datetime``, over which travel times are computed.
    percentiles : int or sequence of int, default 50
        Up to 5 distinct values from 1 to 99, the travel-time percentiles
        over the time window to report.
    max_walk_time : int or float, default math.inf
        The maximum walking time in minutes (``math.inf`` means the trip
        limit).
    max_bike_time : int or float, default math.inf
        The maximum cycling time in minutes (``math.inf`` means the trip
        limit).
    max_car_time : int or float, default math.inf
        The maximum driving time in minutes (``math.inf`` means the trip
        limit).
    max_trip_duration : int, default 120
        The maximum trip duration in minutes.
    walk_speed : float, default 3.6
        Walking speed in km/h.
    bike_speed : float, default 12
        Cycling speed in km/h.
    max_rides : int, default 3
        The maximum number of public transport rides.
    max_lts : int, default 2
        The highest level of traffic stress (1 to 4) a cyclist will ride on.
    fare_structure : None
        Fares are not supported yet; must be ``None``.
    max_fare : float, default math.inf
        Fares are not supported yet; must be ``math.inf``.
    new_carspeeds : None
        Scenarios are not supported yet; must be ``None``.
    carspeed_scale : float, default 1
        Scenarios are not supported yet; must be ``1``.
    new_lts : None
        Scenarios are not supported yet; must be ``None``.
    draws_per_minute : int, default 5
        The Monte Carlo draws per departure minute for frequency-based GTFS.
    seed : int, optional
        A seed for the Monte Carlo draws. With frequency-based GTFS, results
        are reproducible (and equal to r5r's) only with a seed.
    n_threads : int or math.inf, optional
        The number of threads; ``None`` (the default) or ``math.inf`` use
        all cores.
    verbose : bool, default False
        Checked as in r5r; has no effect.
    progress : bool, default True
        Checked as in r5r; peabiru prints no progress counter.
    output_dir : str or os.PathLike, optional
        An existing directory: instead of returning the result, write it
        there as r5r does, one CSV file per origin named
        ``from_<origin id>.csv`` (see [Writing results to
        files](travel-time-matrices.md#writing-results-to-files)), and return
        ``output_dir``. Origins are routed a slice at a time, so the whole
        result is never held in memory.

    Returns
    -------
    pandas.DataFrame or str or os.PathLike
        With ``output_dir``, ``output_dir`` itself. Otherwise:
        ``from_id``, ``to_id`` (strings, object dtype) and one ``Int32``
        column ``travel_time_pXX`` per percentile, ascending (minutes). Rows
        are origin-major, in input order. Pairs that cannot be completed
        within ``max_trip_duration`` are not returned; a higher percentile
        beyond the limit is ``<NA>``.

    Examples
    --------
    >>> import datetime as dt
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> points = peabiru.sample_points()
    >>> ttm = peabiru.travel_time_matrix(
    ...     network, points, points,
    ...     mode=["WALK", "TRANSIT"],
    ...     departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    ...     percentiles=[25, 50, 75],
    ...     seed=42,
    ...     n_threads=2,
    ... )
    >>> list(ttm.columns)
    ['from_id', 'to_id', 'travel_time_p25', 'travel_time_p50', 'travel_time_p75']
    """
    # mirrors r5r R/travel_time_matrix.R
    o = assign_points_input(origins, "origins")
    d = assign_points_input(destinations, "destinations")
    mode_list = assign_mode(mode, mode_egress)
    departure = assign_departure(departure_datetime, mode_list)

    assert_network(r5r_network)
    if mode_list.transit:
        check_transit_availability_on_date(r5r_network, departure)

    # as in r5r, not with output_dir: the files are named after the origins
    # routed from
    swapped = output_dir is None and swap_od(o, d, mode_list, r5r_network)
    if swapped:
        o, d = d, o

    lim = _street_limits(
        max_walk_time, max_bike_time, max_car_time, walk_speed, bike_speed,
        max_trip_duration, mode_list,
    )
    mtd = lim["trip"]

    assert_count(time_window, "time_window", positive=True)
    pcts = assign_percentiles(percentiles)
    c = _common_checks(
        draws_per_minute, seed, walk_speed, bike_speed, max_rides, max_lts,
        n_threads, verbose, progress,
    )
    check_fares(fare_structure, max_fare)
    out_dir = check_output_dir(output_dir)
    check_scenarios(new_carspeeds, carspeed_scale, new_lts)

    names = [f"travel_time_p{p:02d}" for p in pcts]
    core = r5r_network._core
    output = _csv_output(out_dir, o, d)
    if mode_list.transit:
        with core_errors("travel_time_matrix"):
            fb, tb, cols = _peabiru.transit_ttm(
                core, o.lat_bytes(), o.lon_bytes(), d.lat_bytes(), d.lon_bytes(),
                departure.yyyymmdd, departure.seconds, int(time_window), pcts,
                lim["walk"], mtd, float(walk_speed), int(max_rides),
                int(draws_per_minute), c["seed"], c["threads"], output,
            )
        times = [_i32(b) for b in cols]
    else:
        with core_errors("travel_time_matrix"):
            fb, tb, tt = _peabiru.street_ttm(
                core, o.lat_bytes(), o.lon_bytes(), d.lat_bytes(), d.lon_bytes(),
                mode_list.direct_modes, float(walk_speed), float(bike_speed),
                int(max_lts), mtd, c["threads"], pcts, output,
            )
        # a street mode has one travel time, reported for every percentile
        times = [_i32(tt)] * len(pcts)
    if output_dir is not None:
        return output_dir
    frm, to = _i32(fb), _i32(tb)

    if swapped:
        # r5r reverse_back_if_direct_mode(): the routed destinations are the
        # caller's origins
        from_id, to_id = d.ids[to], o.ids[frm]
    else:
        from_id, to_id = o.ids[frm], d.ids[to]
    data: Dict[str, Any] = {"from_id": _strings(from_id), "to_id": _strings(to_id)}
    for name, t in zip(names, times):
        # r5r: travel times of non-viable trips become NA
        data[name] = _nullable_int(t, (t == _NA) | (t > mtd))
    return pd.DataFrame(data)


def accessibility(
    r5r_network: Network,
    origins: pd.DataFrame,
    destinations: pd.DataFrame,
    opportunities_colnames: Union[str, Sequence[str]] = "opportunities",
    mode: Mode = "WALK",
    mode_egress: str = "WALK",
    departure_datetime: Optional[Any] = None,
    time_window: Number = 10,
    percentiles: Union[int, Sequence[int]] = 50,
    decay_function: str = "step",
    cutoffs: Optional[Union[int, Sequence[int]]] = None,
    decay_value: Optional[float] = None,
    max_walk_time: Number = math.inf,
    max_bike_time: Number = math.inf,
    max_car_time: Number = math.inf,
    max_trip_duration: Number = 120,
    walk_speed: float = 3.6,
    bike_speed: float = 12,
    max_rides: Number = 3,
    max_lts: Number = 2,
    fare_structure: Any = None,
    max_fare: Number = math.inf,
    new_carspeeds: Optional[pd.DataFrame] = None,
    carspeed_scale: float = 1,
    new_lts: Optional[pd.DataFrame] = None,
    draws_per_minute: Number = 5,
    seed: Optional[Number] = None,
    n_threads: Optional[Number] = None,
    verbose: bool = False,
    progress: bool = True,
    output_dir: Optional[PathArg] = None,
) -> Union[pd.DataFrame, PathArg]:
    """Access to opportunities.

    Cumulative or decay-weighted access to the opportunities at the
    destinations from each origin, by one street mode (``"WALK"``,
    ``"BICYCLE"`` or ``"CAR"``) or by public transport with walk access and
    egress (``["WALK", "TRANSIT"]``). Arguments, defaults and the result are
    those of ``r5r::accessibility()`` (r5r 2.5.0); the values are
    bit-identical.

    As in r5r, with the ``"step"`` function trips longer than the largest
    cutoff are not considered (``max_trip_duration`` becomes
    ``max(cutoffs)``), while the other functions still weight them; and
    ``max_trip_duration`` may not be shorter than the largest cutoff.

    Not supported yet, and an [`UnsupportedError`][peabiru.UnsupportedError] when requested:
    transit with ``"BICYCLE"`` or ``"CAR"`` access or a non-walk
    ``mode_egress``; a subset of transit modes; fares; scenarios.

    Parameters
    ----------
    r5r_network : Network
        A network built with [`build_network()`][peabiru.build_network].
    origins : pandas.DataFrame
        Points with columns ``id``, ``lon`` and ``lat`` (WGS 84), or a
        ``geopandas.GeoDataFrame`` of points in EPSG:4326 with an ``id``
        column. Ids that are not strings are converted with ``str()``, with
        a warning; points with missing coordinates are not routed, with a
        warning; duplicated ids are an error.
    destinations : pandas.DataFrame
        Points, as ``origins``, plus the opportunity columns.
    opportunities_colnames : str or sequence of str, default "opportunities"
        The numeric columns of ``destinations`` with the opportunity
        counts. Missing values count as 0, with a warning.
    mode : str or sequence of str, default "WALK"
        One of ``"WALK"``, ``"BICYCLE"`` or ``"CAR"``, or ``"TRANSIT"``
        (alone or with ``"WALK"``).
    mode_egress : str, default "WALK"
        The mode from the last stop to the destination. Only ``"WALK"`` is
        supported with transit.
    departure_datetime : datetime.datetime, optional
        The departure date and time. Its date and clock time are taken as
        they read, in its own time zone when it is timezone-aware. Required
        with transit.
    time_window : int, default 10
        The window of departure minutes, starting at
        ``departure_datetime``, over which travel times are computed.
    percentiles : int or sequence of int, default 50
        Up to 5 distinct values from 1 to 99, the travel-time percentiles
        over the time window to report.
    decay_function : str, default "step"
        ``"step"``, ``"exponential"``, ``"fixed_exponential"``,
        ``"linear"`` or ``"logistic"``.
    cutoffs : int or sequence of int, optional
        Up to 12 distinct values from 1 to 120 (minutes). Must be ``None``
        with ``"fixed_exponential"`` and given otherwise.
    decay_value : float, optional
        The parameter of the decay function: the decay constant for
        ``"fixed_exponential"`` (between 0 and 1), the width in minutes for
        ``"linear"`` (a whole number from 1 to 59), the standard deviation
        in minutes for ``"logistic"`` (at least 1); ``None`` for ``"step"``
        and ``"exponential"``.
    max_walk_time : int or float, default math.inf
        The maximum walking time in minutes (``math.inf`` means the trip
        limit).
    max_bike_time : int or float, default math.inf
        The maximum cycling time in minutes (``math.inf`` means the trip
        limit).
    max_car_time : int or float, default math.inf
        The maximum driving time in minutes (``math.inf`` means the trip
        limit).
    max_trip_duration : int, default 120
        The maximum trip duration in minutes.
    walk_speed : float, default 3.6
        Walking speed in km/h.
    bike_speed : float, default 12
        Cycling speed in km/h.
    max_rides : int, default 3
        The maximum number of public transport rides.
    max_lts : int, default 2
        The highest level of traffic stress (1 to 4) a cyclist will ride on.
    fare_structure : None
        Fares are not supported yet; must be ``None``.
    max_fare : float, default math.inf
        Fares are not supported yet; must be ``math.inf``.
    new_carspeeds : None
        Scenarios are not supported yet; must be ``None``.
    carspeed_scale : float, default 1
        Scenarios are not supported yet; must be ``1``.
    new_lts : None
        Scenarios are not supported yet; must be ``None``.
    draws_per_minute : int, default 5
        The Monte Carlo draws per departure minute for frequency-based GTFS.
    seed : int, optional
        A seed for the Monte Carlo draws. With frequency-based GTFS, results
        are reproducible (and equal to r5r's) only with a seed.
    n_threads : int or math.inf, optional
        The number of threads; ``None`` (the default) or ``math.inf`` use
        all cores.
    verbose : bool, default False
        Checked as in r5r; has no effect.
    progress : bool, default True
        Checked as in r5r; peabiru prints no progress counter.
    output_dir : str or os.PathLike, optional
        An existing directory: instead of returning the result, write it
        there as r5r does, one CSV file per origin named
        ``from_<origin id>.csv`` (see [Writing results to
        files](travel-time-matrices.md#writing-results-to-files)), and return
        ``output_dir``. Origins are routed a slice at a time, so the whole
        result is never held in memory.
    Returns
    -------
    pandas.DataFrame or str or os.PathLike
        With ``output_dir``, ``output_dir`` itself. Otherwise:
        ``id`` and ``opportunity`` (strings), ``percentile`` and ``cutoff``
        (``int32``; ``cutoff`` is absent with ``"fixed_exponential"``) and
        ``accessibility`` (``float64``): one row per origin, opportunity,
        percentile and cutoff, in that order.

    Examples
    --------
    >>> import datetime as dt
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> points = peabiru.sample_points()
    >>> access = peabiru.accessibility(
    ...     network, points, points,
    ...     opportunities_colnames=["jobs", "schools"],
    ...     mode=["WALK", "TRANSIT"],
    ...     departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    ...     decay_function="logistic",
    ...     cutoffs=[15, 30],
    ...     decay_value=5,
    ...     seed=42,
    ...     n_threads=2,
    ... )
    >>> len(access) == len(points) * 2 * 2
    True
    """
    # mirrors r5r R/accessibility.R
    assert_network(r5r_network)
    o = assign_points_input(origins, "origins")
    d = assign_points_input(destinations, "destinations")
    opp_names, opp = assign_opportunities(d.df, opportunities_colnames)
    mode_list = assign_mode(mode, mode_egress)
    departure = assign_departure(departure_datetime, mode_list)
    if mode_list.transit:
        check_transit_availability_on_date(r5r_network, departure)

    fun, value = assign_decay_function(decay_function, decay_value)

    # cap trip duration with cutoffs
    cutoffs_r5 = assign_cutoffs(cutoffs, fun)
    assert_number(max_trip_duration, "max_trip_duration", lower=1, finite=True)
    if cutoffs is not None:
        top = max(cutoffs_r5)
        if max_trip_duration < top:
            raise ValueError("'max_trip_duration' cannot be shorter than 'max(cutoffs)'")
        # only the step function ignores trips longer than the cutoffs; the
        # other decay functions still weight them
        if fun == "step":
            max_trip_duration = top

    lim = _street_limits(
        max_walk_time, max_bike_time, max_car_time, walk_speed, bike_speed,
        max_trip_duration, mode_list,
    )

    assert_count(time_window, "time_window", positive=True)
    pcts = assign_percentiles(percentiles)
    c = _common_checks(
        draws_per_minute, seed, walk_speed, bike_speed, max_rides, max_lts,
        n_threads, verbose, progress,
    )
    check_fares(fare_structure, max_fare)
    out_dir = check_output_dir(output_dir)
    check_scenarios(new_carspeeds, carspeed_scale, new_lts)

    # the third element of an accessibility output holds the opportunity
    # names
    output = _csv_output(out_dir, o, d, list(opp_names))
    with core_errors("accessibility"):
        values = _f64(
            _peabiru.accessibility(
                r5r_network._core, o.lat_bytes(), o.lon_bytes(), d.lat_bytes(), d.lon_bytes(),
                opp, "TRANSIT" if mode_list.transit else mode_list.direct_modes,
                fun, value, cutoffs_r5, pcts,
                departure.yyyymmdd, departure.seconds, int(time_window),
                lim["walk"], lim["trip"], float(walk_speed), float(bike_speed),
                int(max_lts), int(max_rides), int(draws_per_minute), c["seed"], c["threads"],
                output,
            )
        )
    if output_dir is not None:
        return output_dir

    # one row per origin, opportunity, percentile and cutoff, in that order
    n_o, n_k, n_p, n_c = len(o), len(opp_names), len(pcts), len(cutoffs_r5)
    data: Dict[str, Any] = {
        "id": _strings(np.repeat(o.ids, n_k * n_p * n_c)),
        "opportunity": _strings(
            np.tile(np.repeat(np.array(opp_names, dtype=object), n_p * n_c), n_o)
        ),
        "percentile": np.tile(np.repeat(np.array(pcts, dtype=np.int32), n_c), n_o * n_k),
        "cutoff": np.tile(np.array(cutoffs_r5, dtype=np.int32), n_o * n_k * n_p),
        "accessibility": values,
    }
    if fun == "fixed_exponential":
        del data["cutoff"]
    return pd.DataFrame(data)


def expanded_travel_time_matrix(
    r5r_network: Network,
    origins: pd.DataFrame,
    destinations: pd.DataFrame,
    mode: Mode = "WALK",
    mode_egress: str = "WALK",
    departure_datetime: Optional[Any] = None,
    time_window: Number = 10,
    breakdown: bool = False,
    max_walk_time: Number = math.inf,
    max_bike_time: Number = math.inf,
    max_car_time: Number = math.inf,
    max_trip_duration: Number = 120,
    walk_speed: float = 3.6,
    bike_speed: float = 12,
    max_rides: Number = 3,
    max_lts: Number = 2,
    new_carspeeds: Optional[pd.DataFrame] = None,
    carspeed_scale: float = 1,
    new_lts: Optional[pd.DataFrame] = None,
    draws_per_minute: Number = 5,
    seed: Optional[Number] = None,
    n_threads: Optional[Number] = None,
    verbose: bool = False,
    progress: bool = True,
    output_dir: Optional[PathArg] = None,
) -> Union[pd.DataFrame, PathArg]:
    """Minute-by-minute travel times between origins and destinations.

    The travel time of every trip between each origin and destination, for
    every departure minute in the time window (and every Monte Carlo draw
    with frequency-based GTFS), by public transport with walk access and
    egress (``["WALK", "TRANSIT"]``). Arguments, defaults and the result are
    those of ``r5r::expanded_travel_time_matrix()`` (r5r 2.5.0), with the
    same values.

    Only public transport is supported: with a street mode alone r5r
    reports the same trip for every minute of a departure time taken from
    the clock, which peabiru does not reproduce; that is an
    [`UnsupportedError`][peabiru.UnsupportedError], as are transit with ``"BICYCLE"`` or
    ``"CAR"`` access or a non-walk ``mode_egress``, a subset of transit
    modes and scenarios.

    Parameters
    ----------
    r5r_network : Network
        A network built with [`build_network()`][peabiru.build_network].
    origins : pandas.DataFrame
        Points with columns ``id``, ``lon`` and ``lat`` (WGS 84), or a
        ``geopandas.GeoDataFrame`` of points in EPSG:4326 with an ``id``
        column. Ids that are not strings are converted with ``str()``, with
        a warning; points with missing coordinates are not routed, with a
        warning; duplicated ids are an error.
    destinations : pandas.DataFrame
        Points, as ``origins``.
    mode : str or sequence of str, default "WALK"
        ``["WALK", "TRANSIT"]`` or ``"TRANSIT"``; a street mode alone is
        an error.
    mode_egress : str, default "WALK"
        The mode from the last stop to the destination. Only ``"WALK"`` is
        supported with transit.
    departure_datetime : datetime.datetime, optional
        The departure date and time. Its date and clock time are taken as
        they read, in its own time zone when it is timezone-aware. Required
        with transit.
    time_window : int, default 10
        The window of departure minutes, starting at
        ``departure_datetime``, over which travel times are computed.
    breakdown : bool, default False
        Whether to break each trip down into access, waiting, in-vehicle,
        transfer and egress times, and the number of rides.
    max_walk_time : int or float, default math.inf
        The maximum walking time in minutes (``math.inf`` means the trip
        limit).
    max_bike_time : int or float, default math.inf
        The maximum cycling time in minutes (``math.inf`` means the trip
        limit).
    max_car_time : int or float, default math.inf
        The maximum driving time in minutes (``math.inf`` means the trip
        limit).
    max_trip_duration : int, default 120
        The maximum trip duration in minutes.
    walk_speed : float, default 3.6
        Walking speed in km/h.
    bike_speed : float, default 12
        Cycling speed in km/h.
    max_rides : int, default 3
        The maximum number of public transport rides.
    max_lts : int, default 2
        The highest level of traffic stress (1 to 4) a cyclist will ride on.
    new_carspeeds : None
        Scenarios are not supported yet; must be ``None``.
    carspeed_scale : float, default 1
        Scenarios are not supported yet; must be ``1``.
    new_lts : None
        Scenarios are not supported yet; must be ``None``.
    draws_per_minute : int, default 5
        The Monte Carlo draws per departure minute for frequency-based GTFS.
    seed : int, optional
        A seed for the Monte Carlo draws. With frequency-based GTFS, results
        are reproducible (and equal to r5r's) only with a seed.
    n_threads : int or math.inf, optional
        The number of threads; ``None`` (the default) or ``math.inf`` use
        all cores.
    verbose : bool, default False
        Checked as in r5r; has no effect.
    progress : bool, default True
        Checked as in r5r; peabiru prints no progress counter.
    output_dir : str or os.PathLike, optional
        An existing directory: instead of returning the result, write it
        there as r5r does, one CSV file per origin named
        ``from_<origin id>.csv`` (see [Writing results to
        files](travel-time-matrices.md#writing-results-to-files)), and return
        ``output_dir``. Origins are routed a slice at a time, so the whole
        result is never held in memory.
    Returns
    -------
    pandas.DataFrame or str or os.PathLike
        With ``output_dir``, ``output_dir`` itself. Otherwise:
        ``from_id``, ``to_id``, ``departure_time`` (``"HH:MM:SS"``),
        ``draw_number`` (``int32``); with ``breakdown=True``
        ``access_time``, ``wait_time``, ``ride_time``, ``transfer_time``,
        ``egress_time``; then ``routes`` (route ids joined by ``"|"``, or
        ``"[WALK]"``); with ``breakdown=True`` ``n_rides`` (``Int32``); and
        ``total_time``. Times are ``float64`` minutes with one decimal.
        Trips longer than ``max_trip_duration`` have ``NaN`` times,
        ``None`` routes and ``<NA>`` rides. String columns are of object
        dtype.

    Examples
    --------
    >>> import datetime as dt
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> points = peabiru.sample_points()
    >>> ettm = peabiru.expanded_travel_time_matrix(
    ...     network, points.iloc[:2], points,
    ...     mode=["WALK", "TRANSIT"],
    ...     departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    ...     time_window=2,
    ...     breakdown=True,
    ...     seed=42,
    ...     n_threads=2,
    ... )
    >>> sorted(ettm["departure_time"].unique())
    ['14:00:00', '14:01:00']
    """
    # mirrors r5r R/expanded_travel_time_matrix.R
    o = assign_points_input(origins, "origins")
    d = assign_points_input(destinations, "destinations")
    mode_list = assign_mode(mode, mode_egress)
    _check_expanded_mode(mode_list)
    departure = assign_departure(departure_datetime, mode_list)

    assert_network(r5r_network)
    check_transit_availability_on_date(r5r_network, departure)

    lim = _street_limits(
        max_walk_time, max_bike_time, max_car_time, walk_speed, bike_speed,
        max_trip_duration, mode_list,
    )

    assert_count(time_window, "time_window", positive=True)
    c = _common_checks(
        draws_per_minute, seed, walk_speed, bike_speed, max_rides, max_lts,
        n_threads, verbose, progress,
    )
    out_dir = check_output_dir(output_dir)
    assert_flag(breakdown, "breakdown")
    check_scenarios(new_carspeeds, carspeed_scale, new_lts)

    with core_errors("expanded_travel_time_matrix", n_destinations=len(d)):
        res = _peabiru.expanded(
            r5r_network._core, o.lat_bytes(), o.lon_bytes(), d.lat_bytes(), d.lon_bytes(),
            False, departure.yyyymmdd, departure.seconds, int(time_window),
            lim["walk"], lim["trip"], float(walk_speed), int(max_rides),
            int(draws_per_minute), c["seed"], c["threads"], bool(breakdown),
            _csv_output(out_dir, o, d),
        )
    if output_dir is not None:
        return output_dir
    return _expanded_result(res, o, d, bool(breakdown), lim["trip"])


def arrival_travel_time_matrix(
    r5r_network: Network,
    origins: pd.DataFrame,
    destinations: pd.DataFrame,
    mode: Mode = "WALK",
    mode_egress: str = "WALK",
    arrival_datetime: Optional[Any] = None,
    breakdown: bool = False,
    max_walk_time: Number = math.inf,
    max_bike_time: Number = math.inf,
    max_car_time: Number = math.inf,
    max_trip_duration: Number = 120,
    walk_speed: float = 3.6,
    bike_speed: float = 12,
    max_rides: Number = 3,
    max_lts: Number = 2,
    new_carspeeds: Optional[pd.DataFrame] = None,
    carspeed_scale: float = 1,
    new_lts: Optional[pd.DataFrame] = None,
    draws_per_minute: Number = 5,
    seed: Optional[Number] = None,
    n_threads: Optional[Number] = None,
    verbose: bool = False,
    progress: bool = True,
    output_dir: Optional[PathArg] = None,
) -> Union[pd.DataFrame, PathArg]:
    """Travel-time matrix for a time of arrival.

    For each origin and destination, the trip that leaves latest and still
    arrives by ``arrival_datetime``, by public transport with walk access
    and egress (``["WALK", "TRANSIT"]``). Arguments, defaults and the result
    are those of ``r5r::arrival_travel_time_matrix()`` (r5r 2.5.0), with the
    same values: as r5r does, departures are searched minute by minute from
    ``arrival_datetime - max_trip_duration``, which may fall on the day
    before.

    Only public transport is supported (see
    [`expanded_travel_time_matrix()`][peabiru.expanded_travel_time_matrix]); the other unsupported options are
    errors too.

    Parameters
    ----------
    r5r_network : Network
        A network built with [`build_network()`][peabiru.build_network].
    origins : pandas.DataFrame
        Points with columns ``id``, ``lon`` and ``lat`` (WGS 84), or a
        ``geopandas.GeoDataFrame`` of points in EPSG:4326 with an ``id``
        column. Ids that are not strings are converted with ``str()``, with
        a warning; points with missing coordinates are not routed, with a
        warning; duplicated ids are an error.
    destinations : pandas.DataFrame
        Points, as ``origins``.
    mode : str or sequence of str, default "WALK"
        ``["WALK", "TRANSIT"]`` or ``"TRANSIT"``; a street mode alone is
        an error.
    mode_egress : str, default "WALK"
        The mode from the last stop to the destination. Only ``"WALK"`` is
        supported with transit.
    arrival_datetime : datetime.datetime
        The time by which trips must arrive (required; ``None`` is an
        error). Its date and clock time are taken as they read, in its own
        time zone when it is timezone-aware. ``arrival_datetime -
        max_trip_duration`` is computed on the absolute time for an aware
        datetime (as R does with POSIXct) and on the clock time for a
        naive one.
    breakdown : bool, default False
        Whether to break each trip down into access, waiting, in-vehicle,
        transfer and egress times, and the number of rides.
    max_walk_time : int or float, default math.inf
        The maximum walking time in minutes (``math.inf`` means the trip
        limit).
    max_bike_time : int or float, default math.inf
        The maximum cycling time in minutes (``math.inf`` means the trip
        limit).
    max_car_time : int or float, default math.inf
        The maximum driving time in minutes (``math.inf`` means the trip
        limit).
    max_trip_duration : int, default 120
        The maximum trip duration in minutes.
    walk_speed : float, default 3.6
        Walking speed in km/h.
    bike_speed : float, default 12
        Cycling speed in km/h.
    max_rides : int, default 3
        The maximum number of public transport rides.
    max_lts : int, default 2
        The highest level of traffic stress (1 to 4) a cyclist will ride on.
    new_carspeeds : None
        Scenarios are not supported yet; must be ``None``.
    carspeed_scale : float, default 1
        Scenarios are not supported yet; must be ``1``.
    new_lts : None
        Scenarios are not supported yet; must be ``None``.
    draws_per_minute : int, default 5
        The Monte Carlo draws per departure minute for frequency-based GTFS.
    seed : int, optional
        A seed for the Monte Carlo draws. With frequency-based GTFS, results
        are reproducible (and equal to r5r's) only with a seed.
    n_threads : int or math.inf, optional
        The number of threads; ``None`` (the default) or ``math.inf`` use
        all cores.
    verbose : bool, default False
        Checked as in r5r; has no effect.
    progress : bool, default True
        Checked as in r5r; peabiru prints no progress counter.
    output_dir : str or os.PathLike, optional
        An existing directory: instead of returning the result, write it
        there as r5r does, one CSV file per origin named
        ``from_<origin id>.csv`` (see [Writing results to
        files](travel-time-matrices.md#writing-results-to-files)), and return
        ``output_dir``. Origins are routed a slice at a time, so the whole
        result is never held in memory.
    Returns
    -------
    pandas.DataFrame or str or os.PathLike
        With ``output_dir``, ``output_dir`` itself. Otherwise:
        The columns of [`expanded_travel_time_matrix()`][peabiru.expanded_travel_time_matrix]: one row per
        origin and destination reached in time.

    Examples
    --------
    >>> import datetime as dt
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> points = peabiru.sample_points()
    >>> attm = peabiru.arrival_travel_time_matrix(
    ...     network, points.iloc[:2], points,
    ...     mode=["WALK", "TRANSIT"],
    ...     arrival_datetime=dt.datetime(2019, 5, 13, 15, 0),
    ...     max_trip_duration=30,
    ...     seed=42,
    ...     n_threads=2,
    ... )
    >>> bool((attm["total_time"] <= 30).all())
    True
    """
    # mirrors r5r R/arrival_travel_time_matrix.R
    if arrival_datetime is None:
        raise ValueError(
            "'arrival_datetime' is required. Supply a datetime value specifying "
            "when trips must arrive. For trips that do not depend on time, use "
            "travel_time_matrix()."
        )
    if n_threads is not None:
        assert_number(n_threads, "n_threads", lower=1)
    assert_network(r5r_network)

    o = assign_points_input(origins, "origins")
    d = assign_points_input(destinations, "destinations")
    mode_list = assign_mode(mode, mode_egress)
    _check_expanded_mode(mode_list)

    # set before the departure datetime, which depends on the final
    # max_trip_duration
    lim = _street_limits(
        max_walk_time, max_bike_time, max_car_time, walk_speed, bike_speed,
        max_trip_duration, mode_list,
    )
    mtd = lim["trip"]

    # the departure is arrival - max_trip_duration, as r5r computes it
    if not isinstance(arrival_datetime, _dt.datetime) or pd.isna(arrival_datetime):
        fail_assert(
            "arrival_datetime",
            "Must be a single datetime.datetime without missing values",
            TypeError,
        )
    departure = assign_departure(
        minus_minutes(arrival_datetime, mtd), mode_list, "arrival_datetime"
    )
    check_transit_availability_on_date(r5r_network, departure)

    # the time window is max_trip_duration
    c = _common_checks(
        draws_per_minute, seed, walk_speed, bike_speed, max_rides, max_lts,
        n_threads, verbose, progress,
    )
    out_dir = check_output_dir(output_dir)
    assert_flag(breakdown, "breakdown")
    check_scenarios(new_carspeeds, carspeed_scale, new_lts)

    # the engine's time window is max_trip_duration here
    with core_errors(
        "arrival_travel_time_matrix",
        names={"time_window": "max_trip_duration"},
        n_destinations=len(d),
    ):
        res = _peabiru.expanded(
            r5r_network._core, o.lat_bytes(), o.lon_bytes(), d.lat_bytes(), d.lon_bytes(),
            True, departure.yyyymmdd, departure.seconds, mtd,
            lim["walk"], mtd, float(walk_speed), int(max_rides),
            int(draws_per_minute), c["seed"], c["threads"], bool(breakdown),
            _csv_output(out_dir, o, d),
        )
    if output_dir is not None:
        return output_dir
    return _expanded_result(res, o, d, bool(breakdown), mtd)


def _check_expanded_mode(mode_list: Any) -> None:
    if not mode_list.transit:
        unsupported(
            f"An expanded or arrival travel time matrix by {mode_list.direct_modes} "
            "alone (use travel_time_matrix() for street modes)"
        )


_BREAKDOWN = ["access_time", "wait_time", "ride_time", "transfer_time", "egress_time"]


def _expanded_result(
    res: Dict[str, Any], o: Points, d: Points, breakdown: bool, max_trip_duration: int
) -> pd.DataFrame:
    """Column assembly of r5r's expanded and arrival matrices."""
    frm, to = _i32(res["from"]), _i32(res["to"])
    total = _f64(res["total_time"])
    # r5r: replace travel times of non-viable trips with NAs (NaN compares
    # False, so missing values stay missing)
    over = total > max_trip_duration

    def times(name: str) -> np.ndarray:
        v = _f64(res[name])
        return np.where(over, np.nan, v) if over.any() else v

    codes = _i32(res["route_codes"]).copy()
    codes[over] = -1
    # dictionary-decoded: each row references one of the few route strings
    levels = np.array(list(res["route_levels"]) + [None], dtype=object)
    routes = levels[codes]  # code -1 picks the trailing None

    data: Dict[str, Any] = {
        "from_id": _strings(o.ids[frm]),
        "to_id": _strings(d.ids[to]),
        "departure_time": _strings(seconds_to_hhmmss(_i32(res["departure"]))),
        "draw_number": _i32(res["draw"]),
    }
    if breakdown:
        for name in _BREAKDOWN:
            data[name] = times(name)
        data["routes"] = _strings(routes)
        n_rides = _i32(res["n_rides"])
        data["n_rides"] = _nullable_int(n_rides, (n_rides == _NA) | over)
    else:
        data["routes"] = _strings(routes)
    data["total_time"] = times("total_time")
    return pd.DataFrame(data)
