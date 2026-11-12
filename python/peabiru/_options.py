"""trip_options(): peabiru's own multi-criteria trip planner."""

from __future__ import annotations

import math
import os
from typing import Any, NamedTuple, Optional, Sequence, Union

import numpy as np
import pandas as pd

from . import _peabiru
from ._assign import (
    assign_departure,
    assign_mode,
    assign_n_threads,
    assign_points_input,
    check_max_lts,
    check_messages,
    check_output_dir,
    check_speed,
    check_transit_availability_on_date,
)
from ._checks import assert_count, assert_flag, assert_number, core_errors
from ._fares import fare_arg
from ._itinerary import _expand_od_pairs
from ._network import Network, assert_network
from ._routing import _f64, _i32, _street_limits, _strings
from ._scenario import scenario_spec

Number = Union[int, float]
PathArg = Union[str, "os.PathLike[str]"]

CRITERIA = ("TIME", "FARE", "TRANSFERS", "WALK")


class TripOptions(NamedTuple):
    """The result of [`trip_options()`][peabiru.trip_options]: three tables
    (``itineraries`` and ``departures`` are ``None`` unless asked for).
    Unpacks as a tuple."""

    options: pd.DataFrame
    itineraries: Any
    departures: Optional[pd.DataFrame]


def _criteria(criteria: Any, fare_structure: Any) -> list:
    if criteria is None:
        return [c for c in CRITERIA if c != "FARE" or fare_structure is not None]
    if isinstance(criteria, str):
        criteria = [criteria]
    if (
        not isinstance(criteria, Sequence)
        or len(criteria) == 0
        or not all(isinstance(c, str) for c in criteria)
    ):
        raise TypeError(
            "Assertion on 'criteria' failed: Must be a non-empty sequence of strings."
        )
    out = [c.upper() for c in criteria]
    if len(set(out)) != len(out):
        raise ValueError("Assertion on 'criteria' failed: Contains duplicated values.")
    bad = [c for c in out if c not in CRITERIA]
    if bad:
        raise ValueError(
            "Assertion on 'criteria' failed: Must be a subset of {'TIME','FARE','TRANSFERS',"
            f"'WALK'}}, but has additional elements {{{','.join(repr(b) for b in bad)}}}."
        )
    if "FARE" in out and fare_structure is None:
        raise ValueError(
            'The "FARE" criterion needs a fare_structure: create one with '
            "setup_fare_structure() or read_fare_structure()."
        )
    return out


def trip_options(
    r5r_network: Network,
    origins: Any,
    destinations: Any,
    mode: Any = ("WALK", "TRANSIT"),
    mode_egress: str = "WALK",
    departure_datetime: Optional[Any] = None,
    time_window: Number = 10,
    criteria: Optional[Sequence[str]] = None,
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
    max_options: Number = math.inf,
    walk_resolution: Number = 60,
    all_to_all: bool = False,
    itineraries: bool = True,
    by_departure: bool = False,
    new_carspeeds: Optional[Any] = None,
    carspeed_scale: float = 1,
    new_lts: Optional[Any] = None,
    n_threads: Optional[Number] = None,
    verbose: bool = False,
    progress: bool = True,
    drop_geometry: bool = False,
    osm_link_ids: bool = False,
    output_dir: Optional[PathArg] = None,
) -> Any:
    """Trip options between origin-destination pairs.

    For each pair of origin and destination, every trip worth considering:
    the trips that no other trip beats on all the chosen criteria at once.
    This is peabiru's own multi-criteria trip planner, with no counterpart
    in r5r; see the guide [Trip options](trip-options.md) for the
    definitions, a worked example, and how it differs from
    [`pareto_frontier()`][peabiru.pareto_frontier] and
    [`detailed_itineraries()`][peabiru.detailed_itineraries].

    A trip *dominates* another when it is no worse on every chosen
    criterion and better on at least one. At each departure minute of the
    window the options are the exact set of trips not dominated (one per
    combination of values; of trips equal on the chosen criteria, the one
    with the least total time, then transfers, minutes on foot and fare),
    compared with total time to a tenth of a minute and walking to the
    minute (``walk_resolution``), so that trips a few seconds apart are not
    all returned; the search is exact and the values returned are exact.
    The minutes are merged into options: trips with the same street modes,
    the same routes between the same stops and the same fare, transfers and
    minutes on foot are one option, reported at its best minute (the least
    total time). Frequency-based routes are boarded at their expected times
    (half a headway of waiting; the scheduled departures with
    ``exact_times = 1``): no random draws, no seed.

    Parameters
    ----------
    r5r_network : Network
        A network built with [`build_network()`][peabiru.build_network].
    origins, destinations : pandas.DataFrame
        Points with columns ``id``, ``lon`` and ``lat``, or a
        ``geopandas.GeoDataFrame`` of points; paired row by row unless
        ``all_to_all``.
    mode : str or sequence of str, default ("WALK", "TRANSIT")
        The modes, as in [`detailed_itineraries()`][peabiru.detailed_itineraries].
        Without a public transport mode the only option is the direct trip.
    criteria : sequence of str, optional
        Some of ``"TIME"`` (door-to-door minutes from the departure minute),
        ``"FARE"`` (needs a ``fare_structure``), ``"TRANSFERS"`` (rides minus
        one; 0 without public transport) and ``"WALK"`` (minutes on foot:
        walked access and egress legs and walks between stops; bicycle and
        car legs count 0). Default: all four with a ``fare_structure``, all
        but ``"FARE"`` without one.
    mode_egress, departure_datetime, time_window, max_walk_time, max_bike_time, max_car_time, max_trip_duration, walk_speed, bike_speed, max_rides, max_lts, all_to_all, fare_structure, max_fare, new_carspeeds, carspeed_scale, new_lts, n_threads, verbose, progress, osm_link_ids
        As in [`detailed_itineraries()`][peabiru.detailed_itineraries].
    max_options : int or math.inf, default math.inf
        At most this many options per pair, the first in the order of
        ``option``.
    walk_resolution : int, default 60
        Seconds to which minutes on foot are compared: by default walking is
        compared to the whole minute (rounded), so that of two trips a few
        seconds of walking apart and otherwise no better, only one is
        returned; 1 compares the exact seconds. The search is exact, and the
        values returned are exact.
    itineraries : bool, default True
        Whether to build the legs of each option.
    by_departure : bool, default False
        Whether to add the options of every departure minute.
    drop_geometry : bool, default False
        Whether to leave the lines of the legs out (a ``pandas.DataFrame``).
    output_dir : str or os.PathLike, optional
        Write one CSV file per pair with options,
        ``from_<origin id>_to_<destination id>.csv`` (one row per leg, or per
        option without itineraries, the geometry as WKT), and return
        ``output_dir``.

    Returns
    -------
    TripOptions
        A named tuple ``(options, itineraries, departures)``:
        ``options`` has one row per option (``from_id``, ``to_id``,
        ``option``, ``total_time`` and ``walk_time`` in minutes, ``fare``
        with a ``fare_structure``, ``transfers``, ``departure_time`` and
        ``arrival_time`` of the best minute, ``n_departures``,
        ``first_departure``, ``last_departure``, ``description``);
        ``itineraries`` the legs of each option in the format of
        ``detailed_itineraries()`` keyed by ``from_id``, ``to_id`` and
        ``option`` (a ``geopandas.GeoDataFrame``, or a ``pandas.DataFrame``
        with ``drop_geometry``; ``None`` with ``itineraries=False``);
        ``departures`` (with ``by_departure``) the options of every minute.
        Integer columns are ``int32``.

    Examples
    --------
    >>> import datetime as dt
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> points = peabiru.sample_points()
    >>> res = peabiru.trip_options(
    ...     network, points.iloc[[10]], points.iloc[[12]],
    ...     departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    ...     drop_geometry=True, n_threads=2,
    ... )
    >>> list(res.options.columns[:4])
    ['from_id', 'to_id', 'option', 'total_time']
    """
    assert_network(r5r_network)
    o = assign_points_input(origins, "origins", unique_ids=bool(all_to_all))
    d = assign_points_input(destinations, "destinations", unique_ids=bool(all_to_all))
    o, d = _expand_od_pairs(o, d, all_to_all)
    if output_dir is not None:
        pairs = pd.DataFrame({"o": list(o.ids), "d": list(d.ids)})
        if pairs.duplicated().any():
            raise ValueError(
                "With 'output_dir', origin-destination pairs must be unique, because each "
                "pair is written to a file named 'from_<id>_to_<id>.csv'."
            )
        # the core's rule for ids in file names, as for every output_dir
        _peabiru.check_output_ids([str(i) for i in list(o.ids) + list(d.ids) if i is not None])
    mode_list = assign_mode(mode, mode_egress)
    departure = assign_departure(departure_datetime, mode_list)
    if mode_list.transit:
        check_transit_availability_on_date(r5r_network, departure)
    crit = _criteria(criteria, fare_structure)
    lim = _street_limits(
        max_walk_time, max_bike_time, max_car_time, walk_speed, bike_speed,
        max_trip_duration, mode_list,
    )
    assert_count(time_window, "time_window", positive=True)
    check_speed(walk_speed, "walk")
    check_speed(bike_speed, "bike")
    assert_count(max_rides, "max_rides", positive=True)
    check_max_lts(max_lts)
    assert_number(max_options, "max_options", lower=1)
    if math.isfinite(float(max_options)):
        assert_count(max_options, "max_options", positive=True)
    assert_count(walk_resolution, "walk_resolution", positive=True)
    for name, v in [("itineraries", itineraries), ("by_departure", by_departure),
                    ("drop_geometry", drop_geometry), ("osm_link_ids", osm_link_ids)]:
        assert_flag(v, name)
    if drop_geometry and osm_link_ids:
        raise ValueError(
            "To set osm_link_ids to `True`, the parameter 'drop_geometry' must also be `False`"
        )
    threads = assign_n_threads(n_threads)
    check_messages(verbose, progress)
    fares = fare_arg(fare_structure, max_fare, r5r_network)
    out_dir = check_output_dir(output_dir)
    scenario = scenario_spec(new_carspeeds, carspeed_scale, new_lts, r5r_network)
    lines = itineraries and not drop_geometry and out_dir is None
    if lines:
        try:
            import geopandas as gpd
            import shapely
        except ImportError as e:  # pragma: no cover - depends on the environment
            raise ImportError(
                "trip_options() with the geometry of the legs needs geopandas (and shapely): "
                "pip install 'peabiru[geo]', or use drop_geometry=True"
            ) from e
    if out_dir is not None and (any(i is None for i in o.ids) or any(i is None for i in d.ids)):
        raise ValueError("With 'output_dir', the ids of 'origins' and 'destinations' cannot be missing.")

    with core_errors("trip_options"):
        r = _peabiru.trip_options_py(
            r5r_network._core, o.lat_bytes(), o.lon_bytes(), d.lat_bytes(), d.lon_bytes(),
            mode_list.direct_modes, mode_list.transit_mode, mode_list.egress_mode,
            departure.yyyymmdd, departure.seconds, int(time_window), crit,
            lim["walk"], lim["bike"], lim["car"], lim["trip"],
            float(walk_speed), float(bike_speed), int(max_rides), int(max_lts),
            threads, scenario, fares, float(max_fare),
            int(max_options) if math.isfinite(float(max_options)) else 0,
            bool(itineraries), bool(drop_geometry), bool(osm_link_ids),
            None if out_dir is None else (out_dir, [str(i) for i in o.ids], [str(i) for i in d.ids]),
            int(walk_resolution),
        )
    if r is None:
        return output_dir

    def keyed(t: dict) -> dict:
        pair = _i32(t["pair"])
        return {"from_id": _strings(o.ids[pair]), "to_id": _strings(d.ids[pair]),
                "option": _i32(t["option"]).copy()}

    def text(v: list) -> Any:
        return _strings(np.array(v, dtype=object))

    t = r["options"]
    options = pd.DataFrame({
        **keyed(t),
        "total_time": _f64(t["total_time"]).copy(),
        **({"fare": _f64(t["fare"]).copy()} if fares is not None else {}),
        "transfers": _i32(t["transfers"]).copy(),
        "walk_time": _f64(t["walk_time"]).copy(),
        "departure_time": text(t["departure_time"]),
        "arrival_time": text(t["arrival_time"]),
        "n_departures": _i32(t["n_departures"]).copy(),
        "first_departure": text(t["first_departure"]),
        "last_departure": text(t["last_departure"]),
        "description": text(t["description"]),
    })
    legs = None
    if itineraries:
        t = r["legs"]
        legs = pd.DataFrame({
            **keyed(t),
            "segment": _i32(t["segment"]).copy(),
            "mode": text(t["mode"]),
            **({"cumulative_fare": _f64(t["cumulative_fare"]).copy()} if fares is not None else {}),
            "segment_duration": _f64(t["segment_duration"]).copy(),
            "wait": _f64(t["wait"]).copy(),
            "distance": _i32(t["distance"]).copy(),
            "route": text(t["route"]),
        })
        if osm_link_ids:
            for c in ["osm_id_list", "edge_id_list", "board_stop_id", "alight_stop_id"]:
                legs[c] = text(t[c])
        if lines:
            n_points = _i32(t["n_points"])
            xy = np.column_stack([_f64(t["x"]), _f64(t["y"])])
            offsets = np.concatenate([[0], np.cumsum(n_points)])
            geoms = [shapely.LineString(xy[offsets[k]:offsets[k + 1]]) for k in range(len(n_points))]
            legs = gpd.GeoDataFrame(legs, geometry=gpd.GeoSeries(geoms, crs="EPSG:4326"))
    departures = None
    if by_departure:
        t = r["departures"]
        departures = pd.DataFrame({
            **keyed(t),
            "departure_time": text(t["departure_time"]),
            "arrival_time": text(t["arrival_time"]),
            "total_time": _f64(t["total_time"]).copy(),
        })
    return TripOptions(options, legs, departures)
