"""Type stubs of the compiled extension (internal API)."""

from typing import Any, Dict, List, Optional, Tuple

__version__: str
NA_I32: int

class Network:
    def info(self) -> Dict[str, Any]: ...
    def transit_availability(self, date: int) -> Tuple[int, int]: ...
    def r5r_warnings(self) -> List[str]: ...

def core_version() -> str: ...
def build_network(data_path: str, n_threads: int) -> Network: ...
def street_ttm(
    network: Network, olat: bytes, olon: bytes, dlat: bytes, dlon: bytes,
    mode: str, walk_speed: float, bike_speed: float, max_lts: int,
    max_trip_duration: int, n_threads: int,
) -> Tuple[bytes, bytes, bytes]: ...
def transit_ttm(
    network: Network, olat: bytes, olon: bytes, dlat: bytes, dlon: bytes,
    date: int, departure_time: int, time_window: int, percentiles: List[int],
    max_walk_time: int, max_trip_duration: int, walk_speed: float,
    max_rides: int, draws_per_minute: int, seed: Optional[int], n_threads: int,
) -> Tuple[bytes, bytes, List[bytes]]: ...
def accessibility(
    network: Network, olat: bytes, olon: bytes, dlat: bytes, dlon: bytes,
    opportunities: List[bytes], mode: str, decay_function: str,
    decay_value: float, cutoffs: List[int], percentiles: List[int], date: int,
    departure_time: int, time_window: int, max_walk_time: int,
    max_trip_duration: int, walk_speed: float, bike_speed: float,
    max_lts: int, max_rides: int, draws_per_minute: int, seed: Optional[int],
    n_threads: int,
) -> bytes: ...
def expanded(
    network: Network, olat: bytes, olon: bytes, dlat: bytes, dlon: bytes,
    arrival: bool, date: int, departure_time: int, time_window: int,
    max_walk_time: int, max_trip_duration: int, walk_speed: float,
    max_rides: int, draws_per_minute: int, seed: Optional[int], n_threads: int,
) -> Dict[str, Any]: ...
def isochrone_polygons(
    network: Network, olat: bytes, olon: bytes, mode: str, cutoffs: List[float],
    zoom: int, percentiles: List[int], date: int, departure_time: int,
    time_window: int, max_walk_time: int, max_trip_duration: int,
    walk_speed: float, bike_speed: float, max_lts: int, max_rides: int,
    draws_per_minute: int, seed: Optional[int], n_threads: int,
) -> Dict[str, bytes]: ...
def street_edges(network: Network) -> Dict[str, Any]: ...
def isochrone_lines(
    n_origins: int, n_edges: int, minutes: bytes, cutoffs: List[float],
) -> Tuple[bytes, bytes, bytes, bytes]: ...
