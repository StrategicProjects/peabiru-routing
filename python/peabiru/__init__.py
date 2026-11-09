"""peabiru: multimodal routing on OpenStreetMap and GTFS networks.

The routing functions of the r5r package -- [`build_network()`][peabiru.build_network],
[`travel_time_matrix()`][peabiru.travel_time_matrix], [`accessibility()`][peabiru.accessibility],
[`expanded_travel_time_matrix()`][peabiru.expanded_travel_time_matrix] and [`arrival_travel_time_matrix()`][peabiru.arrival_travel_time_matrix]
-- with the same arguments, defaults and results, computed by a Rust port of
the R5 routing engine that r5r 2.5.0 drives (R5 7.6). No Java is needed.
Options that peabiru does not support yet raise [`UnsupportedError`][peabiru.UnsupportedError];
they are never silently ignored.
"""

from ._checks import UnsupportedError
from ._data import sample_data_path, sample_points
from ._network import Network, build_network
from ._peabiru import __version__
from ._peabiru import core_version as _core_version
from ._routing import (
    accessibility,
    arrival_travel_time_matrix,
    expanded_travel_time_matrix,
    travel_time_matrix,
)


def core_version() -> str:
    """Version of the Rust routing core.

    Returns
    -------
    str
        The version of the ``peabiru-core`` crate this package was built
        against.

    Examples
    --------
    >>> import peabiru
    >>> peabiru.core_version() == peabiru.__version__
    True
    """
    return _core_version()


__all__ = [
    "Network",
    "UnsupportedError",
    "__version__",
    "accessibility",
    "arrival_travel_time_matrix",
    "build_network",
    "core_version",
    "expanded_travel_time_matrix",
    "sample_data_path",
    "sample_points",
    "travel_time_matrix",
]
