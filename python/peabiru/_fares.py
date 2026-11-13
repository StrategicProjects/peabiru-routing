"""Fare structures and pareto_frontier().

A fare structure is a dict: ``max_discounted_transfers``,
``transfer_time_allowance``, ``fare_cap``, the tables ``fares_per_type``,
``fares_per_transfer`` and ``fares_per_route`` (pandas DataFrames) and
``debug_settings`` (a dict with ``output_file`` and ``trip_info``).
"""

from __future__ import annotations

import datetime as _dt
import math
import os
from typing import Any, Dict, List, Optional, Sequence, Union

import numpy as np
import pandas as pd

from . import _checkmate as cm
from . import _peabiru
from ._assign import (
    check_destination_ids,
    assign_departure,
    assign_mode,
    assign_n_threads,
    assign_percentiles,
    assign_points_input,
    check_draws,
    check_max_lts,
    check_messages,
    check_output_dir,
    check_speed,
    check_transit_availability_on_date,
    seed_arg,
)
from ._checks import (
    assert_count,
    assert_number,
    assert_string,
    core_errors,
    fail_assert,
    scalar,
    unsupported,
)
from ._network import Network, assert_network

Number = Union[int, float]
PathArg = Union[str, "os.PathLike[str]"]
FareStructure = Dict[str, Any]

_TYPE_COLS = ["type", "unlimited_transfers", "allow_same_route_transfer", "use_route_fare", "fare"]
_TRANSFER_COLS = ["first_leg", "second_leg", "fare"]
_ROUTE_COLS = [
    "agency_id", "agency_name", "route_id", "route_short_name", "route_long_name",
    "mode", "route_fare", "fare_type",
]
_ELEMENTS = [
    "max_discounted_transfers", "transfer_time_allowance", "fare_cap",
    "fares_per_type", "fares_per_transfer", "fares_per_route", "debug_settings",
]


def _count(x: float) -> Optional[int]:
    return None if math.isnan(x) else int(x)


def _from_core(d: Dict[str, Any]) -> FareStructure:
    """The core's flat dict as a fare structure."""
    def frame(cols: List[str], keys: List[str]) -> pd.DataFrame:
        # explicit dtypes: text columns of object dtype holding str (None
        # when missing), as in every other result, whatever pandas' default
        # string dtype is
        data = {}
        for c, k in zip(cols, keys):
            if c in ("unlimited_transfers", "allow_same_route_transfer", "use_route_fare"):
                data[c] = pd.Series(list(d[k]), dtype=bool)
            elif c in ("fare", "route_fare"):
                data[c] = pd.Series(list(d[k]), dtype=float)
            else:
                data[c] = pd.Series(list(d[k]), dtype=object)
        return pd.DataFrame(data, columns=cols)

    return {
        "max_discounted_transfers": _count(d["max_discounted_transfers"]),
        "transfer_time_allowance": _count(d["transfer_time_allowance"]),
        "fare_cap": float(d["fare_cap"]),
        "fares_per_type": frame(_TYPE_COLS, _TYPE_COLS),
        "fares_per_transfer": frame(_TRANSFER_COLS, ["first_leg", "second_leg", "transfer_fare"]),
        "fares_per_route": frame(_ROUTE_COLS, _ROUTE_COLS),
        "debug_settings": {"output_file": d["output_file"], "trip_info": d["trip_info"]},
    }


def _strings(col: Any) -> List[Optional[str]]:
    return [None if (v is None or (isinstance(v, float) and math.isnan(v))) else str(v) for v in col]


def _floats(col: Any) -> List[float]:
    return [float("nan") if v is None else float(v) for v in col]


def _bools(col: Any, name: str) -> List[bool]:
    out = []
    for k, v in enumerate(col):
        if v is None or (isinstance(v, float) and math.isnan(v)):
            raise ValueError(
                f"Check on 'fare_structure${name}' failed: Contains missing values "
                f"(element {k + 1})"
            )
        out.append(bool(v))
    return out


def _number(x: Any, name: str) -> float:
    if x is None:
        return float("nan")
    return float(x)


def _to_core(fs: FareStructure) -> Dict[str, Any]:
    """A fare structure as the core's flat dict (the checks of
    assert_fare_structure() on the values are the core's)."""
    t, tr, r = fs["fares_per_type"], fs["fares_per_transfer"], fs["fares_per_route"]

    def col(df: pd.DataFrame, c: str) -> Any:
        return df[c].tolist() if c in df.columns else []

    return {
        "max_discounted_transfers": _number(fs["max_discounted_transfers"], "max_discounted_transfers"),
        "transfer_time_allowance": _number(fs["transfer_time_allowance"], "transfer_time_allowance"),
        "fare_cap": _number(fs["fare_cap"], "fare_cap"),
        "type": _strings(col(t, "type")),
        "unlimited_transfers": _bools(col(t, "unlimited_transfers"), "fares_per_type$unlimited_transfers"),
        "allow_same_route_transfer": _bools(
            col(t, "allow_same_route_transfer"), "fares_per_type$allow_same_route_transfer"
        ),
        "use_route_fare": _bools(col(t, "use_route_fare"), "fares_per_type$use_route_fare"),
        "fare": _floats(col(t, "fare")),
        "first_leg": _strings(col(tr, "first_leg")),
        "second_leg": _strings(col(tr, "second_leg")),
        "transfer_fare": _floats(col(tr, "fare")),
        **{c: _strings(col(r, c)) for c in _ROUTE_COLS if c != "route_fare"},
        "route_fare": _floats(col(r, "route_fare")),
        "output_file": str(fs["debug_settings"]["output_file"]),
        "trip_info": str(fs["debug_settings"]["trip_info"]),
    }


def setup_fare_structure(
    network: Network,
    base_fare: Number,
    by: str = "MODE",
    debug_path: Optional[str] = None,
    debug_info: Optional[str] = None,
) -> FareStructure:
    """Set up a fare structure to calculate the monetary costs of trips.

    Every route, fare type and transfer between two types costs
    ``base_fare`` (read as a 32-bit float), with one fare type
    per mode (``by="MODE"``), per agency (``"AGENCY_ID"``,
    ``"AGENCY_NAME"``) or one for all routes (``"GENERIC"``). Edit the
    result to follow the fare rules of the study area, and pass it to the
    routing functions as ``fare_structure`` (see [Fares](fares.md)).

    Parameters
    ----------
    network : Network
        A network built with [`build_network()`][peabiru.build_network].
    base_fare : float
        The fare of every route, type and transfer, at least 0.
    by : {"MODE", "AGENCY_ID", "AGENCY_NAME", "GENERIC"}, default "MODE"
        What a fare type groups: the routes of a mode, of an agency (by id or
        by name), or all routes. Case does not matter.
    debug_path, debug_info : str, optional
        Debug settings, kept in ``debug_settings``: a ``.csv`` path and
        ``"MODE"``, ``"ROUTE"`` (the default with a ``debug_path``) or
        ``"MODE_ROUTE"``. No debug file is written.

    Returns
    -------
    dict
        The fare structure: ``max_discounted_transfers`` (1),
        ``transfer_time_allowance`` (120 minutes), ``fare_cap`` (``inf``),
        the DataFrames ``fares_per_type`` (``type``,
        ``unlimited_transfers``, ``allow_same_route_transfer``,
        ``use_route_fare``, ``fare``), ``fares_per_transfer``
        (``first_leg``, ``second_leg``, ``fare``) and ``fares_per_route``
        (``agency_id``, ``agency_name``, ``route_id``, ``route_short_name``,
        ``route_long_name``, ``mode``, ``route_fare``, ``fare_type``), and
        ``debug_settings`` (a dict with ``output_file`` and ``trip_info``).

    Notes
    -----
    The debug settings are kept in the structure, but no debug file is
    written.

    Examples
    --------
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> fares = peabiru.setup_fare_structure(network, base_fare=4.4)
    >>> sorted(fares["fares_per_type"]["type"])
    ['BUS', 'RAIL', 'SUBWAY']
    >>> fares["fares_per_type"]["fare"].tolist()
    [4.4, 4.4, 4.4]
    """
    assert_network(network)
    cm.cm_assert(cm.check_numeric(base_fare, lower=0, len_=1, any_missing=False), "base_fare")
    base_fare = float(scalar(base_fare))
    by_options = ["MODE", "AGENCY_ID", "AGENCY_NAME", "GENERIC"]
    # toupper(by): None and [] are character(0)
    vb = cm.Vec(by)
    if vb.type == "NULL" or len(vb) == 0 or (vb.type == "character" and len(vb) == 1):
        by = cm.to_upper_unique(by)
    cm.cm_assert_and("by", cm.check_string(by), lambda: cm.check_names(by, subset_of=by_options))
    by = str(scalar(by))
    assert_string(debug_path, "debug_path", none_ok=True)
    if debug_path is not None and not str(debug_path).endswith(".csv"):
        fail_assert("debug_path", "Must comply to pattern '\\.csv$'")
    if debug_path is None and debug_info is not None:
        raise ValueError("Please specify a file to write debug info to with 'debug_path'.")
    if debug_path is not None and debug_info is None:
        debug_info = "ROUTE"
    assert_string(debug_info, "debug_info", none_ok=True)
    if debug_info is not None:
        debug_info = debug_info.upper()
        if debug_info not in ("MODE", "ROUTE", "MODE_ROUTE"):
            fail_assert("debug_info", "Must be a subset of {'MODE','ROUTE','MODE_ROUTE'}, but has "
                        f"additional elements {{'{debug_info}'}}")
    with core_errors("setup_fare_structure"):
        fs = _from_core(_peabiru.fare_setup(network._core, float(base_fare), by))
    fs["debug_settings"] = (
        {"output_file": debug_path, "trip_info": debug_info} if debug_path is not None
        else {"output_file": "", "trip_info": "MODE"}
    )
    return fs


def write_fare_structure(fare_structure: FareStructure, file_path: PathArg) -> PathArg:
    """Write a fare structure to a ``.zip`` file.

    Five CSV files (the settings and the three tables) in a ``.zip``
    archive that deflates them, to be read again with
    [`read_fare_structure()`][peabiru.read_fare_structure] here or in the R
    package.

    Parameters
    ----------
    fare_structure : dict
        A fare structure (see
        [`setup_fare_structure()`][peabiru.setup_fare_structure]).
    file_path : str or path-like
        A path ending in ``.zip``.

    Returns
    -------
    str or path-like
        ``file_path``.

    Notes
    -----
    The CSV files are in the format of R's ``data.table::fwrite()``, so the
    R package of peabiru reads the file too.

    Examples
    --------
    >>> import os, tempfile, peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> fares = peabiru.setup_fare_structure(network, base_fare=4.4)
    >>> path = os.path.join(tempfile.mkdtemp(), "fares.zip")
    >>> peabiru.write_fare_structure(fares, path) == path
    True
    """
    if file_path is None:
        fail_assert("file_path", "Must be of type 'string', not 'None'", TypeError)
    p = os.fspath(file_path)
    if not p.endswith(".zip"):
        fail_assert("file_path", "Must comply to pattern '\\.zip$'")
    # the structure is checked before anything is written, as the routing
    # functions check it
    assert_fare_structure(fare_structure)
    with core_errors("write_fare_structure"):
        data = _peabiru.fare_write(_to_core(fare_structure))
    with open(p, "wb") as f:
        f.write(data)
    return file_path


def read_fare_structure(file_path: PathArg, encoding: str = "UTF-8") -> FareStructure:
    """Read a fare structure from a ``.zip`` file.

    Reads the files written by
    [`write_fare_structure()`][peabiru.write_fare_structure], here or in the
    R package.

    Parameters
    ----------
    file_path : str or path-like
        A fare structure ``.zip`` file.
    encoding : {"UTF-8", "unknown", "Latin-1"}, default "UTF-8"
        The encoding of the files (``"unknown"`` is read as UTF-8).

    Returns
    -------
    dict
        The fare structure (see
        [`setup_fare_structure()`][peabiru.setup_fare_structure]).

    Examples
    --------
    >>> import os, tempfile, peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> path = os.path.join(tempfile.mkdtemp(), "fares.zip")
    >>> _ = peabiru.write_fare_structure(peabiru.setup_fare_structure(network, 4.4), path)
    >>> peabiru.read_fare_structure(path)["transfer_time_allowance"]
    120
    """
    p = os.fspath(file_path)
    if not os.path.isfile(p):
        fail_assert("file_path", f"File does not exist: '{p}'")
    if not p.lower().endswith(".zip"):
        fail_assert("file_path", "File extension must be in {'zip'}")
    if encoding not in ("unknown", "UTF-8", "Latin-1"):
        fail_assert("encoding", "Must be a subset of {'unknown','UTF-8','Latin-1'}, but has "
                    f"additional elements {{'{encoding}'}}")
    with open(p, "rb") as f:
        data = f.read()
    with core_errors("read_fare_structure"):
        return _from_core(_peabiru.fare_read(data, encoding))


def assert_fare_structure(fare_structure: Any) -> None:
    """The checks of a fare structure: the element names here, the values
    and tables in the core."""
    if not isinstance(fare_structure, dict):
        fail_assert("fare_structure", "Must be of type 'list'", TypeError)
    if "type" in fare_structure:
        unsupported("A fare structure with a 'type' (a named fare calculator instead of fare rules)")
    if sorted(fare_structure) != sorted(_ELEMENTS):
        fail_assert("names(fare_structure)", "Must have exactly the names {'" + "','".join(_ELEMENTS) + "'}")
    for n in ("max_discounted_transfers", "transfer_time_allowance", "fare_cap"):
        assert_number(fare_structure[n], f"fare_structure${n}", lower=0)
    known = {"fares_per_type": _TYPE_COLS, "fares_per_transfer": _TRANSFER_COLS,
             "fares_per_route": _ROUTE_COLS}
    for n, cols in known.items():
        if not isinstance(fare_structure[n], pd.DataFrame):
            raise TypeError(f"Check on 'fare_structure${n}' failed: Must be a data.frame")
        # only the columns of a fare structure
        extra = [c for c in fare_structure[n].columns if c not in cols]
        if extra:
            raise ValueError(
                f"fare_structure${n} has column(s) that are not part of a fare "
                f"structure: {extra}."
            )
    ds = fare_structure["debug_settings"]
    if not isinstance(ds, dict) or sorted(ds) != ["output_file", "trip_info"]:
        fail_assert("fare_structure$debug_settings", "Must have exactly the names {'output_file','trip_info'}")
    assert_string(ds["output_file"], "fare_structure$debug_settings$output_file")
    assert_string(ds["trip_info"], "fare_structure$debug_settings$trip_info")


def fare_arg(fare_structure: Any, max_fare: Any, network: Network) -> Optional[Dict[str, Any]]:
    """The fare arguments of a routing call: the structure checked
    and read in for the network's routes whatever the modes; a
    finite max_fare needs a structure. Returns the core's flat dict."""
    core = None
    if fare_structure is not None:
        assert_fare_structure(fare_structure)
        core = _to_core(fare_structure)
        with core_errors("fare_structure"):
            _peabiru.fare_check(network._core, core)
    assert_number(max_fare, "max_fare", lower=0)
    if math.isfinite(float(scalar(max_fare))) and fare_structure is None:
        raise ValueError(
            f"'max_fare' requires a 'fare_structure'. 'max_fare' is {cm.r_num(scalar(max_fare))} "
            "but 'fare_structure' is None, so no fares are calculated and the limit would be "
            "ignored. Pass a fare structure (see 'setup_fare_structure()') or leave "
            "'max_fare' as math.inf."
        )
    return core


def pareto_frontier(
    network: Network,
    origins: Any,
    destinations: Any,
    mode: Union[str, Sequence[str]] = ("WALK", "TRANSIT"),
    mode_egress: str = "WALK",
    departure_datetime: Optional[_dt.datetime] = None,
    time_window: Number = 10,
    seed: Optional[int] = None,
    percentiles: Union[Number, Sequence[Number]] = 50,
    max_walk_time: Number = math.inf,
    max_bike_time: Number = math.inf,
    max_car_time: Number = math.inf,
    max_trip_duration: Number = 120,
    fare_structure: Optional[FareStructure] = None,
    fare_cutoffs: Optional[Sequence[Number]] = None,
    walk_speed: Number = 3.6,
    bike_speed: Number = 12,
    max_rides: Number = 3,
    max_lts: Number = 2,
    n_threads: Number = math.inf,
    verbose: bool = False,
    progress: bool = True,
    output_dir: Optional[PathArg] = None,
) -> pd.DataFrame:
    """Travel time and monetary cost Pareto frontier.

    For every value of ``fare_cutoffs``, in the order given, the travel
    times of the multi-criteria router (McRAPTOR over ``time_window`` sampled
    departure times) when no trip may cost more than the cutoff; a
    destination and percentile gets a row at each cutoff where its travel
    time changes and is below ``max_trip_duration``. ``monetary_cost`` is
    the cutoff as the router reads it, a 32-bit float (4.8 is
    4.800000190734863).

    Parameters
    ----------
    network, origins, destinations, mode, mode_egress, departure_datetime, time_window, seed, percentiles, max_walk_time, max_bike_time, max_car_time, max_trip_duration, walk_speed, bike_speed, max_rides, max_lts, n_threads, verbose, progress
        As in [`travel_time_matrix()`][peabiru.travel_time_matrix].
    fare_structure : dict
        A fare structure (see
        [`setup_fare_structure()`][peabiru.setup_fare_structure]); required.
    fare_cutoffs : sequence of float
        The fare cutoffs, at least 0 and unique; required.
    output_dir : None
        Must be ``None``: writing the frontier to files raises
        [`UnsupportedError`][peabiru.UnsupportedError].

    Returns
    -------
    pandas.DataFrame
        ``from_id``, ``to_id`` (strings, object dtype), ``percentile``
        (``int32``), ``travel_time`` (``Int32``, minutes) and
        ``monetary_cost`` (``float64``).

    Notes
    -----
    ``fare_structure`` and ``fare_cutoffs`` have a default of ``None``
    because Python cannot have required arguments after optional ones;
    leaving them out is an error.

    Examples
    --------
    >>> import datetime as dt, peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> points = peabiru.sample_points()
    >>> fares = peabiru.setup_fare_structure(network, base_fare=4.4)
    >>> pf = peabiru.pareto_frontier(
    ...     network, points.iloc[:3], points,
    ...     departure_datetime=dt.datetime(2019, 5, 13, 14, 0),
    ...     fare_structure=fares, fare_cutoffs=[0, 4.4, 8.8], seed=42,
    ...     n_threads=2,
    ... )
    >>> list(pf.columns)
    ['from_id', 'to_id', 'percentile', 'travel_time', 'monetary_cost']
    """
    from ._routing import _street_limits

    if fare_structure is None:
        raise ValueError("'fare_structure' is required. Create one with setup_fare_structure() "
                         "or read_fare_structure().")
    if fare_cutoffs is None:
        raise ValueError("'fare_cutoffs' is required. Pass the monetary cutoffs of the "
                         "frontier, e.g. the fares of your 'fare_structure'.")
    o = assign_points_input(origins, "origins")
    d = assign_points_input(destinations, "destinations")
    check_destination_ids(d)
    mode_list = assign_mode(mode, mode_egress)
    departure = assign_departure(departure_datetime, mode_list)
    assert_network(network)
    if mode_list.transit:
        check_transit_availability_on_date(network, departure)
    lim = _street_limits(
        max_walk_time, max_bike_time, max_car_time, walk_speed, bike_speed,
        max_trip_duration, mode_list,
    )
    assert_count(time_window, "time_window", positive=True)
    pcts = assign_percentiles(percentiles)
    check_draws(1, time_window, mcraptor=mode_list.transit)
    s = seed_arg(seed)
    check_speed(walk_speed, "walk")
    check_speed(bike_speed, "bike")
    assert_count(max_rides, "max_rides", positive=True)
    check_max_lts(max_lts)
    threads = assign_n_threads(n_threads)
    check_messages(verbose, progress)
    core = fare_arg(fare_structure, math.inf, network)
    check_output_dir(output_dir)
    if output_dir is not None:
        unsupported("'output_dir' in pareto_frontier()")
    cutoffs = _fare_cutoffs(fare_cutoffs)
    with core_errors("pareto_frontier"):
        res = _peabiru.pareto(
            network._core, o.lat_bytes(), o.lon_bytes(), d.lat_bytes(), d.lon_bytes(),
            mode_list.access_mode, mode_list.transit_mode if mode_list.transit else "",
            mode_list.egress_mode if mode_list.transit else mode_list.access_mode,
            departure.yyyymmdd, departure.seconds, int(time_window), pcts,
            lim["walk"], lim["bike"], lim["car"], lim["trip"], float(walk_speed),
            float(bike_speed), int(max_lts), int(max_rides), s, threads, core, cutoffs,
        )
    frm = np.frombuffer(res["from"], dtype=np.int32)
    to = np.frombuffer(res["to"], dtype=np.int32)
    tt = np.frombuffer(res["travel_time"], dtype=np.int32)
    from ._routing import _nullable_int, _strings as _str_col

    return pd.DataFrame({
        "from_id": _str_col(o.ids[frm]),
        "to_id": _str_col(d.ids[to]),
        "percentile": np.frombuffer(res["percentile"], dtype=np.int32),
        "travel_time": _nullable_int(tt, tt > lim["trip"]),
        "monetary_cost": np.frombuffer(res["monetary_cost"], dtype=np.float64),
    })


def _fare_cutoffs(fare_cutoffs: Any) -> List[float]:
    """The fare cutoffs: numeric, at least one, no NA, >= 0, unique."""
    cm.cm_assert(
        cm.check_numeric(fare_cutoffs, lower=0, any_missing=False, min_len=1, unique=True),
        "fare_cutoffs",
    )
    values = [float(v) for v in cm.Vec(fare_cutoffs).values]
    return values
