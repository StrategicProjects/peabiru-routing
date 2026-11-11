"""r5r 2.5.0's isochrone(): polygons from travel-time surfaces, or the street
segments reached."""

from __future__ import annotations

import math
import warnings
from typing import Any, List, Optional, Sequence, Union

import numpy as np
import pandas as pd

from . import _peabiru
from ._assign import (
    assign_departure,
    assign_mode,
    assign_percentiles,
    assign_points_input,
)
from ._checks import (
    as_number_list,
    assert_count,
    assert_flag,
    assert_int,
    assert_number,
    core_errors,
    fail_assert,
)
from ._network import Network, assert_network
from ._routing import (
    Mode,
    Number,
    _common_checks,
    _f64,
    _i32,
    _street_limits,
    _strings,
    travel_time_matrix,
)

_NA = _peabiru.NA_I32
# r5r's sample_size is deprecated: any value given warns
_UNSET: Any = object()


def _geo() -> Any:
    try:
        import geopandas
        import shapely
    except ImportError as e:  # pragma: no cover - depends on the environment
        raise ImportError(
            "isochrone() returns geometries and needs geopandas (and shapely): "
            "pip install 'peabiru[geo]'"
        ) from e
    return geopandas, shapely


def isochrone(
    r5r_network: Network,
    origins: pd.DataFrame,
    mode: Mode = "transit",
    mode_egress: str = "walk",
    cutoffs: Union[Number, Sequence[Number]] = (0, 15, 30),
    zoom: Number = 10,
    departure_datetime: Optional[Any] = None,
    polygon_output: bool = True,
    time_window: Number = 10,
    max_walk_time: Number = math.inf,
    max_bike_time: Number = math.inf,
    max_car_time: Number = math.inf,
    max_trip_duration: Number = 120,
    walk_speed: float = 3.6,
    bike_speed: float = 12,
    max_rides: Number = 3,
    max_lts: Number = 2,
    draws_per_minute: Number = 5,
    seed: Optional[Number] = None,
    percentiles: Optional[Union[int, Sequence[int]]] = None,
    n_threads: Optional[Number] = None,
    verbose: bool = False,
    progress: bool = True,
    sample_size: Any = _UNSET,
) -> Any:
    """Isochrones: the areas or street segments reached from each origin.

    The areas (``polygon_output=True``) or the street segments
    (``polygon_output=False``) reached from each origin within each cutoff,
    by one street mode (``"WALK"``, ``"BICYCLE"`` or ``"CAR"``) or by
    public transport with walk access and egress (``["WALK", "TRANSIT"]``).
    Arguments and defaults are those of ``r5r::isochrone()`` (r5r 2.5.0).

    Polygon isochrones are made as r5r makes them: travel times from each
    origin to the centres of a regular grid of Web Mercator pixels covering
    the network (see ``zoom``; these are R5's travel times exactly), then
    the band from 0 to each cutoff traced over the grid by marching squares
    and grouped into polygons with holes, as the R package isoband does for
    r5r (vertex for vertex). r5r then passes the polygons to
    ``sf::st_make_valid()``, which rebuilds them with the s2 library
    (snapping vertices to 1e-7 degrees, re-orienting rings, turning a single
    polygon into a ``POLYGON``); peabiru does not reproduce that step: the
    geometries are isoband's ``MultiPolygon`` (shells as traced, holes
    reversed), which cover the same area. Pass them to
    ``shapely.make_valid()`` if your tools need valid geometries (rings of
    one polygon may touch at a vertex).

    Line isochrones are the travel times from each origin to the centroid
    of every street segment (r5r routes to ``sf::st_centroid()`` of each
    edge of ``street_network_to_sf()``; peabiru's centroids agree to within
    about 1e-14 degrees), each segment in the band of the smallest cutoff at
    or above its travel time.

    As in r5r, ``max_trip_duration`` is ignored: the trip limit is the
    largest cutoff (plus 10 minutes for polygons, so that the outer band is
    interpolated like the inner ones).

    Not supported yet, and an [`UnsupportedError`][peabiru.UnsupportedError]
    when requested: transit with ``"BICYCLE"`` or ``"CAR"`` access or a
    non-walk ``mode_egress``; a subset of transit modes. Needs geopandas
    (``pip install 'peabiru[geo]'``).

    Parameters
    ----------
    r5r_network : Network
        A network built with [`build_network()`][peabiru.build_network].
    origins : pandas.DataFrame
        Points with columns ``id``, ``lon`` and ``lat`` (WGS 84), or a
        ``geopandas.GeoDataFrame`` of points in EPSG:4326 with an ``id``
        column, as in [`travel_time_matrix()`][peabiru.travel_time_matrix].
    mode : str or sequence of str, default "transit"
        One of ``"WALK"``, ``"BICYCLE"`` or ``"CAR"``, or ``"TRANSIT"``
        (alone or with ``"WALK"``); case does not matter.
    mode_egress : str, default "walk"
        The mode from the last stop to the destination. Only ``"WALK"`` is
        supported with transit.
    cutoffs : number or sequence of numbers, default (0, 15, 30)
        The travel times in minutes that delimit the isochrones: finite and
        at least 0. They are sorted and duplicates removed; 0 is added, and
        at least one must be greater than 0.
    zoom : int, default 10
        From 9 to 12: the Web Mercator zoom level of the travel-time grid
        polygons are interpolated from (cells of about 153 metres at the
        Equator at zoom 10). Only used for polygons.
    departure_datetime : datetime.datetime, optional
        The departure date and time, as in
        [`travel_time_matrix()`][peabiru.travel_time_matrix]. Required with
        transit.
    polygon_output : bool, default True
        ``True`` for polygons, ``False`` for the street segments reached.
    time_window : int, default 10
        The window of departure minutes over which travel times are
        computed.
    max_walk_time : int or float, default math.inf
        The maximum walking time in minutes (``math.inf`` means the trip
        limit).
    max_bike_time : int or float, default math.inf
        The maximum cycling time in minutes.
    max_car_time : int or float, default math.inf
        The maximum driving time in minutes.
    max_trip_duration : int, default 120
        Ignored, as in r5r: the trip limit comes from the cutoffs.
    walk_speed : float, default 3.6
        Walking speed in km/h.
    bike_speed : float, default 12
        Cycling speed in km/h.
    max_rides : int, default 3
        The maximum number of public transport rides.
    max_lts : int, default 2
        The highest level of traffic stress (1 to 4) a cyclist will ride on.
    draws_per_minute : int, default 5
        The Monte Carlo draws per departure minute for frequency-based GTFS.
    seed : int, optional
        A seed for the Monte Carlo draws; with frequency-based GTFS the
        results are reproducible (and equal to r5r's) only with a seed.
    percentiles : int or sequence of int, optional
        Up to 5 distinct values from 1 to 99: the travel-time percentiles to
        make polygons for. ``None`` (the default) means 50. Must be ``None``
        for line isochrones.
    n_threads : int or math.inf, optional
        The number of threads; ``None`` (the default) or ``math.inf`` use
        all cores.
    verbose : bool, default False
        Checked as in r5r; has no effect.
    progress : bool, default True
        Checked as in r5r; peabiru prints no progress counter.
    sample_size : optional
        Deprecated in r5r and ignored: giving it only warns.

    Returns
    -------
    geopandas.GeoDataFrame
        In EPSG:4326. With ``polygon_output=True``: ``id``, ``isochrone``
        (the cutoff, float), ``percentile`` (such as ``"p50"``) and the
        geometry column ``polygons`` (a ``MultiPolygon``, empty when nothing
        is reached), one row per origin, percentile (ascending) and cutoff
        (descending). With ``polygon_output=False``: ``id``, ``edge_index``
        (a string, as in r5r), ``osm_id`` (int64), ``isochrone``,
        ``travel_time_p50`` (``Int32``), the segment's ``from_vertex``,
        ``to_vertex``, ``street_class``, ``length`` (metres), ``walk``,
        ``car``, ``car_speed`` (km/h), ``bicycle``, ``bicycle_lts`` and the
        geometry column ``geometry`` (a ``LineString``); rows by origin,
        then by band and travel time, both descending.

    Examples
    --------
    >>> import datetime as dt
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> origin = peabiru.sample_points().iloc[:1]
    >>> iso = peabiru.isochrone(
    ...     network, origin, mode="WALK", cutoffs=[0, 10, 20],
    ...     departure_datetime=dt.datetime(2019, 5, 13, 14, 0), n_threads=2,
    ... )
    >>> list(iso.columns), list(iso.isochrone)
    (['id', 'isochrone', 'percentile', 'polygons'], [20.0, 10.0])
    """
    # mirrors r5r R/isochrone.R
    if sample_size is not _UNSET:
        warnings.warn(
            "The `sample_size` argument is no longer used and has no effect.",
            stacklevel=2,
        )
    assert_network(r5r_network)
    gpd, shapely = _geo()

    # checkmate::assert_numeric(cutoffs, lower = 0, finite = TRUE,
    #                           any.missing = FALSE, min.len = 1)
    values = as_number_list(cutoffs, "cutoffs", "numeric")
    if len(values) < 1:
        fail_assert("cutoffs", "Must have length >= 1")
    if any(math.isnan(v) for v in values):
        fail_assert("cutoffs", "Contains missing values")
    if any(math.isinf(v) for v in values):
        fail_assert("cutoffs", "Must be finite")
    if any(v < 0 for v in values):
        fail_assert("cutoffs", "All elements must be >= 0")
    assert_flag(polygon_output, "polygon_output")

    # the largest cutoff is the trip limit; the user's max_trip_duration is
    # ignored
    limit = int(max(values))
    cuts = sorted(set([0.0] + values))
    if len(cuts) < 2:
        raise ValueError("`cutoffs` must contain at least one value greater than 0.")

    if polygon_output:
        if percentiles is None:
            percentiles = 50
        assert_number(zoom, "zoom", lower=9, upper=12)
        assert_int(zoom, "zoom")
        return _polygons(
            gpd, shapely, r5r_network, origins, mode, mode_egress, cuts, int(round(zoom)),
            departure_datetime, time_window, max_walk_time, max_bike_time, max_car_time,
            # route past max(cutoffs) so the outer band is interpolated like
            # the inner ones
            limit + 10,
            walk_speed, bike_speed, max_rides, max_lts, draws_per_minute, seed,
            percentiles, n_threads, verbose, progress,
        )
    if percentiles is not None:
        raise ValueError("Percentiles not supported for line-based isochrones")
    return _lines(
        gpd, shapely, r5r_network, origins, mode, mode_egress, cuts, departure_datetime,
        time_window, max_walk_time, max_bike_time, max_car_time, limit, walk_speed,
        bike_speed, max_rides, max_lts, draws_per_minute, seed, n_threads, verbose,
        progress,
    )


def _polygons(
    gpd: Any, shapely: Any, network: Network, origins: Any, mode: Any, mode_egress: Any,
    cutoffs: List[float], zoom: int, departure_datetime: Any, time_window: Any,
    max_walk_time: Any, max_bike_time: Any, max_car_time: Any, max_trip_duration: int,
    walk_speed: Any, bike_speed: Any, max_rides: Any, max_lts: Any,
    draws_per_minute: Any, seed: Any, percentiles: Any, n_threads: Any, verbose: Any,
    progress: Any,
) -> Any:
    """r5r's travel_time_surface() argument handling, then the core's
    surfaces and polygons."""
    o = assign_points_input(origins, "origins")
    mode_list = assign_mode(mode, mode_egress)
    departure = assign_departure(departure_datetime, mode_list)
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
    with core_errors("isochrone"):
        r = _peabiru.isochrone_polygons(
            network._core, o.lat_bytes(), o.lon_bytes(),
            "TRANSIT" if mode_list.transit else mode_list.direct_modes,
            cutoffs, zoom, pcts, departure.yyyymmdd, departure.seconds,
            int(time_window), lim["walk"], lim["trip"], float(walk_speed),
            float(bike_speed), int(max_lts), int(max_rides), int(draws_per_minute),
            c["seed"], c["threads"],
        )
    origin = _i32(r["origin"])
    geoms = _multipolygons(shapely, r)
    data = {
        "id": _strings(o.ids[origin]),
        "isochrone": _f64(r["isochrone"]).copy(),
        "percentile": _strings(np.array([f"p{p}" for p in _i32(r["percentile"])], dtype=object)),
    }
    return gpd.GeoDataFrame(
        pd.DataFrame(data), geometry=gpd.GeoSeries(geoms, crs="EPSG:4326", name="polygons")
    ).rename_geometry("polygons")


def _multipolygons(shapely: Any, r: Any) -> List[Any]:
    """isoband's MultiPolygons from the flat buffers."""
    n_polygons = _i32(r["n_polygons"])
    n_rings = _i32(r["n_rings"])
    n_points = _i32(r["n_points"])
    xy = np.column_stack([_f64(r["x"]), _f64(r["y"])])
    ring_end = np.cumsum(n_points)
    rings = [xy[e - n:e] for e, n in zip(ring_end, n_points)]
    out = []
    k = 0  # next ring
    p = 0  # next polygon
    for npoly in n_polygons:
        polys = []
        for nr in n_rings[p:p + npoly]:
            polys.append(shapely.Polygon(rings[k], rings[k + 1:k + nr]))
            k += nr
        p += npoly
        out.append(shapely.MultiPolygon(polys))
    return out


def _lines(
    gpd: Any, shapely: Any, network: Network, origins: Any, mode: Any, mode_egress: Any,
    cutoffs: List[float], departure_datetime: Any, time_window: Any, max_walk_time: Any,
    max_bike_time: Any, max_car_time: Any, max_trip_duration: int, walk_speed: Any,
    bike_speed: Any, max_rides: Any, max_lts: Any, draws_per_minute: Any, seed: Any,
    n_threads: Any, verbose: Any, progress: Any,
) -> Any:
    """r5r's line isochrones: travel_time_matrix() to the centroids of the
    street edges, banded by the core."""
    e = _peabiru.street_edges(network._core)
    edge_index = _i32(e["edge_index"])
    dest = pd.DataFrame({
        "id": _strings(np.array([str(i) for i in edge_index], dtype=object)),
        "lon": _f64(e["lon"]),
        "lat": _f64(e["lat"]),
    })
    ttm = travel_time_matrix(
        network, origins, dest, mode=mode, mode_egress=mode_egress,
        departure_datetime=departure_datetime, time_window=time_window,
        max_walk_time=max_walk_time, max_bike_time=max_bike_time,
        max_car_time=max_car_time, max_trip_duration=max_trip_duration,
        walk_speed=walk_speed, bike_speed=bike_speed, max_rides=max_rides,
        max_lts=max_lts, draws_per_minute=draws_per_minute, seed=seed,
        n_threads=n_threads, verbose=verbose, progress=progress,
    )
    with warnings.catch_warnings():
        # travel_time_matrix() has already warned about these origins
        warnings.simplefilter("ignore")
        ids = assign_points_input(origins, "origins").ids
    n_o, n_e = len(ids), len(edge_index)
    o = pd.Index(ids).get_indexer(ttm["from_id"])
    d = pd.Index(dest["id"]).get_indexer(ttm["to_id"])
    minutes = np.full(n_o * n_e, _NA, dtype=np.int32)
    t = ttm["travel_time_p50"].to_numpy(dtype=np.int32, na_value=_NA)
    minutes[o * n_e + d] = t
    with core_errors("isochrone"):
        ob, eb, tb, ib = _peabiru.isochrone_lines(n_o, n_e, minutes.tobytes(), cutoffs)
    k = _i32(eb)
    n_points = _i32(e["n_points"])
    ends = np.cumsum(n_points)
    starts = ends - n_points
    xy = np.column_stack([_f64(e["x"]), _f64(e["y"])])
    geoms = [shapely.LineString(xy[starts[i]:ends[i]]) for i in k]
    classes = np.array(e["street_class"], dtype=object)
    data = {
        "id": _strings(ids[_i32(ob)]),
        "edge_index": _strings(np.array([str(i) for i in edge_index[k]], dtype=object)),
        "osm_id": _f64(e["osm_id"])[k].astype(np.int64),
        "isochrone": _f64(ib).copy(),
        "travel_time_p50": pd.array(_i32(tb), dtype="Int32"),
        "from_vertex": _i32(e["from_vertex"])[k],
        "to_vertex": _i32(e["to_vertex"])[k],
        "street_class": _strings(classes[k]),
        "length": _f64(e["length"])[k],
        "walk": _i32(e["walk"])[k].astype(bool),
        "car": _i32(e["car"])[k].astype(bool),
        "car_speed": _f64(e["car_speed"])[k],
        "bicycle": _i32(e["bicycle"])[k].astype(bool),
        "bicycle_lts": _i32(e["bicycle_lts"])[k],
    }
    return gpd.GeoDataFrame(
        pd.DataFrame(data), geometry=gpd.GeoSeries(geoms, crs="EPSG:4326")
    )
