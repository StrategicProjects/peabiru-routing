"""Scenarios: r5r's ``set_new_congestion()`` and ``set_new_lts()``.

The checks of r5r's ``congestion_poly2geojson()``, ``dt_to_speed_map()``,
``lts_lines2shp()`` and ``dt_to_lts_map()``, and the dictionary the engine
reads (``src/scenario.rs``): the data frame columns as text, the way r5r
hands them to R5 (``as.character()``, read again by Java), and the
geometries of the GeoDataFrames flattened.
"""

from __future__ import annotations

import math
from typing import Any, Dict, List, Optional

import numpy as np
import pandas as pd

from ._checks import fail_assert, is_number


def _q(values: List[str]) -> str:
    return "{" + ",".join(f"'{v}'" for v in values) + "}"


def _type_name(col: pd.Series) -> str:
    """checkmate's name of the type of an R column of this dtype."""
    if pd.api.types.is_bool_dtype(col):
        return "logical"
    if pd.api.types.is_integer_dtype(col):
        return "integer"
    if pd.api.types.is_float_dtype(col):
        return "double"
    if isinstance(col.dtype, pd.CategoricalDtype):
        return "factor"
    return "character"


def _names(df: pd.DataFrame, must: List[str], name: str) -> None:
    missing = [m for m in must if m not in df.columns]
    if missing:
        fail_assert(
            name,
            f"Names must include the elements {_q(must)}, but is missing elements {_q(missing)}",
        )


def _typed(
    col: pd.Series,
    kind: str,
    name: str,
    all_missing: bool = True,
    lower: float = -math.inf,
    upper: float = math.inf,
) -> None:
    """checkmate's assert_numeric / assert_integer / assert_character with
    ``any.missing = FALSE``."""
    t = _type_name(col)
    ok = {
        "numeric": t in ("integer", "double"),
        "integer": t == "integer",
        "character": t == "character" and col.map(lambda v: v is None or isinstance(v, str)).all(),
    }[kind]
    if not ok:
        fail_assert(name, f"Must be of type '{kind}', not '{t}'", TypeError)
    na = col.isna().to_numpy()
    if not all_missing and (len(col) == 0 or na.all()):
        fail_assert(name, "Contains only missing values")
    if na.any():
        fail_assert(name, f"Contains missing values (element {int(np.argmax(na)) + 1})")
    if kind != "character":
        v = col.to_numpy(dtype=float)
        low = np.flatnonzero(v < lower)
        if len(low):
            fail_assert(name, f"Element {low[0] + 1} is not >= {lower:g}")
        high = np.flatnonzero(v > upper)
        if len(high):
            fail_assert(name, f"Element {high[0] + 1} is not <= {upper:g}")


def _r_double_text(x: float) -> str:
    """R's ``as.character()`` of a double, as far as Java's parsers can
    tell: 15 significant digits, in fixed or scientific notation as R
    chooses (``100000`` is ``"1e+05"``, which ``Long.parseLong`` refuses)."""
    if math.isnan(x):
        return "NaN"
    if math.isinf(x):
        return "Inf" if x > 0 else "-Inf"
    sci = f"{x:.14e}"
    mantissa, exp = sci.split("e")
    mantissa = mantissa.rstrip("0").rstrip(".")
    e = int(exp)
    sci_text = f"{mantissa}e{'-' if e < 0 else '+'}{abs(e):02d}"
    digits = len(mantissa.replace("-", "").replace(".", ""))
    # fixed: the same significant digits
    decimals = max(0, digits - 1 - e)
    fixed = f"{x:.{decimals}f}"
    return fixed if len(fixed) <= len(sci_text) else sci_text


def _id_text(col: pd.Series) -> List[str]:
    if pd.api.types.is_integer_dtype(col):
        return [str(int(v)) for v in col]
    return [_r_double_text(float(v)) for v in col]


def _value_text(col: pd.Series) -> List[str]:
    if pd.api.types.is_integer_dtype(col):
        return [str(int(v)) for v in col]
    return [_r_double_text(float(v)) for v in col]


def _geometry_types(gdf: Any, choices: List[str], name: str) -> None:
    types = list(dict.fromkeys(str(t).upper() for t in gdf.geometry.geom_type))
    if not types:
        fail_assert(name, f"Must be a subset of {_q(choices)}, not empty")
    extra = [t for t in types if t not in choices]
    if extra:
        fail_assert(name, f"Must be a subset of {_q(choices)}, but has additional elements {_q(extra)}")


def _check_wgs84(gdf: Any, what: str) -> None:
    crs = gdf.crs
    ok = crs is not None
    if ok:
        try:
            from pyproj import CRS

            ok = CRS.from_user_input(crs) == CRS.from_epsg(4326)
        except ImportError:  # pragma: no cover - geopandas needs pyproj
            ok = crs.to_epsg() == 4326
    if not ok:
        raise ValueError(
            f"The CRS of parameter `{what}` must be WGS 84 (EPSG 4326). Please use either "
            "GeoDataFrame.set_crs() to set it or GeoDataFrame.to_crs() to reproject it."
        )


def _assert_scale(x: Any, name: str = "carspeed_scale") -> None:
    """r5r's check of carspeed_scale (checkmate::assert_numeric(lower = 0,
    finite = TRUE)), then a clear error for NaN, which that check lets
    through and r5r then fails on with an internal R error."""
    if x is None:
        fail_assert(name, "Must be of type 'numeric', not 'None'", TypeError)
    if not is_number(x):
        fail_assert(name, f"Must be of type 'numeric', not '{type(x).__name__}'", TypeError)
    x = float(x)
    if x < 0:
        fail_assert(name, "Element 1 is not >= 0")
    if math.isinf(x):
        fail_assert(name, "Must be finite")
    if math.isnan(x):
        raise ValueError(
            f"'{name}' must be a number that is not NaN (r5r 2.5.0 fails on this value "
            "with an internal R error)"
        )


def _is_geodataframe(x: Any) -> bool:
    return type(x).__name__ == "GeoDataFrame" and hasattr(x, "geometry")


def scenario_spec(new_carspeeds: Any, carspeed_scale: Any, new_lts: Any) -> Optional[Dict[str, Any]]:
    """The scenario of a routing call (see the module), ``None`` when it
    changes nothing."""
    for value, name in ((new_carspeeds, "new_carspeeds"), (new_lts, "new_lts")):
        if value is not None and not isinstance(value, pd.DataFrame):
            fail_assert(
                name,
                f"Must inherit from class 'DataFrame', but has class '{type(value).__name__}'",
                TypeError,
            )
    _assert_scale(carspeed_scale)
    spec: Dict[str, Any] = {"car": "none", "carspeed_scale": float(carspeed_scale), "lts": "none"}
    changed = False
    if new_carspeeds is not None or carspeed_scale != 1:
        changed = True
        if _is_geodataframe(new_carspeeds):
            x = new_carspeeds
            _names(x, ["poly_id", "scale", "priority", "geometry"], "names(new_speeds_poly)")
            _typed(x["poly_id"], "character", "new_speeds_poly$poly_id")
            _typed(x["scale"], "numeric", "new_speeds_poly$scale")
            _typed(x["priority"], "integer", "new_speeds_poly$priority")
            _geometry_types(
                x,
                ["POLYGON", "MULTIPOLYGON"],
                "unique(as.character(sf::st_geometry_type(new_speeds_poly)))",
            )
            _check_wgs84(x, "new_speeds")
            px: List[float] = []
            py: List[float] = []
            ring_len: List[int] = []
            ring_polygon: List[int] = []
            polygon_feature: List[int] = []
            for f, g in enumerate(x.geometry):
                parts = list(g.geoms) if g.geom_type == "MultiPolygon" else [g]
                for p in parts:
                    for ring in [p.exterior, *p.interiors]:
                        c = np.asarray(ring.coords, dtype=float)
                        # r5r's GeoJSON for R5 keeps 15 decimals
                        px.extend(float(f"{v:.15f}") for v in c[:, 0])
                        py.extend(float(f"{v:.15f}") for v in c[:, 1])
                        ring_len.append(len(c))
                        ring_polygon.append(len(polygon_feature))
                    polygon_feature.append(f)
            spec.update(
                car="polygon",
                poly_x=px,
                poly_y=py,
                ring_len=ring_len,
                ring_polygon=ring_polygon,
                polygon_feature=polygon_feature,
                scale=[float(v) for v in x["scale"]],
                priority=[float(v) for v in x["priority"]],
            )
        elif new_carspeeds is not None:
            dt = new_carspeeds
            _names(dt, ["osm_id", "max_speed", "speed_type"], "names(dt)")
            _typed(dt["osm_id"], "numeric", "dt$osm_id", all_missing=False)
            _typed(dt["max_speed"], "numeric", "dt$max_speed", all_missing=False)
            types = list(dict.fromkeys(dt["speed_type"].tolist()))
            if not (len(types) == 1 and types[0] in ("km/h", "scale")):
                fail_assert(
                    'length(unique(dt$speed_type)) == 1 && dt$speed_type[1] %in% c("km/h", "scale")',
                    "Must be TRUE",
                )
            spec.update(
                car="km/h" if types[0] == "km/h" else "scale",
                car_ids=_id_text(dt["osm_id"]),
                car_values=_value_text(dt["max_speed"]),
            )
    if new_lts is not None:
        changed = True
        if _is_geodataframe(new_lts):
            x = new_lts
            _names(x, ["line_id", "lts", "priority", "geometry"], "names(new_lts_lines)")
            _typed(x["line_id"], "character", "new_lts_lines$line_id")
            _typed(x["lts"], "integer", "new_lts_lines$lts", lower=1, upper=4)
            _typed(x["priority"], "integer", "new_lts_lines$priority")
            _geometry_types(
                x, ["LINESTRING"], "unique(as.character(sf::st_geometry_type(new_lts_lines)))"
            )
            _check_wgs84(x, "new_lts_lines")
            lx: List[float] = []
            ly: List[float] = []
            lens: List[int] = []
            for g in x.geometry:
                c = np.asarray(g.coords, dtype=float)
                lx.extend(c[:, 0].tolist())
                ly.extend(c[:, 1].tolist())
                lens.append(len(c))
            spec.update(
                lts="line",
                line_x=lx,
                line_y=ly,
                line_len=lens,
                line_lts=[int(v) for v in x["lts"]],
            )
        else:
            dt = new_lts
            _names(dt, ["osm_id", "lts"], "names(dt)")
            _typed(dt["osm_id"], "numeric", "dt$osm_id", all_missing=False)
            _typed(dt["lts"], "integer", "dt$lts", all_missing=False)
            spec.update(
                lts="way",
                lts_ids=_id_text(dt["osm_id"]),
                lts_values=[str(int(v)) for v in dt["lts"]],
            )
    return spec if changed else None
