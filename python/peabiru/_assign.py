"""Input handling shared by the routing functions: the checks, defaults
and conversions of their arguments, the same as in the R package of
peabiru."""

from __future__ import annotations

import datetime as _dt
import math
import os
import sys
import warnings
from dataclasses import dataclass
from typing import Any, List, Optional, Sequence, Tuple

import numpy as np
import pandas as pd

from . import _checkmate as cm
from ._checks import (
    as_strings,
    assert_count,
    assert_flag,
    assert_int,
    assert_integerish,
    assert_number,
    assert_string,
    fail_assert,
    scalar,
    unique,
    unsupported,
)

TRANSIT_MODES = [
    "TRANSIT",
    "TRAM",
    "SUBWAY",
    "RAIL",
    "BUS",
    "FERRY",
    "CABLE_CAR",
    "GONDOLA",
    "FUNICULAR",
]
DIRECT_MODES = ["WALK", "BICYCLE", "CAR", "BICYCLE_RENT", "CAR_PARK"]


@dataclass
class Points:
    """Validated points: ids and coordinates as numpy arrays, plus the
    (possibly converted) data frame."""

    ids: np.ndarray  # object array of str
    lat: np.ndarray  # float64
    lon: np.ndarray  # float64
    df: pd.DataFrame

    def __len__(self) -> int:
        return len(self.ids)

    def lat_bytes(self) -> bytes:
        return np.ascontiguousarray(self.lat, dtype=np.float64).tobytes()

    def lon_bytes(self) -> bytes:
        return np.ascontiguousarray(self.lon, dtype=np.float64).tobytes()


def _is_geodataframe(df: Any) -> bool:
    gpd = sys.modules.get("geopandas")
    return gpd is not None and isinstance(df, gpd.GeoDataFrame)


def _is_string_column(col: pd.Series) -> bool:
    if len(col) == 0:
        return pd.api.types.is_string_dtype(col.dtype) or col.dtype == object
    return pd.api.types.infer_dtype(col, skipna=True) in ("string", "empty")


def assign_points_input(df: Any, name: str, unique_ids: bool = True) -> Points:
    """Points of a routing call: a DataFrame (or a GeoDataFrame of points)
    with id, lon, lat."""
    if _is_geodataframe(df):
        if len(df) > 0 and not (df.geom_type == "Point").all():
            raise TypeError(
                f"'{name}' must be either a 'DataFrame' or a 'GeoDataFrame' of points."
            )
        if df.crs is None or df.crs.to_epsg() != 4326:
            raise ValueError(
                f"'{name}' CRS must be WGS 84 (EPSG 4326). Please use either "
                "GeoDataFrame.set_crs() to set it or GeoDataFrame.to_crs() to "
                "reproject it."
            )
        # coordinates come from the geometry, never from attribute columns
        geom = df.geometry
        lon = geom.x.to_numpy(dtype=np.float64)
        lat = geom.y.to_numpy(dtype=np.float64)
        df = pd.DataFrame(df.drop(columns=df.geometry.name))
        df["lon"] = lon
        df["lat"] = lat
    elif not isinstance(df, pd.DataFrame):
        raise TypeError(
            f"'{name}' must be either a 'DataFrame' or a 'GeoDataFrame' of points."
        )

    missing = [c for c in ("id", "lat", "lon") if c not in df.columns]
    if missing:
        fail_assert(
            name,
            "Names must include the elements {'id','lat','lon'}, but is missing "
            "elements {" + ",".join(f"'{c}'" for c in missing) + "}",
        )
    for c in ("lon", "lat"):
        col = df[c]
        if not pd.api.types.is_numeric_dtype(col) or pd.api.types.is_bool_dtype(col):
            fail_assert(f"{name}$" + c, "Must be of type 'numeric'", TypeError)

    # the core takes double coordinates; missing values become NaN
    lon = df["lon"].to_numpy(dtype=np.float64, na_value=np.nan)
    lat = df["lat"].to_numpy(dtype=np.float64, na_value=np.nan)

    id_col = df["id"]
    # a missing id stays missing (None), as R's as.character(NA) is NA;
    # under pandas < 3 astype(str) turns it into the text "nan" or "<NA>"
    id_missing = id_col.isna().to_numpy(dtype=bool)
    if not _is_string_column(id_col):
        warnings.warn(f"'{name}$id' forcefully cast to character.", stacklevel=4)
        id_col = id_col.astype(str)
    ids = np.array(id_col.tolist(), dtype=object)
    ids[id_missing] = None

    na_coords = np.isnan(lon) | np.isnan(lat)
    if na_coords.any():
        affected = ", ".join(f'"{i}"' for i in ids[na_coords])
        warnings.warn(
            f"{int(na_coords.sum())} point(s) in '{name}' have missing lon/lat "
            f"coordinates and will not be routed. Affected id(s): {affected}.",
            stacklevel=4,
        )

    if unique_ids:
        dup = pd.Series(ids).duplicated()
        if dup.any():
            dups = unique(list(ids[dup.to_numpy()]))
            raise ValueError(
                f"`{name}` must not contain duplicated ids.\n\u2716 Duplicated id"
                + ("" if len(dups) == 1 else "s") + ": " + _val(dups) + "."
            )
    return Points(ids=ids, lat=lat, lon=lon, df=df)


@dataclass
class ModeList:
    direct_modes: str
    transit_mode: str
    access_mode: str
    egress_mode: str
    transit: bool


def check_destination_ids(d: "Points") -> None:
    """A destination without an id cannot be reported: an error, as in
    the R package."""
    if any(i is None or (isinstance(i, float) and math.isnan(i)) for i in d.ids):
        raise ValueError(
            "'destinations$id' cannot be missing."
        )


def _set(values: Sequence[str]) -> str:
    """checkmate's set_collapse(): {'a','b'}"""
    return "{" + ",".join(f"'{v}'" for v in unique(list(values))) + "}"


def _val(values: Sequence[Any]) -> str:
    """cli's {.val}: "a", "b", and "c" """
    q = [f'"{v}"' for v in values]
    if len(q) <= 1:
        return "".join(q)
    if len(q) == 2:
        return f"{q[0]} and {q[1]}"
    return ", ".join(q[:-1]) + ", and " + q[-1]


def assign_mode(mode: Any, mode_egress: Any) -> ModeList:
    """The modes of a call: access, transit and egress."""
    all_modes = TRANSIT_MODES + DIRECT_MODES
    # upper case and repeats dropped come before the checks, so a
    # repeated mode is accepted
    mode = cm.to_upper_unique(mode)
    cm.cm_assert_and(
        "mode",
        cm.check_character(mode, min_len=1, any_missing=False),
        lambda: cm.check_names(mode, subset_of=all_modes),
    )
    modes = [str(m) for m in cm.Vec(mode).values]
    mode_egress = cm.to_upper_unique(mode_egress)
    cm.cm_assert(cm.check_string(mode_egress), "mode_egress")
    cm.cm_assert(cm.check_names(mode_egress, subset_of=["WALK", "BICYCLE", "CAR"]), "mode_egress")
    egress = str(scalar(mode_egress))

    direct = [m for m in modes if m in DIRECT_MODES]
    transit = [m for m in modes if m in TRANSIT_MODES]
    if len(direct) > 1:
        raise ValueError(
            "Please use only 1 of {" + ",".join(f"'{m}'" for m in direct) + "} when routing."
        )
    if "CAR_PARK" in direct or "BICYCLE_RENT" in direct:
        raise ValueError("CAR_PARK and BICYCLE_RENT are not supported.")

    access = list(direct)
    if not transit:
        transit_modes: List[str] = [""]
        egress_mode = ""
    else:
        transit_modes = list(TRANSIT_MODES) if "TRANSIT" in transit else transit
        if not direct:
            access = direct = ["WALK"]
        egress_mode = egress

    ml = ModeList(
        direct_modes=";".join(direct),
        transit_mode=";".join(transit_modes),
        access_mode=";".join(access),
        egress_mode=egress_mode,
        transit=transit_modes != [""],
    )

    return ml


@dataclass
class Departure:
    date: str  # YYYY-MM-DD
    time: str  # HH:MM:SS
    yyyymmdd: int
    seconds: int


def _is_datetime(x: Any) -> bool:
    return isinstance(x, _dt.datetime) and not pd.isna(x)


def assign_departure(
    datetime: Any, mode_list: ModeList, name: str = "departure_datetime"
) -> Departure:
    """The departure: the date and the clock time of the datetime
    in its own time zone (or as given, when naive), with no conversion."""
    if datetime is None:
        if not mode_list.transit:
            datetime = _dt.datetime.now()
        else:
            raise ValueError(
                "You must pass a 'departure_datetime' for public transport routing. Results "
                "depend on the departure time. Supply an explicit datetime value "
                "to make results reproducible."
            )
    cm.cm_assert(cm.check_datetime(datetime, len_=1, any_missing=False), name)
    datetime = scalar(datetime)
    d = datetime
    return Departure(
        date=f"{d.year:04d}-{d.month:02d}-{d.day:02d}",
        time=f"{d.hour:02d}:{d.minute:02d}:{d.second:02d}",
        yyyymmdd=d.year * 10000 + d.month * 100 + d.day,
        seconds=d.hour * 3600 + d.minute * 60 + d.second,
    )


def minus_minutes(datetime: _dt.datetime, minutes: int) -> _dt.datetime:
    """``datetime - minutes`` as POSIXct arithmetic does it: on the absolute
    time for an aware datetime (then shown in its own zone), on the clock
    time for a naive one."""
    delta = _dt.timedelta(minutes=minutes)
    if datetime.tzinfo is None or datetime.utcoffset() is None:
        return datetime - delta
    utc = datetime.astimezone(_dt.timezone.utc) - delta
    return utc.astimezone(datetime.tzinfo)


def assign_max_street_time(
    max_time: Any, speed: Any, max_trip_duration: Any, mode: str
) -> int:
    """The limit of a street mode, in whole minutes."""
    var = f"max_{mode}_time"
    assert_number(max_time, var, lower=1)
    max_time = scalar(max_time)
    if math.isfinite(float(max_time)):
        assert_count(max_time, var, positive=True)
    assert_number(speed, f"{mode}_speed", finite=True, none_ok=True)
    assert_count(max_trip_duration, "max_trip_duration", positive=True)
    speed, max_trip_duration = scalar(speed), scalar(max_trip_duration)
    if speed is not None and speed <= 0:
        raise ValueError(f"Assertion on '{mode}_speed' failed: Must have value greater than 0.")
    if math.isinf(float(max_time)):
        return int(round(max_trip_duration))
    return int(round(max_time))


def assign_max_trip_duration(
    max_trip_duration: Any,
    modes: ModeList,
    max_walk_time: int,
    max_bike_time: int,
    max_car_time: int,
) -> int:
    """The trip limit: with a street mode alone, the mode's
    own limit caps the trip."""
    assert_count(max_trip_duration, "max_trip_duration", positive=True)
    mtd = int(round(scalar(max_trip_duration)))
    if not modes.transit:
        if modes.direct_modes == "WALK" and max_walk_time < mtd:
            mtd = max_walk_time
        if modes.direct_modes == "BICYCLE" and max_bike_time < mtd:
            mtd = max_bike_time
        if modes.direct_modes == "CAR" and max_car_time < mtd:
            mtd = max_car_time
    return mtd


def assign_opportunities(df: pd.DataFrame, colnames: Any) -> Tuple[List[str], List[bytes]]:
    """The opportunity columns: numeric columns, NA counted as 0."""
    cm.cm_assert(
        cm.check_character(colnames, min_len=1, unique=True, any_missing=False),
        "opportunities_colnames",
    )
    names = [str(c) for c in cm.Vec(colnames).values]
    cm.cm_assert(cm.check_names(list(map(str, df.columns)), must_include=names), "destinations")
    out = []
    for c in names:
        col = df[c]
        if not pd.api.types.is_numeric_dtype(col) or pd.api.types.is_bool_dtype(col):
            fail_assert(c, f"Must be of type 'numeric', not '{col.dtype}'", TypeError)
        x = col.to_numpy(dtype=np.float64, na_value=np.nan)
        if np.isinf(x).any():
            fail_assert(c, "Must be finite")
        na = np.isnan(x)
        if na.any():
            warnings.warn(
                f"'{c}' has {int(na.sum())} missing value(s), treated as 0.", stacklevel=4
            )
            x = np.where(na, 0.0, x)
        out.append(np.ascontiguousarray(x, dtype=np.float64).tobytes())
    return names, out


DECAY_FUNCTIONS = ["step", "exponential", "fixed_exponential", "linear", "logistic"]


def assign_decay_function(decay_function: Any, decay_value: Any) -> Tuple[str, float]:
    """The decay function and its value, checked together."""
    cm.cm_assert_and(
        "decay_function",
        cm.check_string(decay_function),
        lambda: cm.check_names(decay_function, subset_of=DECAY_FUNCTIONS),
    )
    decay_function = str(scalar(decay_function))
    assert_number(decay_value, "decay_value", finite=True, none_ok=True)
    decay_value = scalar(decay_value)

    if decay_value is not None and decay_function in ("step", "exponential"):
        raise ValueError(
            "Assertion on decay_value failed: must be None when decay_function "
            f"is {decay_function}."
        )
    if decay_value is None and decay_function in ("fixed_exponential", "linear", "logistic"):
        raise ValueError(
            "Assertion on decay_value failed: must not be None when decay_function "
            f"is {decay_function}."
        )
    if decay_function == "fixed_exponential" and (decay_value <= 0 or decay_value >= 1):
        raise ValueError(
            "Assertion on decay_value failed: must be a number between 0 and 1 "
            "(exclusive) when decay_function is fixed_exponential."
        )
    if decay_function in ("logistic", "linear") and decay_value < 1:
        raise ValueError(
            "Assertion on decay_value failed: must be a number greater than or "
            f"equal to 1 when decay_function is {decay_function}."
        )
    # R5 takes the linear width as whole minutes, under one hour
    if decay_function == "linear":
        assert_integerish(decay_value, "decay_value", lower=1, upper=59)
    # the placeholder 0 when decay_value is None (the function ignores it)
    return decay_function, 0.0 if decay_value is None else float(decay_value)


def assign_cutoffs(cutoffs: Any, decay_function: str) -> List[int]:
    """The cutoffs the engine receives (ascending, or the
    placeholder 0 for fixed_exponential)."""
    values = assert_integerish(
        cutoffs, "cutoffs", lower=1, upper=120, min_len=1, max_len=12, unique=True, none_ok=True
    )
    if values is not None and decay_function == "fixed_exponential":
        raise ValueError(
            "Assertion on cutoffs failed: must be None when decay_function "
            f"is {decay_function}."
        )
    if values is None and decay_function != "fixed_exponential":
        raise ValueError(
            "Assertion on cutoffs failed: must not be None when decay_function "
            f"is {decay_function}."
        )
    return [0] if values is None else sorted(values)


def assign_percentiles(percentiles: Any) -> List[int]:
    """The percentiles: ascending, as the output columns.

    An empty list is an error for every mode (R5 fails on it too; without
    the check a street matrix would come back with no travel time column).
    """
    values = assert_integerish(
        percentiles, "percentiles", lower=1, upper=99, min_len=1, max_len=5, unique=True
    )
    assert values is not None
    return sorted(values)


def assign_n_threads(n_threads: Any) -> int:
    """The threads: None or math.inf mean all cores (0 for the
    core)."""
    if n_threads is None:
        return 0
    assert_number(n_threads, "n_threads", lower=1)
    n_threads = scalar(n_threads)
    if math.isinf(float(n_threads)):
        return 0
    assert_count(n_threads, "n_threads", positive=True)
    return int(round(n_threads))


def check_speed(speed: Any, mode: str) -> None:
    """A walking or cycling speed in km/h, above zero."""
    name = f"{mode}_speed"
    assert_number(speed, name, finite=True)
    if scalar(speed) <= 0:
        raise ValueError(f"Assertion on '{name}' failed: Must have value greater than 0.")


def check_max_lts(max_lts: Any) -> None:
    """The level of traffic stress, 1 to 4."""
    assert_count(max_lts, "max_lts", positive=True)
    max_lts = scalar(max_lts)
    if max_lts < 1 or max_lts > 4:
        raise ValueError(
            f"{_num(max_lts)} is not a valid value for the maximum Level of Transit "
            "Stress (LTS).\nPlease enter a value between 1 and 4."
        )


def check_output_dir(output_dir: Any) -> Optional[str]:
    """The output directory: checkmate::assert_string() and
    checkmate::assert_directory_exists(). A string or a path-like object;
    returns the directory to write to as a string (``~`` expanded), or
    ``None``."""
    if output_dir is None:
        return None
    if not isinstance(output_dir, (str, os.PathLike)):
        fail_assert("output_dir", "Must be a single string", TypeError)
    path = os.fsdecode(os.fspath(output_dir))
    full = os.path.expanduser(path)
    if not os.path.exists(full):
        fail_assert("output_dir", f"Directory '{path}' does not exist", FileNotFoundError)
    if not os.path.isdir(full):
        fail_assert(
            "output_dir", f"Directory expected, but file in place: '{path}'", NotADirectoryError
        )
    return full


def check_draws(draws_per_minute: Any, time_window: Any = 1, mcraptor: bool = False) -> None:
    """The Monte Carlo draws: with McRAPTOR (a fare structure, or the
    alternatives of detailed_itineraries()), the draws R5's router cannot
    sample are rejected."""
    assert_count(draws_per_minute, "draws_per_minute", positive=True)
    if not mcraptor:
        return
    dpm, tw = int(round(scalar(draws_per_minute))), float(scalar(time_window))
    draws = int(tw * dpm)
    if draws == 200:
        raise ValueError(
            f"A 'time_window' of {_num(time_window)} minutes with {dpm} draw(s) per minute "
            "gives exactly 200 Monte Carlo draws, which R5's McRaptor algorithm does not "
            f"accept. Use a slightly different 'time_window', e.g. {_num(tw + 1)}."
        )
    m = 60 // dpm
    mu = (m - 1) / 2 + m // 2
    span = 60 * tw
    p = 0.0
    if m >= 2:
        sd = math.sqrt(span * (m * m - 1) / 12 / mu ** 3)
        z = ((span - (m - 1) / 2) / mu + 1 - draws) / sd
        p = math.exp(-z * z / 2) / math.sqrt(2 * math.pi) / sd
    if p < 5e-5:
        raise ValueError(
            f"'draws_per_minute' = {dpm} with a 'time_window' of {_num(time_window)} minutes "
            "is not supported with a 'fare_structure'. R5's McRaptor algorithm cannot "
            "sample this many departure times and could run indefinitely. Use a lower "
            "'draws_per_minute' or a shorter 'time_window'."
        )


def seed_arg(seed: Any) -> Optional[int]:
    assert_int(seed, "seed", none_ok=True)
    return None if seed is None else int(round(scalar(seed)))


def check_messages(verbose: Any, progress: Any) -> None:
    """'progress' and 'verbose' are checked; peabiru prints no progress
    counter."""
    assert_flag(verbose, "verbose")
    assert_flag(progress, "progress")


def check_transit_availability_on_date(network: Any, departure: Departure) -> None:
    """Warns when few services run on the date: R5 counts the
    services active on the date over all services."""
    total, active = network._core.transit_availability(departure.yyyymmdd)
    pct = 0.0 if total == 0 else active / total
    if pct == 0:
        raise ValueError(
            "There are no transit services available on the selected departure "
            f'date: "{departure.date}". Please ensure there is a GTFS in your '
            "data path & that the departure date falls within the GTFS calendar."
        )
    if pct < 0.2:
        warnings.warn(
            "Less than 20% of the transit services in the GTFS are running on the "
            "selected departure date.",
            stacklevel=4,
        )


def swap_od(origins: Points, destinations: Points, mode_list: ModeList, network: Any) -> bool:
    """With WALK alone, more origins than destinations and no elevation
    raster in the data directory, the matrix is routed from the destinations
    and the columns swapped back afterwards (fewer searches). The two
    directions can differ, so the rule is part of the result."""
    return (
        not mode_list.transit
        and mode_list.direct_modes == "WALK"
        and len(origins) > len(destinations)
        and not network.has_tif
    )


def seconds_to_hhmmss(s: np.ndarray) -> np.ndarray:
    """Seconds since midnight as HH:MM:SS, hours not wrapped
    at 24. Formats each distinct value once."""
    levels, inverse = np.unique(s, return_inverse=True)
    text = np.array(
        [f"{v // 3600:02d}:{(v // 60) % 60:02d}:{v % 60:02d}" for v in levels.tolist()],
        dtype=object,
    )
    return text[inverse.reshape(-1)] if len(s) else np.array([], dtype=object)


def _num(x: Any) -> str:
    """A number as R prints it."""
    return cm.r_num(x)
