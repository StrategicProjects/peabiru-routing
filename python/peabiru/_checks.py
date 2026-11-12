"""Argument checks.

r5r validates its arguments with checkmate; these are Python equivalents of
the few checkmate assertions it uses (as the R package of peabiru does), so
that the same inputs are accepted and rejected, with the same messages.
Python's ``None`` plays R's ``NULL`` and ``math.inf`` plays ``Inf``.
"""

from __future__ import annotations

import math
import numbers
import re
from contextlib import contextmanager
from typing import Any, Iterator, List, Mapping, Optional, Sequence

import numpy as np

from . import _checkmate as cm

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


_NUMBER = re.compile(r"(?<![\w.])-?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?(?![\w.])")
_PATHS = re.compile(r"at most (\d+) destinations are supported with paths")


def _short_number(m: "re.Match[str]") -> str:
    tok = m.group(0)
    if "e" in tok.lower() or len(tok) > 10 and "." in tok:
        return f"{float(tok):g}"
    return tok


def user_message(
    msg: str,
    function: str,
    names: Optional[Mapping[str, str]] = None,
    n_destinations: Optional[int] = None,
) -> str:
    """An error message of the engine in the terms of ``function``'s
    arguments: the engine's parameter names in ``names`` become the argument
    names, and numbers in scientific or long decimal notation are shortened."""
    renames = dict(names or {})
    for core in sorted(renames, key=len, reverse=True):
        msg = re.sub(rf"\b{re.escape(core)}\b", renames[core], msg)
    paths = _PATHS.search(msg)
    if paths:
        got = "" if n_destinations is None else f", got {n_destinations}"
        msg = f"{function}() supports at most {paths.group(1)} destinations{got}"
    return _NUMBER.sub(_short_number, msg)


@contextmanager
def core_errors(
    function: str,
    names: Optional[Mapping[str, str]] = None,
    n_destinations: Optional[int] = None,
) -> Iterator[None]:
    """Re-raise the engine's ``ValueError`` with :func:`user_message`."""
    try:
        yield
    except ValueError as e:
        msg = user_message(str(e), function, names, n_destinations)
        # the engine's refusals of what it does not reproduce
        if msg.startswith("not supported: "):
            raise UnsupportedError(msg) from None
        raise ValueError(msg) from None


def unsupported(what: str, instead: str = "") -> None:
    """An argument peabiru does not support yet: always an error."""
    raise UnsupportedError(f"{what} is not supported by peabiru{instead}.")


def is_number(x: Any) -> bool:
    """A real number (Python or numpy), not a boolean."""
    return isinstance(x, numbers.Real) and not isinstance(x, (bool, np.bool_))


def is_integerish_value(x: float) -> bool:
    return math.isfinite(x) and abs(x - round(x)) < _TOL


def scalar(x: Any) -> Any:
    """The one element of a value that passed a scalar check (``[5]`` is
    5, as ``c(5)`` is in R)."""
    if isinstance(x, (list, tuple, np.ndarray)) or type(x).__name__ == "Series":
        return list(np.asarray(x, dtype=object).ravel())[0] if isinstance(x, np.ndarray) else list(x)[0]
    return x


def assert_number(
    x: Any,
    name: str,
    lower: float = -math.inf,
    upper: float = math.inf,
    finite: bool = False,
    none_ok: bool = False,
) -> None:
    """checkmate::assert_number"""
    cm.cm_assert(cm.check_number(x, lower=lower, upper=upper, finite=finite, null_ok=none_ok), name)


def assert_count(x: Any, name: str, positive: bool = False) -> None:
    """checkmate::assert_count"""
    cm.cm_assert(cm.check_count(x, positive=positive), name)


def assert_int(x: Any, name: str, none_ok: bool = False) -> None:
    """checkmate::assert_int"""
    cm.cm_assert(cm.check_int(x, null_ok=none_ok), name)


def as_number_list(x: Any, name: str, kind: str) -> List[float]:
    """A scalar or a one-dimensional sequence of numbers, as floats."""
    v = cm.Vec(x)
    if not (v.numeric() or v.type == "missing") or v.type == "NULL":
        cm.cm_assert(cm.type_msg(v, kind), name)
    return [math.nan if cm._is_na(e) else float(e) for e in v.values]


def assert_integerish(
    x: Any,
    name: str,
    lower: float = -math.inf,
    upper: float = math.inf,
    min_len: Optional[int] = None,
    max_len: Optional[float] = None,
    unique: bool = False,
    none_ok: bool = False,
) -> Optional[List[int]]:
    """checkmate::assert_integerish(any.missing = FALSE, ...), as r5r calls
    it; returns the values as ints (or None)."""
    cm.cm_assert(
        cm.check_integerish(x, lower=lower, upper=upper, any_missing=False, min_len=min_len,
                            max_len=max_len, unique=unique, null_ok=none_ok),
        name,
    )
    if x is None:
        return None
    return [int(round(float(v))) for v in cm.Vec(x).values]


def assert_flag(x: Any, name: str) -> None:
    """checkmate::assert_logical(x, len = 1, any.missing = FALSE)"""
    cm.cm_assert(cm.check_logical(x, len_=1, any_missing=False), name)


def assert_string(x: Any, name: str, none_ok: bool = False) -> None:
    """checkmate::assert_string"""
    cm.cm_assert(cm.check_string(x, null_ok=none_ok), name)


def as_strings(x: Any, name: str) -> List[str]:
    """checkmate::assert_character(x, min.len = 1, any.missing = FALSE): a
    string or a sequence of strings, as a list."""
    cm.cm_assert(cm.check_character(x, min_len=1, any_missing=False), name)
    return [str(v) for v in cm.Vec(x).values]


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
