"""A port of the R package's port of checkmate (r/R/checkmate.R), so that
Python accepts and rejects what r5r accepts and rejects, with the same
messages in Python's terms.

Values are read as R vectors: ``None`` is ``NULL``; a bool, int, float or
str is a vector of one; a list, tuple, numpy array or pandas Series is a
vector of its elements (``None`` and NaN are missing values; an empty one
passes every type check, as an empty vector does in R); a mix of kinds is a
list. Types are named in Python's terms (an empty list is
R's ``numeric(0)``): ``bool`` for R's logical, ``str``
for character, ``int`` and ``float`` for integer and double, ``None`` for
NULL, ``datetime`` for POSIXct (see bench/compare/argument_errors.json,
whose "python" rules turn R's messages into these). Positions in messages
count from 1, as R's.
"""

from __future__ import annotations

import datetime as _dt
import math
import numbers
from typing import Any, List, Optional, Sequence, Tuple, Union

import numpy as np

Res = Union[bool, str]

TOL = math.sqrt(np.finfo(float).eps)
INT_MIN_EXCL = -2147483648.0
INT_MAX = 2147483647.0

# R type -> the name messages give
_NAMES = {
    "NULL": "None",
    "logical": "bool",
    "integer": "int",
    "double": "float",
    "character": "str",
    "list": "list",
    "POSIXct": "datetime",
    "Date": "date",
}


def _is_na(v: Any) -> bool:
    if v is None:
        return True
    if isinstance(v, (float, np.floating)):
        return math.isnan(v)
    try:
        import pandas as pd

        return v is pd.NA or v is pd.NaT
    except ImportError:  # pragma: no cover
        return False


def _kind(v: Any) -> Optional[str]:
    """The R type of one element; None for a missing value."""
    if _is_na(v):
        return None
    if isinstance(v, (bool, np.bool_)):
        return "logical"
    if isinstance(v, (numbers.Integral, np.integer)):
        return "integer"
    if isinstance(v, (numbers.Real, np.floating)):
        return "double"
    if isinstance(v, str):
        return "character"
    if isinstance(v, _dt.datetime):
        return "POSIXct"
    if isinstance(v, _dt.date):
        return "Date"
    return "other"


class Vec:
    """A value as an R vector: its type and elements."""

    def __init__(self, x: Any):
        self.x = x
        self.cls: Optional[str] = None  # an object that is not a vector
        if x is None:
            self.type, self.values = "NULL", []
            return
        if isinstance(x, (str, bool, np.bool_, numbers.Number, _dt.date)):
            values = [x]
        elif isinstance(x, np.ndarray) and x.dtype.kind in "US" and x.size == 0:
            self.type, self.values = "character", []  # R's character(0)
            return
        elif isinstance(x, (list, tuple, np.ndarray)) or type(x).__name__ == "Series":
            values = list(np.asarray(x, dtype=object).ravel()) if isinstance(x, np.ndarray) else list(x)
        else:
            self.type, self.values = "other", [x]
            self.cls = type(x).__name__
            return
        self.values = values
        kinds = {_kind(v) for v in values} - {None}
        if not kinds:
            self.type = "missing"  # R's NA or an empty vector: any type
        elif kinds <= {"integer"}:
            self.type = "integer"
        elif kinds <= {"integer", "double"}:
            self.type = "double"
        elif len(kinds) == 1 and "other" not in kinds:
            self.type = kinds.pop()
        else:
            self.type = "list"

    def __len__(self) -> int:
        return len(self.values)

    def numeric(self) -> bool:
        # an empty list is R's numeric(0)
        return self.type in ("integer", "double") or (self.type == "missing" and not self.values)

    def na(self) -> List[bool]:
        return [_is_na(v) for v in self.values]

    def guess(self) -> str:
        """checkmate's guess_type(), in Python's terms."""
        if self.cls is not None:
            return self.cls
        if self.type == "missing":
            # R's NA is logical, NaN double; an empty list is numeric(0)
            nan = any(isinstance(v, (float, np.floating)) for v in self.values)
            return "float" if nan or not self.values else "bool"
        if self.type == "list" and not isinstance(self.x, (list, tuple, np.ndarray)):
            return type(self.x).__name__
        return _NAMES.get(self.type, self.type)


def _g(x: float) -> str:
    """R's %g of a bound."""
    if math.isinf(x):
        return "Inf" if x > 0 else "-Inf"
    s = "%g" % x
    return s


def type_msg(v: Vec, expected: str, null_ok: bool = False) -> str:
    return "Must be of type '%s'%s, not '%s'" % (
        expected,
        " (or 'None')" if null_ok else "",
        v.guess(),
    )


def integerish(v: Vec) -> Tuple[str, int]:
    """checkIntegerish(): "ok", or the failure and its position (1-based)."""
    if v.type == "integer":
        return "ok", 0
    if v.type == "logical":
        return "type", 0
    if v.type == "missing":
        return "ok", 0
    if v.type != "double":
        return "type", 0
    for i, x in enumerate(v.values, 1):
        if _is_na(x):
            continue
        x = float(x)
        if x <= INT_MIN_EXCL or x > INT_MAX:
            return "range", i
        if abs(x - round(x)) > TOL:
            return "tol", i
    return "ok", 0


def typed_missing(v: Vec) -> bool:
    return v.type == "missing"


def scalar_na(v: Vec) -> bool:
    return len(v) == 1 and v.type == "missing"


def check_len(v: Vec, len_: Optional[int] = None, min_len: Optional[float] = None,
              max_len: Optional[float] = None) -> Res:
    n = len(v)
    if len_ is not None and n != len_:
        return "Must have length %s, but has length %d" % (_g(len_), n)
    if min_len is not None and n < min_len:
        return "Must have length >= %s, but has length %d" % (_g(min_len), n)
    if max_len is not None and n > max_len:
        return "Must have length <= %s, but has length %d" % (_g(max_len), n)
    return True


def check_missings(v: Vec, any_missing: bool = True, all_missing: bool = True) -> Res:
    na = v.na()
    if not any_missing and any(na):
        return "Contains missing values (element %d)" % (na.index(True) + 1)
    if not all_missing and v.type != "list" and all(na):
        return "Contains only missing values"
    return True


def check_bounds(v: Vec, lower: float = -math.inf, upper: float = math.inf) -> Res:
    if not v.numeric():
        return True
    xs = [(i, float(x)) for i, x in enumerate(v.values, 1) if not _is_na(x)]
    if math.isfinite(lower) or lower == math.inf:
        for i, x in xs:
            if x < lower:
                return "Element %d is not >= %s" % (i, _g(lower))
    if math.isfinite(upper) or upper == -math.inf:
        for i, x in xs:
            if x > upper:
                return "Element %d is not <= %s" % (i, _g(upper))
    return True


def check_unique(v: Vec, unique: bool = False) -> Res:
    if unique:
        seen: list = []
        for i, x in enumerate(v.values, 1):
            key = "NA" if _is_na(x) else x
            if key in seen:
                return "Contains duplicated values, position %d" % i
            seen.append(key)
    return True


def vector_checks(v: Vec, len_=None, min_len=None, max_len=None, any_missing=True,
                  all_missing=True, lower=-math.inf, upper=math.inf, finite=False,
                  unique=False) -> Res:
    for r in (check_len(v, len_, min_len, max_len), check_missings(v, any_missing, all_missing),
              check_bounds(v, lower, upper)):
        if r is not True:
            return r
    if finite and v.numeric() and any(math.isinf(float(x)) for x in v.values if not _is_na(x)):
        return "Must be finite"
    return check_unique(v, unique)


def check_numeric(x: Any, lower=-math.inf, upper=math.inf, finite=False, any_missing=True,
                  all_missing=True, len_=None, min_len=None, max_len=None, unique=False,
                  null_ok=False) -> Res:
    v = Vec(x)
    if v.type == "NULL":
        return True if null_ok else "Must be of type 'numeric', not 'None'"
    if not (v.numeric() or typed_missing(v)):
        return type_msg(v, "numeric", null_ok)
    return vector_checks(v, len_, min_len, max_len, any_missing, all_missing, lower, upper,
                         finite, unique)


def check_integerish(x: Any, lower=-math.inf, upper=math.inf, any_missing=True,
                     all_missing=True, len_=None, min_len=None, max_len=None, unique=False,
                     null_ok=False) -> Res:
    v = Vec(x)
    if v.type == "NULL":
        return True if null_ok else "Must be of type 'integerish', not 'None'"
    err, pos = integerish(v)
    if err == "type" and not typed_missing(v):
        return type_msg(v, "integerish", null_ok)
    if err == "range":
        return "Must be of type 'integerish', but element %d is not in integer range" % pos
    if err == "tol":
        return "Must be of type 'integerish', but element %d is not close to an integer" % pos
    return vector_checks(v, len_, min_len, max_len, any_missing, all_missing, lower, upper,
                         False, unique)


def check_character(x: Any, any_missing=True, all_missing=True, len_=None, min_len=None,
                    max_len=None, unique=False, null_ok=False) -> Res:
    v = Vec(x)
    if v.type == "NULL":
        return True if null_ok else "Must be of type 'str', not 'None'"
    if not (v.type == "character" or typed_missing(v)):
        return type_msg(v, "str", null_ok)
    return vector_checks(v, len_, min_len, max_len, any_missing, all_missing, unique=unique)


def check_logical(x: Any, any_missing=True, all_missing=True, len_=None, min_len=None,
                  max_len=None, null_ok=False) -> Res:
    v = Vec(x)
    if v.type == "NULL":
        return True if null_ok else "Must be of type 'bool', not 'None'"
    if not (v.type == "logical" or typed_missing(v)):
        return type_msg(v, "bool", null_ok)
    return vector_checks(v, len_, min_len, max_len, any_missing, all_missing)


def check_datetime(x: Any, any_missing=True, all_missing=True, len_=None, min_len=None,
                   max_len=None, null_ok=False) -> Res:
    """check_posixct: datetime.datetime values (pandas Timestamps too)."""
    v = Vec(x)
    if v.type == "NULL":
        return True if null_ok else "Must be of type 'datetime', not 'None'"
    if v.type != "POSIXct":
        return type_msg(v, "datetime", null_ok)
    return vector_checks(v, len_, min_len, max_len, any_missing, all_missing)


def scalar_head(v: Vec, expected: str, ok_type: bool, na_ok: bool, null_ok: bool):
    if scalar_na(v):
        return None if na_ok else "May not be NA"
    if v.type == "NULL":
        return None if null_ok else "Must be of type '%s', not 'None'" % expected
    if not ok_type:
        return type_msg(v, expected, null_ok)
    if len(v) != 1:
        return "Must have length 1"
    return True


def check_count(x: Any, na_ok=False, positive=False, null_ok=False) -> Res:
    v = Vec(x)
    h = scalar_head(v, "count", integerish(v)[0] == "ok", na_ok, null_ok)
    if h is not True:
        return True if h is None else h
    if int(v.values[0]) < int(positive):
        return "Must be >= %d" % int(positive)
    return True


def check_int(x: Any, na_ok=False, lower=-math.inf, upper=math.inf, null_ok=False) -> Res:
    v = Vec(x)
    h = scalar_head(v, "single integerish value", integerish(v)[0] == "ok", na_ok, null_ok)
    if h is not True:
        return True if h is None else h
    return check_bounds(v, lower, upper)


def check_number(x: Any, na_ok=False, lower=-math.inf, upper=math.inf, finite=False,
                 null_ok=False) -> Res:
    v = Vec(x)
    h = scalar_head(v, "number", v.numeric(), na_ok, null_ok)
    if h is not True:
        return True if h is None else h
    if finite and math.isinf(float(v.values[0])):
        return "Must be finite"
    return check_bounds(v, lower, upper)


def check_string(x: Any, na_ok=False, null_ok=False) -> Res:
    v = Vec(x)
    h = scalar_head(v, "string", v.type == "character", na_ok, null_ok)
    return True if h is None else h


def check_flag(x: Any, na_ok=False, null_ok=False) -> Res:
    v = Vec(x)
    h = scalar_head(v, "bool flag", v.type == "logical", na_ok, null_ok)
    return True if h is None else h


def set_collapse(values: Sequence[Any]) -> str:
    if len(values) == 0:
        return "{}"
    seen: list = []
    for v in values:
        s = text(v)
        if s not in seen:
            seen.append(s)
    return "{'%s'}" % "','".join(seen)


def text(v: Any) -> str:
    """as.character() of one element, in Python's terms."""
    if _is_na(v):
        return "NA" if v is None or not isinstance(v, float) else "NaN"
    if isinstance(v, (bool, np.bool_)):
        return str(bool(v))
    if isinstance(v, (float, np.floating)) and float(v).is_integer() and abs(v) < 1e15:
        return str(int(v))
    return str(v)


def _cap(s: str) -> str:
    return s[:1].upper() + s[1:]


def set_msg(msg: str, what: Optional[str], *args: Any) -> str:
    return _cap(msg % args) if what is None else _cap(what) + " " + (msg % args)


def subset_internal(v: Vec, choices: Sequence[str], what: Optional[str] = None) -> Res:
    if len(choices) == 0:
        if len(v) == 0:
            return True
        return set_msg("must be a subset of the empty set, i.e. also empty", what)
    if v.type != "NULL":
        if v.type not in ("character", "missing"):
            return set_msg("must be a subset of %s, but has different type", what,
                           set_collapse(choices))
        extra = [x for x in v.values if _is_na(x) or x not in choices]
        if extra:
            return set_msg("must be a subset of %s, but has additional elements %s", what,
                           set_collapse(choices), set_collapse(extra))
    return True


def check_names(x: Any, subset_of: Optional[Sequence[str]] = None,
                must_include: Optional[Sequence[str]] = None, what: str = "names",
                type_: str = "named") -> Res:
    v = Vec(x)
    if v.type not in ("character", "NULL") and not (v.type == "missing" and not v.values):
        return "Must be a character vector"
    if v.type == "NULL":
        return "Must have %s" % what
    na = v.na()
    if any(na):
        return "Must have %s, but is NA at position %d" % (what, na.index(True) + 1)
    for i, s in enumerate(v.values, 1):
        if len(s) < 1:
            return "Must have %s, but element %d is empty" % (what, i)
    if type_ == "unique":
        r = check_unique(v, True)
        if r is not True:
            pos = int(r.rsplit(" ", 1)[1])
            return "Must have unique %s, but element %d is duplicated" % (what, pos)
    if subset_of is not None:
        r = subset_internal(v, subset_of, what)
        if r is not True:
            return r
    if must_include is not None:
        missing = [m for m in must_include if m not in v.values]
        if missing:
            return set_msg("must include the elements %s, but is missing elements %s", what,
                           set_collapse(must_include), set_collapse(missing))
    return True


def check_subset(x: Any, choices: Sequence[str], empty_ok: bool = True) -> Res:
    v = Vec(x)
    if len(v) == 0:
        if not empty_ok:
            return "Must be a subset of %s, not empty" % set_collapse(choices)
        return True
    return subset_internal(v, choices)


def check_choice(x: Any, choices: Sequence[str], null_ok: bool = False) -> Res:
    v = Vec(x)
    if v.type == "NULL":
        return True if null_ok else "Must be a subset of %s, not 'None'" % set_collapse(choices)
    if v.type in ("list", "other") or len(v) != 1:
        return "Must be element of set %s, but is not atomic scalar" % set_collapse(choices)
    if v.type not in ("character", "missing"):
        return "Must be element of set %s, but types do not match (%s != str)" % (
            set_collapse(choices), v.guess())
    if v.values[0] not in choices:
        return "Must be element of set %s, but is '%s'" % (set_collapse(choices), text(v.values[0]))
    return True


def to_upper_unique(x: Any) -> Any:
    """r5r's toupper(unique(x)) on a mode: strings upper-cased, repeats
    dropped, None an empty list (R's character(0)); anything else as it is
    (R would turn numbers and logicals into text: Python reports their
    type)."""
    if x is None:
        return np.array([], dtype=str)
    v = Vec(x)
    if v.type != "character" and not (v.type == "missing" and not v.values):
        return x
    if not v.values:
        return np.array([], dtype=str)
    out: list = []
    for s in v.values:
        if s not in out:
            out.append(s)
    return [s.upper() for s in out]


def cm_assert(res: Res, name: str, type_error: Optional[bool] = None) -> None:
    """checkmate's error: ``Assertion on '<name>' failed: <message>.``, a
    TypeError when the message is about the type, else a ValueError."""
    if res is True:
        return
    msg = "Assertion on '%s' failed: %s." % (name, res)
    is_type = type_error if type_error is not None else (
        str(res).startswith("Must be of type")
        or str(res).startswith("Must be a character vector")
    )
    raise (TypeError if is_type else ValueError)(msg)


def cm_assert_and(name: str, *checks) -> None:
    """checkmate::assert(..., combine = "and"): the first failing check;
    each check a result or a callable giving one."""
    for c in checks:
        cm_assert(c() if callable(c) else c, name)


def r_num(x: Any) -> str:
    """A number as R prints it (as.character()): up to 15 significant
    digits, in scientific notation when that is shorter (1e+10, 1e-04)."""
    x = float(x)
    if math.isnan(x):
        return "NaN"
    if math.isinf(x):
        return "Inf" if x > 0 else "-Inf"
    if x == 0:
        return "0"
    m, _, e = ("%.14e" % x).partition("e")
    m = m.rstrip("0").rstrip(".")
    exp = int(e)
    sci = "%se%s%02d" % (m, "-" if exp < 0 else "+", abs(exp))
    digits = len(m.replace("-", "").replace(".", ""))
    fixed = ("%.*f" % (max(0, digits - 1 - exp), x)).rstrip("0").rstrip(".") if "." in ("%.*f" % (max(0, digits - 1 - exp), x)) else "%.*f" % (max(0, digits - 1 - exp), x)
    return sci if len(fixed) > len(sci) else fixed
