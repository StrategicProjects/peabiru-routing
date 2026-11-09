"""Argument checks.

r5r validates its arguments with checkmate; these are Python equivalents of
the few checkmate assertions it uses (as the R package of peabiru does), so
that the same inputs are accepted and rejected, with the same messages.
Python's ``None`` plays R's ``NULL`` and ``math.inf`` plays ``Inf``.
"""

from __future__ import annotations

import math
import numbers
from typing import Any, List, Optional, Sequence

import numpy as np

INT_MAX = 2147483647
# checkmate's default tolerance for "integerish"
_TOL = math.sqrt(np.finfo(float).eps)


class UnsupportedError(NotImplementedError):
    """An option of r5r that peabiru does not support yet.

    peabiru never ignores an argument it cannot honour: requesting it raises
    this error (a subclass of ``NotImplementedError``).
    """


def fail_assert(name: str, msg: str, exc: type = ValueError) -> None:
    raise exc(f"Assertion on '{name}' failed: {msg}.")


def unsupported(what: str) -> None:
    """An argument peabiru does not support yet: always an error."""
    raise UnsupportedError(f"{what} is not supported by peabiru yet.")


def is_number(x: Any) -> bool:
    """A real number (Python or numpy), not a boolean."""
    return isinstance(x, numbers.Real) and not isinstance(x, (bool, np.bool_))


def is_integerish_value(x: float) -> bool:
    return math.isfinite(x) and abs(x - round(x)) < _TOL


def assert_number(
    x: Any,
    name: str,
    lower: float = -math.inf,
    upper: float = math.inf,
    finite: bool = False,
    none_ok: bool = False,
) -> None:
    """checkmate::assert_number"""
    if x is None:
        if none_ok:
            return
        fail_assert(name, "Must be of type 'number', not 'None'", TypeError)
    if not is_number(x):
        fail_assert(name, "Must be of type 'number'", TypeError)
    x = float(x)
    if math.isnan(x):
        fail_assert(name, "May not be NA")
    if finite and not math.isfinite(x):
        fail_assert(name, "Must be finite")
    if x < lower:
        fail_assert(name, f"Element 1 is not >= {_fmt(lower)}")
    if x > upper:
        fail_assert(name, f"Element 1 is not <= {_fmt(upper)}")


def assert_count(x: Any, name: str, positive: bool = False) -> None:
    """checkmate::assert_count"""
    if not is_number(x):
        fail_assert(name, "Must be of type 'count'", TypeError)
    x = float(x)
    if math.isnan(x):
        fail_assert(name, "May not be NA")
    if not is_integerish_value(x) or abs(x) > INT_MAX:
        fail_assert(name, "Must be of type 'count'", TypeError)
    if x < int(positive):
        fail_assert(name, f"Must be >= {int(positive)}")


def assert_int(x: Any, name: str, none_ok: bool = False) -> None:
    """checkmate::assert_int"""
    if x is None and none_ok:
        return
    if (
        not is_number(x)
        or math.isnan(float(x))
        or not is_integerish_value(float(x))
        or abs(float(x)) > INT_MAX
    ):
        fail_assert(name, "Must be a single integerish value", TypeError)


def as_number_list(x: Any, name: str, kind: str) -> List[float]:
    """A scalar or a one-dimensional sequence of numbers, as floats."""
    if is_number(x):
        return [float(x)]
    if isinstance(x, (str, bytes)) or not hasattr(x, "__iter__"):
        fail_assert(name, f"Must be of type '{kind}'", TypeError)
    values = list(x)
    if not all(is_number(v) for v in values):
        fail_assert(name, f"Must be of type '{kind}'", TypeError)
    return [float(v) for v in values]


def assert_integerish(
    x: Any,
    name: str,
    lower: float = -math.inf,
    upper: float = math.inf,
    min_len: int = 0,
    max_len: float = math.inf,
    unique: bool = False,
    none_ok: bool = False,
) -> Optional[List[int]]:
    """checkmate::assert_integerish with the options r5r uses; returns the
    values as ints (or None)."""
    if x is None:
        if none_ok:
            return None
        fail_assert(name, "Must be of type 'integerish', not 'None'", TypeError)
    values = as_number_list(x, name, "integerish")
    if not all(math.isnan(v) or is_integerish_value(v) for v in values):
        fail_assert(name, "Must be of type 'integerish'", TypeError)
    if any(math.isnan(v) for v in values):
        fail_assert(name, "Contains missing values")
    if len(values) < min_len:
        fail_assert(name, f"Must have length >= {min_len}")
    if len(values) > max_len:
        fail_assert(name, f"Must have length <= {_fmt(max_len)}")
    if any(v < lower for v in values):
        fail_assert(name, f"All elements must be >= {_fmt(lower)}")
    if any(v > upper for v in values):
        fail_assert(name, f"All elements must be <= {_fmt(upper)}")
    if unique and len(set(values)) < len(values):
        fail_assert(name, "Contains duplicated values")
    return [int(round(v)) for v in values]


def assert_flag(x: Any, name: str) -> None:
    """checkmate::assert_logical(x, len = 1, any.missing = FALSE)"""
    if not isinstance(x, (bool, np.bool_)):
        fail_assert(name, "Must be a single True or False", TypeError)


def assert_string(x: Any, name: str, none_ok: bool = False) -> None:
    """checkmate::assert_string"""
    if x is None and none_ok:
        return
    if not isinstance(x, str):
        fail_assert(name, "Must be a single string", TypeError)


def as_strings(x: Any, name: str) -> List[str]:
    """A string or a sequence of strings, as a list."""
    if isinstance(x, str):
        return [x]
    msg = "Must be of type 'character' with at least 1 element and no missing values"
    if isinstance(x, bytes) or not hasattr(x, "__iter__"):
        fail_assert(name, msg, TypeError)
    values = list(x)
    if len(values) < 1 or not all(isinstance(v, str) for v in values):
        fail_assert(name, msg, TypeError)
    return values


def unique(values: Sequence[Any]) -> List[Any]:
    """R's unique(): first occurrences, in order."""
    seen: set = set()
    out = []
    for v in values:
        if v not in seen:
            seen.add(v)
            out.append(v)
    return out


def _fmt(x: float) -> str:
    """A bound as R prints it."""
    if math.isinf(x):
        return "Inf" if x > 0 else "-Inf"
    if float(x).is_integer():
        return str(int(x))
    return repr(float(x))
