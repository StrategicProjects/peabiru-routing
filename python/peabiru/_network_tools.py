"""r5r 2.5.0's network functions: find_snap(), street_network_bbox(),
street_network_to_sf(), transit_network_to_sf(), check_transit_availability()
and get_gtfs_errors()."""

from __future__ import annotations

import datetime as _dt
import os
import re
from typing import Any, Dict, List, Optional, Sequence, Union

import numpy as np
import pandas as pd

from . import _checkmate as cm
from . import _peabiru
from ._assign import assign_points_input
from ._checks import assert_string, core_errors, fail_assert, is_number, scalar
from ._network import Network, PathLike, assert_network
from ._routing import _f64, _i32, _strings

DateLike = Union[str, _dt.date]


def _geo() -> Any:
    try:
        import geopandas
        import shapely
    except ImportError as e:  # pragma: no cover - depends on the environment
        raise ImportError(
            "this function returns geometries and needs geopandas (and shapely): "
            "pip install 'peabiru[geo]'"
        ) from e
    return geopandas, shapely


def _bools(b: bytes) -> np.ndarray:
    return np.frombuffer(b, dtype=np.uint8).astype(bool)


def find_snap(
    r5r_network: Network,
    points: pd.DataFrame,
    radius: float = 1600,
    mode: str = "WALK",
) -> pd.DataFrame:
    """Find where points snap to the street network.

    Snapping is how routing places origins and destinations on the
    network. The nearest point of a street the mode may
    use is looked for first within 300 metres of the point and then, if
    there is none, within ``radius``. A point that does not snap is not
    routed. The street a point snaps to depends on the mode, since some
    streets cannot be used by some modes (a pedestrian street cannot be used
    to snap car trips).

    Parameters
    ----------
    r5r_network : Network
        A network built with [`build_network()`][peabiru.build_network].
    points : pandas.DataFrame or geopandas.GeoDataFrame
        Points with ``id``, ``lon`` and ``lat`` columns, or a GeoDataFrame
        of points in EPSG:4326 with an ``id`` column.
    radius : float, default 1600
        The largest distance in metres within which to snap.
    mode : {"WALK", "BICYCLE", "CAR"}, default "WALK"
        The mode whose streets the points snap to.

    Returns
    -------
    pandas.DataFrame
        ``point_id``, ``lat`` and ``lon`` (the points), ``snap_lat`` and
        ``snap_lon`` (where they snap), ``distance`` (metres, haversine on
        R5's sphere of radius 6,367,450 m) and ``found``. Points that do not
        snap have NaN coordinates and distance and ``found`` False.

    Notes
    -----
    **Compatibility with r5r.** The arguments, defaults, checks, columns
    and values are those of ``find_snap()`` in r5r 2.5.0 (R5 7.6's
    snapping, ported from Conveyal's R5). Differences: no ``r5r_core``
    argument; a pandas DataFrame instead of a ``data.table``.

    Examples
    --------
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> snaps = peabiru.find_snap(network, peabiru.sample_points(), radius=2000)
    >>> bool(snaps["found"].all())
    True
    """
    assert_network(r5r_network)
    cm.cm_assert(cm.check_numeric(radius, lower=0, finite=True, max_len=1), "radius")
    modes = ["WALK", "BICYCLE", "CAR"]
    cm.cm_assert_and("mode", cm.check_string(mode), lambda: cm.check_names(mode, subset_of=modes))
    mode = str(scalar(mode))
    p_ = cm.Vec(radius)
    if len(p_) == 0:
        # r5r hands Java a zero-length radius; it fails there
        raise ValueError("'radius' must be a single number.")
    if p_.type == "missing":
        # checkmate lets NA through and R5 then snaps nothing
        radius = 0.0
    radius = float(scalar(radius))
    p = assign_points_input(points, "points")
    with core_errors("find_snap"):
        r = _peabiru.find_snap(r5r_network._core, p.lat_bytes(), p.lon_bytes(), radius, mode)
    return pd.DataFrame({
        "point_id": _strings(p.ids),
        "lat": p.lat,
        "lon": p.lon,
        "snap_lat": _f64(r["snap_lat"]).copy(),
        "snap_lon": _f64(r["snap_lon"]).copy(),
        "distance": _f64(r["distance"]).copy(),
        "found": _bools(r["found"]),
    })


def street_network_bbox(r5r_network: Network, output: str = "polygon") -> Any:
    """The bounding box of the street network.

    R5's envelope of the street layer, which covers every OpenStreetMap node of the ways streets were made from
    (it can be a little larger than the box of the edges of
    [`street_network_to_sf()`][peabiru.street_network_to_sf]).

    Parameters
    ----------
    r5r_network : Network
        A network built with [`build_network()`][peabiru.build_network].
    output : {"polygon", "bbox", "vector"}, default "polygon"
        The form of the result.

    Returns
    -------
    geopandas.GeoSeries, tuple or dict
        ``"polygon"``: a GeoSeries with the box as one polygon, in
        EPSG:4326 (needs geopandas). ``"bbox"``: the tuple ``(xmin, ymin,
        xmax, ymax)``, as shapely's ``bounds``. ``"vector"``: a dict with
        the keys ``xmin``, ``ymin``, ``xmax`` and ``ymax``.

    Notes
    -----
    **Compatibility with r5r.** The arguments, the choices of ``output``
    and the coordinates are those of ``street_network_bbox()`` in r5r
    2.5.0. Differences: no ``r5r_core`` argument; the results are Python
    objects (a GeoSeries for r5r's ``sfc``, a tuple for its ``bbox``, a
    dict for its named vector).

    Examples
    --------
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> sorted(peabiru.street_network_bbox(network, "vector"))
    ['xmax', 'xmin', 'ymax', 'ymin']
    """
    assert_network(r5r_network)
    choices = ["polygon", "bbox", "vector"]
    if output not in choices:
        raise ValueError(
            "'arg' should be one of " + ", ".join(f'"{c}"' for c in choices)
        )
    xmin, ymin, xmax, ymax = _peabiru.street_bbox(r5r_network._core)
    if output == "vector":
        return {"xmin": xmin, "ymin": ymin, "xmax": xmax, "ymax": ymax}
    if output == "bbox":
        return (xmin, ymin, xmax, ymax)
    gpd, shapely = _geo()
    return gpd.GeoSeries([shapely.box(xmin, ymin, xmax, ymax)], crs="EPSG:4326")


def _lines(shapely: Any, n_points: np.ndarray, x: np.ndarray, y: np.ndarray) -> List[Any]:
    ends = np.cumsum(n_points)
    starts = ends - n_points
    xy = np.column_stack([x, y])
    # GEOS (shapely) has no line of one point, which r5r's sf holds as a
    # one-point LINESTRING: the point keeps its position
    return [
        shapely.LineString(xy[s:e])
        if e - s > 1
        else (shapely.Point(xy[s]) if e > s else shapely.LineString())
        for s, e in zip(starts, ends)
    ]


def street_network_to_sf(r5r_network: Network) -> Dict[str, Any]:
    """The street network as GeoDataFrames.

    The vertices at the ends of street edges and every directed street edge
    (the connectors of transit stops and park-and-rides left out), in the
    order of R5's street layer.

    Parameters
    ----------
    r5r_network : Network
        A network built with [`build_network()`][peabiru.build_network].

    Returns
    -------
    dict
        ``"vertices"``: a GeoDataFrame of points with ``index``,
        ``park_and_ride``, ``bike_sharing`` and ``geometry``. ``"edges"``: a
        GeoDataFrame of lines with ``edge_index``, ``osm_id``,
        ``from_vertex``, ``to_vertex``, ``street_class``, ``length``
        (metres), ``walk``, ``car``, ``car_speed`` (km/h), ``bicycle``,
        ``bicycle_lts`` and ``geometry``. Both in EPSG:4326; needs
        geopandas.

    Notes
    -----
    **Compatibility with r5r.** The rows, their order, the columns and the
    geometries are those of ``street_network_to_sf()`` in r5r 2.5.0.
    Differences: no ``r5r_core`` argument; GeoDataFrames instead of ``sf``
    data frames, with ``osm_id`` as ``int64`` (r5r's is a double, R having
    no 64-bit integers).

    Examples
    --------
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> streets = peabiru.street_network_to_sf(network)
    >>> list(streets["edges"].columns[:4])
    ['edge_index', 'osm_id', 'from_vertex', 'to_vertex']
    """
    assert_network(r5r_network)
    gpd, shapely = _geo()
    v = _peabiru.street_vertices(r5r_network._core)
    vertices = gpd.GeoDataFrame(
        pd.DataFrame({
            "index": _i32(v["index"]).copy(),
            "park_and_ride": _bools(v["park_and_ride"]),
            "bike_sharing": _bools(v["bike_sharing"]),
        }),
        geometry=gpd.GeoSeries(shapely.points(_f64(v["lon"]), _f64(v["lat"])), crs="EPSG:4326"),
    )
    e = _peabiru.street_edges(r5r_network._core)
    edges = gpd.GeoDataFrame(
        pd.DataFrame({
            "edge_index": _i32(e["edge_index"]).copy(),
            "osm_id": _f64(e["osm_id"]).astype(np.int64),
            "from_vertex": _i32(e["from_vertex"]).copy(),
            "to_vertex": _i32(e["to_vertex"]).copy(),
            "street_class": _strings(np.array(e["street_class"], dtype=object)),
            "length": _f64(e["length"]).copy(),
            "walk": _i32(e["walk"]).astype(bool),
            "car": _i32(e["car"]).astype(bool),
            "car_speed": _f64(e["car_speed"]).copy(),
            "bicycle": _i32(e["bicycle"]).astype(bool),
            "bicycle_lts": _i32(e["bicycle_lts"]).copy(),
        }),
        geometry=gpd.GeoSeries(
            _lines(shapely, _i32(e["n_points"]), _f64(e["x"]), _f64(e["y"])), crs="EPSG:4326"
        ),
    )
    return {"vertices": vertices, "edges": edges}


def transit_network_to_sf(r5r_network: Network) -> Dict[str, Any]:
    """The public transport network as GeoDataFrames.

    The stops and the routes of the public transport layer. There is one
    route row per trip pattern (the trips of a route with the same stops),
    so the same route can appear more than once; its line is the GTFS shape
    of the first trip of the pattern or, without one, the positions of its
    stops. A stop not linked to the street network (outside the area of the
    ``.pbf``, for instance) has an empty point, and a pattern with no line
    is left out; invalid lines are repaired with ``shapely.make_valid()``,
    and a line of a single point (a pattern with one linked stop) is a
    ``Point``. Positions of stops are those of their vertices in the street
    network (the coordinates rounded down to 1e-7 degrees).

    Parameters
    ----------
    r5r_network : Network
        A network built with [`build_network()`][peabiru.build_network].

    Returns
    -------
    dict
        ``"stops"``: a GeoDataFrame of points with ``stop_index``,
        ``stop_id`` (prefixed by the feed id), ``stop_name`` (None when
        blank), ``linked_to_street`` and ``geometry``. ``"routes"``: a
        GeoDataFrame of lines with ``agency_id``, ``agency_name``,
        ``route_id``, ``long_name``, ``short_name``, ``mode`` and
        ``geometry``. Both in EPSG:4326; needs geopandas.

    Notes
    -----
    **Compatibility with r5r.** The rows, their order, the columns and the
    geometries are those of ``transit_network_to_sf()`` in r5r 2.5.0, which
    repairs invalid lines with ``sf::st_make_valid()`` (GEOS, as shapely).
    Differences: no ``r5r_core`` argument; GeoDataFrames instead of ``sf``
    data frames; a route line of a single point is a shapely ``Point``,
    where r5r's ``sf`` holds a one-point ``LINESTRING`` (shapely cannot
    represent one).

    Examples
    --------
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> transit = peabiru.transit_network_to_sf(network)
    >>> sorted(transit)
    ['routes', 'stops']
    """
    assert_network(r5r_network)
    gpd, shapely = _geo()
    with core_errors("transit_network_to_sf"):
        r = _peabiru.transit_routes(r5r_network._core)
    lines = _lines(shapely, _i32(r["n_points"]), _f64(r["x"]), _f64(r["y"]))
    routes = gpd.GeoDataFrame(
        pd.DataFrame({
            "agency_id": _strings(np.array(r["agency_id"], dtype=object)),
            "agency_name": _strings(np.array(r["agency_name"], dtype=object)),
            "route_id": _strings(np.array(r["route_id"], dtype=object)),
            "long_name": _strings(np.array(r["long_name"], dtype=object)),
            "short_name": _strings(np.array(r["short_name"], dtype=object)),
            "mode": _strings(np.array(r["mode"], dtype=object)),
        }),
        geometry=gpd.GeoSeries(lines, crs="EPSG:4326"),
    )
    if len(routes) and not routes.geometry.is_valid.all():
        routes = routes.set_geometry(routes.geometry.make_valid())
    routes = routes[~routes.geometry.is_empty].reset_index(drop=True)
    s = _peabiru.transit_stops(r5r_network._core)
    stops = gpd.GeoDataFrame(
        pd.DataFrame({
            "stop_index": _i32(s["stop_index"]).copy(),
            "stop_id": _strings(np.array(s["stop_id"], dtype=object)),
            "stop_name": _strings(np.array(s["stop_name"], dtype=object)),
            "linked_to_street": _bools(s["linked_to_street"]),
        }),
        geometry=gpd.GeoSeries(_points_or_empty(shapely, _f64(s["lon"]), _f64(s["lat"])), crs="EPSG:4326"),
    )
    return {"stops": stops, "routes": routes}


def _points_or_empty(shapely: Any, x: np.ndarray, y: np.ndarray) -> Any:
    """Points, empty where a coordinate is NaN (sf's POINT EMPTY)."""
    pts = shapely.points(x, y)
    pts[np.isnan(x) | np.isnan(y)] = shapely.Point()
    return pts


_ISO = re.compile(r"^\d{4}-\d{2}-\d{2}$")


def _parse_dates(x: Any, arg: str) -> List[_dt.date]:
    """r5r's parse_date_input(): ``"YYYY-MM-DD"`` strings or dates (a
    datetime counts as its date)."""
    v = cm.Vec(x)
    # strings and dates, mixed as Python lists may mix them
    if v.type not in ("character", "Date", "POSIXct") and not (
        v.type == "list" and all(isinstance(d, (str, _dt.date)) for d in v.values)
    ):
        raise TypeError(f"'{arg}' must be a vector of character strings or Date objects.")
    texts = [d for d in v.values if isinstance(d, str)]
    if not all(_ISO.match(d) for d in texts):
        raise ValueError(
            f"Invalid date format found in '{arg}'. Please use the strict \"'YYYY-MM-DD'\" "
            "format for all date strings."
        )
    out: List[_dt.date] = []
    for d in v.values:
        if isinstance(d, _dt.date):
            out.append(d.date() if isinstance(d, _dt.datetime) else d)
            continue
        try:
            out.append(_dt.date.fromisoformat(d))
        except ValueError:
            raise ValueError(
                f"Could not parse all values in '{arg}'. One or more dates are "
                "logically invalid (e.g., '2025-02-29')."
            ) from None
    return out


def check_transit_availability(
    r5r_network: Network,
    dates: Optional[Union[DateLike, Sequence[DateLike]]] = None,
    start_date: Optional[DateLike] = None,
    end_date: Optional[DateLike] = None,
) -> pd.DataFrame:
    """Check public transport service availability by date.

    Counts the services in the network's GTFS feeds that are active on given dates, and their
    proportion of all services. Public transport routing only finds trips on
    dates when services run.

    With no date argument, the dates with an active service used by some
    trip (frequency-based trips included) are discovered from the network:
    every day of each such service's calendar on which it runs, and the
    dates its calendar exceptions add. Otherwise give either ``dates`` or
    both ``start_date`` and ``end_date`` (a continuous range), not both.
    ``total_services`` counts every service, used by trips or not.

    Parameters
    ----------
    r5r_network : Network
        A network built with [`build_network()`][peabiru.build_network].
    dates : str, datetime.date or a sequence of them, optional
        Dates to check, as ``"YYYY-MM-DD"`` strings or dates.
    start_date, end_date : str or datetime.date, optional
        The first and the last date of a range to check.

    Returns
    -------
    pandas.DataFrame
        ``date`` (datetime64), ``total_services``, ``active_services`` and
        ``pct_active`` (the proportion of active services). Discovered dates
        are ascending; with none, the table has no rows.

    Notes
    -----
    **Compatibility with r5r.** The arguments, their checks and messages,
    the columns and the values are those of ``check_transit_availability()``
    in r5r 2.5.0. Differences: no ``r5r_core`` argument; a pandas
    DataFrame instead of a ``data.table``, with ``date`` as
    ``datetime64`` (r5r's is a ``Date``).

    Examples
    --------
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> peabiru.check_transit_availability(network, dates=["2019-05-13"])
            date  total_services  active_services  pct_active
    0 2019-05-13               6                3         0.5
    """
    assert_network(r5r_network)
    vector = dates is not None and start_date is None and end_date is None
    range_ = dates is None and start_date is not None and end_date is not None
    discover = dates is None and start_date is None and end_date is None
    if not (discover or vector or range_):
        raise ValueError(
            "Incorrect date arguments provided. Omit all date arguments for discovery, "
            "or use one of these methods: Use the 'dates' argument to provide a vector "
            "of specific dates. Use both 'start_date' and 'end_date' to provide a "
            "continuous date range. You cannot mix these methods or provide an "
            "incomplete date range."
        )
    if discover:
        with core_errors("check_transit_availability"):
            found = _peabiru.transit_service_dates(r5r_network._core)
        days = [_dt.date(d // 10000, d // 100 % 100, d % 100) for d in found]
    elif vector:
        days = _parse_dates(dates, "dates")
    else:
        start = _parse_dates(start_date, "start_date")
        end = _parse_dates(end_date, "end_date")
        if len(start) > 1 or len(end) > 1:
            raise ValueError("`start_date` and `end_date` must each be a single date.")
        if start[0] > end[0]:
            raise ValueError("`start_date` must be before or the same as `end_date`.")
        n = (end[0] - start[0]).days + 1
        days = [start[0] + _dt.timedelta(days=k) for k in range(n)]
    counts = [
        r5r_network._core.transit_availability(d.year * 10000 + d.month * 100 + d.day)
        for d in days
    ]
    total = np.array([c[0] for c in counts], dtype=np.int32)
    active = np.array([c[1] for c in counts], dtype=np.int32)
    with np.errstate(divide="ignore", invalid="ignore"):
        pct = np.where(total == 0, 0.0, active / np.maximum(total, 1))
    return pd.DataFrame({
        "date": pd.to_datetime(pd.Series([d.isoformat() for d in days], dtype=object)),
        "total_services": total,
        "active_services": active,
        "pct_active": pct.astype(np.float64),
    })


def get_gtfs_errors(r5r_network: Union[Network, PathLike]) -> pd.DataFrame:
    """The errors gtfs-lib reports in the network's GTFS feeds.

    The errors that R5's GTFS library (Conveyal's gtfs-lib) reports when it
    reads the feeds, with every check of gtfs-lib's table loaders and of its
    post-load validation. [`build_network()`][peabiru.build_network] refuses
    feeds with errors of priority ``"HIGH"``; this function lists them, from
    the files a network was built from or from the ``.zip`` files of a
    directory (so it also works on feeds that cannot be built). The feeds
    are read again on each call; the rows are listed feed by feed, in the
    order gtfs-lib finds them.

    A feed on which gtfs-lib fails instead of listing errors (a date that
    is not ``YYYYMMDD``, a frequency of a trip that does not exist, a
    service or fare without an id, the first or last stop of a trip without
    a name) raises ``RuntimeError`` naming the cause, and so does a
    ``locations.geojson`` of GTFS-Flex zones that is not in UTF-8. GTFS-Flex
    zones and location groups are read as gtfs-lib reads them, for its
    errors; on-demand trips are not routed.

    Parameters
    ----------
    r5r_network : Network, str or os.PathLike
        A network built with [`build_network()`][peabiru.build_network], or
        a directory with GTFS feeds (``.zip``).

    Returns
    -------
    pandas.DataFrame
        ``file`` (the table, without ``.txt``), ``line`` (the record, the
        header being line 1; 0 for a whole table), ``type`` (gtfs-lib's
        error class), ``field`` and ``id`` (None when the error has none)
        and ``priority`` (``"HIGH"``, ``"MEDIUM"``, ``"LOW"`` or
        ``"UNKNOWN"``).

    Notes
    -----
    **Compatibility with r5r.** The argument, the rows and the columns
    (``file``, ``line``, ``type``, ``field``, ``id``, ``priority``) are
    those of ``get_gtfs_errors()`` in r5r 2.5.0. Differences: r5r reads the
    ``gtfs_errors.csv`` its ``build_network()`` wrote, and lists the rows
    in the order of a Java ``HashSet``, which changes from one run to the
    next; on a feed where gtfs-lib fails, r5r's ``build_network()`` fails
    without writing the file. A pandas DataFrame instead of a
    ``data.table``; a directory that does not exist is an error.

    Examples
    --------
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> errors = peabiru.get_gtfs_errors(network)
    >>> sorted(set(errors["type"]))
    ['DuplicateKeyError']
    """
    if isinstance(r5r_network, Network):
        paths = _peabiru.feed_paths(r5r_network._core)
    elif isinstance(r5r_network, (str, os.PathLike)):
        d = os.path.realpath(os.path.expanduser(os.fspath(r5r_network)))
        if not os.path.isdir(d):
            raise ValueError(f"Directory '{os.fspath(r5r_network)}' does not exist.")
        paths = _peabiru.gtfs_files(d)
    else:
        raise TypeError(
            "Assertion on 'r5r_network' failed: Must be a 'peabiru.Network' or a path."
        )
    e = _peabiru.gtfs_errors(paths)
    return pd.DataFrame({
        "file": _strings(np.array(e["file"], dtype=object)),
        "line": _i32(e["line"]).copy(),
        "type": _strings(np.array(e["type"], dtype=object)),
        "field": _strings(np.array(e["field"], dtype=object)),
        "id": _strings(np.array(e["id"], dtype=object)),
        "priority": _strings(np.array(e["priority"], dtype=object)),
    })


__all__ = [
    "check_transit_availability",
    "find_snap",
    "get_gtfs_errors",
    "street_network_bbox",
    "street_network_to_sf",
    "transit_network_to_sf",
]
