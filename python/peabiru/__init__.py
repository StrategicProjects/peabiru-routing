"""peabiru: multimodal routing on OpenStreetMap and GTFS networks.

[`build_network()`][peabiru.build_network] builds a network from an
OpenStreetMap extract and GTFS feeds; [`travel_time_matrix()`][peabiru.travel_time_matrix],
[`accessibility()`][peabiru.accessibility], [`expanded_travel_time_matrix()`][peabiru.expanded_travel_time_matrix],
[`arrival_travel_time_matrix()`][peabiru.arrival_travel_time_matrix] and
[`isochrone()`][peabiru.isochrone] compute travel times
by walking, cycling, driving and public transport;
[`find_snap()`][peabiru.find_snap], [`street_network_to_sf()`][peabiru.street_network_to_sf],
[`transit_network_to_sf()`][peabiru.transit_network_to_sf],
[`check_transit_availability()`][peabiru.check_transit_availability] and
[`get_gtfs_errors()`][peabiru.get_gtfs_errors] inspect a network. The routing engine is
written in Rust; no Java is needed. Options that peabiru does not support yet
raise [`UnsupportedError`][peabiru.UnsupportedError]; they are never silently ignored.

The functions have the names, arguments and defaults of the r5r package
(2.5.0), and the engine is a port of R5 7.6, the engine r5r drives, so the
results are the same.
"""

from ._checks import UnsupportedError
from ._data import sample_data_path, sample_points
from ._isochrone import isochrone
from ._network import Network, build_network
from ._network_tools import (
    check_transit_availability,
    find_snap,
    get_gtfs_errors,
    street_network_bbox,
    street_network_to_sf,
    transit_network_to_sf,
)
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
    "check_transit_availability",
    "core_version",
    "expanded_travel_time_matrix",
    "find_snap",
    "get_gtfs_errors",
    "isochrone",
    "sample_data_path",
    "sample_points",
    "street_network_bbox",
    "street_network_to_sf",
    "transit_network_to_sf",
    "travel_time_matrix",
]
