"""peabiru: multimodal routing on OpenStreetMap and GTFS networks.

[`build_network()`][peabiru.build_network] builds a network from an
OpenStreetMap extract and GTFS feeds; [`travel_time_matrix()`][peabiru.travel_time_matrix],
[`accessibility()`][peabiru.accessibility], [`expanded_travel_time_matrix()`][peabiru.expanded_travel_time_matrix]
and [`arrival_travel_time_matrix()`][peabiru.arrival_travel_time_matrix] compute travel times
by walking, cycling, driving and public transport. The routing engine is
written in Rust; no Java is needed. Options that peabiru does not support yet
raise [`UnsupportedError`][peabiru.UnsupportedError]; they are never silently ignored.

The functions have the names, arguments and defaults of the r5r package
(2.5.0), and the engine is a port of R5 7.6, the engine r5r drives, so the
results are the same.
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
