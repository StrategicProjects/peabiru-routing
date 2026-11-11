"""detailed_itineraries(), as r5r's (2.5.0)."""

from __future__ import annotations

import math
import os
import re
import warnings
from typing import Any, Optional, Union

import numpy as np
import pandas as pd

from . import _peabiru
from ._scenario import scenario_spec
from ._assign import (
    Points,
    assign_departure,
    assign_mode,
    assign_n_threads,
    assign_points_input,
    check_draws,
    check_max_lts,
    check_messages,
    check_output_dir,
    check_speed,
    check_transit_availability_on_date,
)
from ._checks import assert_count, assert_flag, core_errors, unsupported
from ._fares import fare_arg
from ._network import Network, assert_network
from ._routing import _f64, _i32, _street_limits, _strings

Number = Union[int, float]
PathArg = Union[str, "os.PathLike[str]"]


def _expand_od_pairs(o: Points, d: Points, all_to_all: Any) -> tuple:
    """r5r expand_od_pairs()."""
    assert_flag(all_to_all, "all_to_all")
    no, nd = len(o), len(d)
    if all_to_all or no == 1 or nd == 1:
        oi = np.repeat(np.arange(no), nd)
        di = np.tile(np.arange(nd), no)
        if not all_to_all and (no > 1 or nd > 1):
            warnings.warn(
                "'origins' was expanded to match the number of destinations."
                if no == 1
                else "'destinations' was expanded to match the number of origins.",
                stacklevel=3,
            )
    elif no != nd:
        raise ValueError(
            "When 'all_to_all' is False, 'origins' and 'destinations' must either "
            "have the same number of rows or one of them must have only one row."
        )
    else:
        oi = di = np.arange(no)
    pick = lambda p, i: Points(p.ids[i], p.lat[i], p.lon[i], p.df.iloc[i])  # noqa: E731
    return pick(o, oi), pick(d, di)


def detailed_itineraries(
    r5r_network: Network,
    origins: pd.DataFrame,
    destinations: pd.DataFrame,
    mode: Any = "WALK",
    mode_egress: str = "WALK",
    departure_datetime: Optional[Any] = None,
    time_window: Number = 10,
    suboptimal_minutes: Number = 0,
    max_walk_time: Number = math.inf,
    max_bike_time: Number = math.inf,
    max_car_time: Number = math.inf,
    max_trip_duration: Number = 120,
    walk_speed: float = 3.6,
    bike_speed: float = 12,
    max_rides: Number = 3,
    max_lts: Number = 2,
    shortest_path: bool = True,
    all_to_all: bool = False,
    fare_structure: Any = None,
    max_fare: Number = math.inf,
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
    """Detailed itineraries between origin-destination pairs.

    The itinerary of each trip between pairs of origins and destinations:
    its legs (walking, cycling, driving and each ride), with their
    duration, waiting time, distance, route and line. Arguments, defaults
    and the result are those of ``r5r::detailed_itineraries()`` (r5r
    2.5.0), with the same values: the direct trip by the street mode and,
    with public transport, the fastest trip over the departure window
    (``shortest_path=True``) or, with ``shortest_path=False``, the
    alternatives R5's multi-criteria router (McRAPTOR) finds, one per
    sequence of routes; with a ``fare_structure``, the trips are routed
    with fares and get ``total_fare`` and ``cumulative_fare`` columns.

    Not supported yet, and an [`UnsupportedError`][peabiru.UnsupportedError]:
    ``osm_link_ids=True`` and ``output_dir`` with the geometry (use
    ``drop_geometry=True``). As in r5r, networks
    whose GTFS has frequency-based trips cannot be used with public
    transport.

    Parameters
    ----------
    r5r_network : Network
        A network built with [`build_network()`][peabiru.build_network].
    origins, destinations : pandas.DataFrame
        Points with columns ``id``, ``lon`` and ``lat`` (WGS 84), or
        ``geopandas.GeoDataFrame`` of points in EPSG:4326 with an ``id``
        column. Paired row by row, unless ``all_to_all``; a single point is
        repeated to match the other.
    mode, mode_egress, departure_datetime, time_window, max_walk_time, max_bike_time, max_car_time, max_trip_duration, walk_speed, bike_speed, max_rides, max_lts, n_threads, verbose, progress
        As in [`travel_time_matrix()`][peabiru.travel_time_matrix].
    suboptimal_minutes : int, default 0
        With ``shortest_path=False``, how many minutes slower than the best
        an alternative may be; must be 0 with ``shortest_path=True`` or a
        ``fare_structure``.
    shortest_path : bool, default True
        Whether to return only the fastest itinerary.
    all_to_all : bool, default False
        Whether to route from every origin to every destination.
    fare_structure : dict, optional
        A fare structure (see [`setup_fare_structure()`][peabiru.setup_fare_structure]).
    max_fare : float, default math.inf
        The highest fare of a trip; a finite value needs a ``fare_structure``.
    new_carspeeds, carspeed_scale, new_lts
        Not supported yet (the defaults only).
    drop_geometry : bool, default False
        Whether to return a ``pandas.DataFrame`` without the lines.
    osm_link_ids : bool, default False
        ``True`` is not supported yet.
    output_dir : str or os.PathLike, optional
        With ``drop_geometry=True``: write one CSV file per pair with
        itineraries, ``from_<origin id>_to_<destination id>.csv``, as r5r
        does, and return ``output_dir``.

    Returns
    -------
    geopandas.GeoDataFrame or pandas.DataFrame or str or os.PathLike
        One row per leg (``segment``) of each itinerary (``option``):
        ``from_id``, ``from_lat``, ``from_lon``, ``to_id``, ``to_lat``,
        ``to_lon``, ``option``, ``departure_time``, ``total_duration``
        (minutes), ``total_distance`` (metres), ``segment``, ``mode``,
        ``segment_duration``, ``wait`` (minutes), ``distance`` (metres),
        ``route`` and, unless ``drop_geometry``, ``geometry`` (shapely
        ``LineString`` in EPSG:4326). Integer columns are ``int32``.

    Examples
    --------
    >>> import datetime as dt
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> points = peabiru.sample_points()
    >>> trips = peabiru.detailed_itineraries(
    ...     network, points.iloc[:3], points.iloc[3:6], mode="WALK",
    ...     departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    ...     drop_geometry=True, n_threads=2,
    ... )
    >>> list(trips.columns[:4])
    ['from_id', 'from_lat', 'from_lon', 'to_id']
    """
    # mirrors r5r R/detailed_itineraries.R
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
        bad = sorted({i for i in list(o.ids) + list(d.ids) if re.search(r'[/\\:*?"<>|]', str(i))})
        if bad:
            raise ValueError(
                "With 'output_dir', ids cannot contain characters that are not allowed in "
                'file names (/ \\ : * ? " < > |). Invalid id(s): '
                + ", ".join(f'"{i}"' for i in bad) + "."
            )
    mode_list = assign_mode(mode, mode_egress)
    departure = assign_departure(departure_datetime, mode_list)
    if mode_list.transit:
        check_transit_availability_on_date(r5r_network, departure)
        if r5r_network.info()["frequencies"]:
            raise ValueError(
                "Assertion on 'r5r_network' failed: None of the GTFS feeds used to create "
                "the transit network can contain a 'frequencies' table. Try using "
                "gtfstools::frequencies_to_stop_times() to create a suitable feed."
            )
    lim = _street_limits(
        max_walk_time, max_bike_time, max_car_time, walk_speed, bike_speed,
        max_trip_duration, mode_list,
    )
    assert_flag(shortest_path, "shortest_path")
    assert_flag(drop_geometry, "drop_geometry")
    assert_flag(osm_link_ids, "osm_link_ids")
    if drop_geometry and osm_link_ids:
        raise ValueError(
            "To set osm_link_ids to `True`, the parameter 'drop_geometry' must also be `False`"
        )
    assert_count(time_window, "time_window", positive=True)
    check_draws(1, time_window,
                mcraptor=mode_list.transit and not (shortest_path and fare_structure is None))
    check_speed(walk_speed, "walk")
    check_speed(bike_speed, "bike")
    assert_count(max_rides, "max_rides", positive=True)
    check_max_lts(max_lts)
    threads = assign_n_threads(n_threads)
    check_messages(verbose, progress)
    fares = fare_arg(fare_structure, max_fare, r5r_network)
    out_dir = check_output_dir(output_dir)
    assert_count(suboptimal_minutes, "suboptimal_minutes")
    if fare_structure is not None and suboptimal_minutes > 0:
        raise ValueError(
            "Assertion on 'suboptimal_minutes' failed: Must be 0 when calculating fares "
            "with detailed_itineraries()."
        )
    if shortest_path and suboptimal_minutes > 0:
        raise ValueError(
            "Assertion on 'suboptimal_minutes' failed: Must be 0 when 'shortest_path' is True."
        )
    scenario = scenario_spec(new_carspeeds, carspeed_scale, new_lts)
    if osm_link_ids:
        unsupported("'osm_link_ids=True'")
    if out_dir is not None and not drop_geometry:
        unsupported("'output_dir' with the geometry (JTS's WKT text)", "; use 'drop_geometry=True'")
    if not drop_geometry:
        try:
            import geopandas as gpd
            import shapely
        except ImportError as e:  # pragma: no cover - depends on the environment
            raise ImportError(
                "detailed_itineraries() with geometry needs geopandas (and shapely): "
                "pip install 'peabiru[geo]', or use drop_geometry=True"
            ) from e
    if out_dir is not None and (any(i is None for i in o.ids) or any(i is None for i in d.ids)):
        raise ValueError("With 'output_dir', the ids of 'origins' and 'destinations' cannot be missing.")

    with core_errors("detailed_itineraries"):
        r = _peabiru.itineraries(
            r5r_network._core, o.lat_bytes(), o.lon_bytes(), d.lat_bytes(), d.lon_bytes(),
            mode_list.direct_modes, mode_list.transit_mode, mode_list.egress_mode,
            departure.yyyymmdd, departure.seconds, int(time_window),
            lim["walk"], lim["bike"], lim["car"], lim["trip"],
            float(walk_speed), float(bike_speed), int(max_rides), int(max_lts),
            bool(shortest_path), threads, scenario,
            None if out_dir is None else (out_dir, [str(i) for i in o.ids], [str(i) for i in d.ids]),
            int(suboptimal_minutes), fares, float(max_fare),
        )
    if r is None:
        return output_dir
    pair = _i32(r["pair"])
    df = pd.DataFrame({
        "from_id": _strings(o.ids[pair]),
        "from_lat": o.lat[pair],
        "from_lon": o.lon[pair],
        "to_id": _strings(d.ids[pair]),
        "to_lat": d.lat[pair],
        "to_lon": d.lon[pair],
        "option": _i32(r["option"]).copy(),
        "departure_time": _strings(np.array(r["departure_time"], dtype=object)),
        "total_duration": _f64(r["total_duration"]).copy(),
        "total_distance": _i32(r["total_distance"]).copy(),
        **({"total_fare": _f64(r["total_fare"]).copy()} if fares is not None else {}),
        "segment": _i32(r["segment"]).copy(),
        "mode": _strings(np.array(r["mode"], dtype=object)),
        **({"cumulative_fare": _f64(r["cumulative_fare"]).copy()} if fares is not None else {}),
        "segment_duration": _f64(r["segment_duration"]).copy(),
        "wait": _f64(r["wait"]).copy(),
        "distance": _i32(r["distance"]).copy(),
        "route": _strings(np.array(r["route"], dtype=object)),
    })
    if drop_geometry:
        return df
    n_points = _i32(r["n_points"])
    xy = np.column_stack([_f64(r["x"]), _f64(r["y"])])
    offsets = np.concatenate([[0], np.cumsum(n_points)])
    lines = [shapely.LineString(xy[offsets[k]:offsets[k + 1]]) for k in range(len(n_points))]
    return gpd.GeoDataFrame(df, geometry=gpd.GeoSeries(lines, crs="EPSG:4326"))
