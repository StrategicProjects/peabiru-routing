"""Input handling mirrored from r5r 2.5.0 (``R/assign.R``, ``R/set.R``,
``R/utils.R``), as the R package of peabiru does: the same checks, defaults
and conversions, so that a call gives what r5r gives for the same
arguments."""

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

from ._checks import (
    as_strings,
    assert_count,
    assert_flag,
    assert_int,
    assert_integerish,
    assert_number,
    assert_string,
    fail_assert,
    unique,
    unsupported,
)

R5R_TRANSIT_MODES = [
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
R5R_DIRECT_MODES = ["WALK", "BICYCLE", "CAR", "BICYCLE_RENT", "CAR_PARK"]


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
    """r5r assign_points_input(): a DataFrame (or a GeoDataFrame of points)
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
    # a missing id stays missing (None), as as.character(NA) is NA in r5r;
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
    """r5r assign_mode()."""
    all_modes = R5R_TRANSIT_MODES + R5R_DIRECT_MODES
    modes = unique([m.upper() for m in as_strings(mode, "mode")])
    extra = [m for m in modes if m not in all_modes]
    if extra:
        fail_assert("mode", "Names must be a subset of " + _set(all_modes)
                    + ", but has additional elements " + _set(extra))

    # as in r5r, unique() comes before the check, so a repeated mode passes
    if isinstance(mode_egress, (list, tuple)) and all(isinstance(m, str) for m in mode_egress):
        repeated = unique([m.upper() for m in mode_egress])
        if len(repeated) == 1:
            mode_egress = repeated[0]
    assert_string(mode_egress, "mode_egress")
    egress = mode_egress.upper()
    if egress not in ("WALK", "BICYCLE", "CAR"):
        fail_assert("mode_egress", "Names must be a subset of {'WALK','BICYCLE','CAR'}, "
                    "but has additional elements " + _set([egress]))

    direct = [m for m in modes if m in R5R_DIRECT_MODES]
    transit = [m for m in modes if m in R5R_TRANSIT_MODES]
    if len(direct) > 1:
        raise ValueError(
            "Please use only 1 of {" + ",".join(f"'{m}'" for m in direct) + "} when routing."
        )
    if "CAR_PARK" in direct or "BICYCLE_RENT" in direct:
        raise ValueError("CAR_PARK and BICYCLE_RENT are currently unsupported by r5r.")

    access = list(direct)
    if not transit:
        transit_modes: List[str] = [""]
        egress_mode = ""
    else:
        transit_modes = list(R5R_TRANSIT_MODES) if "TRANSIT" in transit else transit
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
    """r5r assign_departure(): the date and the clock time of the datetime
    in its own time zone (or as given, when naive), with no conversion."""
    if datetime is None:
        if not mode_list.transit:
            datetime = _dt.datetime.now()
        else:
            raise ValueError(
                f"You must pass a '{name}' for public transport routing. Results "
                "depend on the departure time. Supply an explicit datetime value "
                "to make results reproducible."
            )
    if not _is_datetime(datetime):
        fail_assert(name, "Must be a single datetime.datetime without missing values", TypeError)
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
    """r5r assign_max_street_time()"""
    var = f"max_{mode}_time"
    assert_number(max_time, var, lower=1)
    if math.isfinite(float(max_time)):
        assert_count(max_time, var, positive=True)
    assert_number(speed, f"{mode}_speed", finite=True, none_ok=True)
    assert_count(max_trip_duration, "max_trip_duration", positive=True)
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
    """r5r assign_max_trip_duration(): with a street mode alone, the mode's
    own limit caps the trip."""
    assert_count(max_trip_duration, "max_trip_duration", positive=True)
    mtd = int(round(max_trip_duration))
    if not modes.transit:
        if modes.direct_modes == "WALK" and max_walk_time < mtd:
            mtd = max_walk_time
        if modes.direct_modes == "BICYCLE" and max_bike_time < mtd:
            mtd = max_bike_time
        if modes.direct_modes == "CAR" and max_car_time < mtd:
            mtd = max_car_time
    return mtd


def assign_opportunities(df: pd.DataFrame, colnames: Any) -> Tuple[List[str], List[bytes]]:
    """r5r assign_opportunities(): numeric columns, NA counted as 0."""
    msg = "Must be a character vector of unique names without missing values"
    names = as_strings(colnames, "opportunities_colnames") if colnames is not None else []
    if not names or len(set(names)) < len(names):
        fail_assert("opportunities_colnames", msg)
    missing = [c for c in names if c not in df.columns]
    if missing:
        fail_assert(
            "destinations",
            "Names must include the elements {"
            + ",".join(f"'{c}'" for c in names)
            + "}, but is missing elements {"
            + ",".join(f"'{c}'" for c in missing)
            + "}",
        )
    out = []
    for c in names:
        col = df[c]
        if not pd.api.types.is_numeric_dtype(col) or pd.api.types.is_bool_dtype(col):
            fail_assert(c, "Must be of type 'numeric'", TypeError)
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
    """r5r assign_decay_function()"""
    assert_string(decay_function, "decay_function")
    if decay_function not in DECAY_FUNCTIONS:
        fail_assert("decay_function", "Names must be a subset of " + _set(DECAY_FUNCTIONS)
                    + ", but has additional elements " + _set([decay_function]))
    assert_number(decay_value, "decay_value", finite=True, none_ok=True)

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
    # r5r passes the placeholder 0 when decay_value is NULL (R5 ignores it)
    return decay_function, 0.0 if decay_value is None else float(decay_value)


def assign_cutoffs(cutoffs: Any, decay_function: str) -> List[int]:
    """r5r set_cutoffs(): the cutoffs R5 receives (ascending, or the
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
    """r5r set_percentiles(): ascending, as the output columns.

    An empty list is an error for every mode (R5 fails on it too; without
    the check a street matrix would come back with no travel time column).
    """
    values = assert_integerish(
        percentiles, "percentiles", lower=1, upper=99, min_len=1, max_len=5, unique=True
    )
    assert values is not None
    return sorted(values)


def assign_n_threads(n_threads: Any) -> int:
    """r5r set_n_threads(): None or math.inf mean all cores (0 for the
    core)."""
    if n_threads is None:
        return 0
    assert_number(n_threads, "n_threads", lower=1)
    if math.isinf(float(n_threads)):
        return 0
    assert_count(n_threads, "n_threads", positive=True)
    return int(round(n_threads))


def check_speed(speed: Any, mode: str) -> None:
    """r5r set_speed()"""
    name = f"{mode}_speed"
    assert_number(speed, name, finite=True)
    if speed <= 0:
        raise ValueError(f"Assertion on '{name}' failed: Must have value greater than 0.")


def check_max_lts(max_lts: Any) -> None:
    """r5r set_max_lts()"""
    assert_count(max_lts, "max_lts", positive=True)
    if max_lts < 1 or max_lts > 4:
        raise ValueError(
            f"{_num(max_lts)} is not a valid value for the maximum Level of Transit "
            "Stress (LTS).\nPlease enter a value between 1 and 4."
        )


def check_fares(fare_structure: Any, max_fare: Any) -> None:
    """r5r set_max_fare() and set_fare_structure(): fares are not
    supported."""
    if fare_structure is not None:
        unsupported("'fare_structure' (fare calculation)")
    assert_number(max_fare, "max_fare", lower=0)
    if math.isfinite(float(max_fare)):
        raise ValueError(
            f"'max_fare' requires a 'fare_structure': 'max_fare' is {_num(max_fare)} "
            "but 'fare_structure' is None, so no fares are calculated and the "
            "limit would be ignored. Leave 'max_fare' as math.inf."
        )


def check_output_dir(output_dir: Any) -> Optional[str]:
    """r5r set_output_dir(): checkmate::assert_string() and
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


def check_draws(draws_per_minute: Any) -> None:
    """r5r set_monte_carlo_draws() without a fare structure"""
    assert_count(draws_per_minute, "draws_per_minute", positive=True)


def seed_arg(seed: Any) -> Optional[int]:
    assert_int(seed, "seed", none_ok=True)
    return None if seed is None else int(round(seed))


def check_messages(verbose: Any, progress: Any) -> None:
    """'progress' and 'verbose' are checked as r5r does; peabiru prints no
    progress counter and has no Java log to make verbose."""
    assert_flag(verbose, "verbose")
    assert_flag(progress, "progress")


def check_transit_availability_on_date(network: Any, departure: Departure) -> None:
    """r5r check_transit_availability_on_date() (R/utils.R): R5 counts the
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
    """r5r reverse_if_direct_mode() (R/set.R): with WALK alone, more origins
    than destinations and no elevation raster in the data directory, r5r
    routes from the destinations and swaps the columns back afterwards. The
    two directions can differ, so the swap is reproduced."""
    return (
        not mode_list.transit
        and mode_list.direct_modes == "WALK"
        and len(origins) > len(destinations)
        and not network.has_tif
    )


def seconds_to_hhmmss(s: np.ndarray) -> np.ndarray:
    """Utils.getTimeFromSeconds in r5r's Java: HH:MM:SS, hours not wrapped
    at 24. Formats each distinct value once."""
    levels, inverse = np.unique(s, return_inverse=True)
    text = np.array(
        [f"{v // 3600:02d}:{(v // 60) % 60:02d}:{v % 60:02d}" for v in levels.tolist()],
        dtype=object,
    )
    return text[inverse.reshape(-1)] if len(s) else np.array([], dtype=object)


def _num(x: Any) -> str:
    x = float(x)
    return str(int(x)) if x.is_integer() else repr(x)
