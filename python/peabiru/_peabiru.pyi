"""Type stubs of the compiled extension (internal API)."""

from typing import Any, Dict, List, Optional, Tuple

# (directory, origin ids, destination ids or, for accessibility, the
# opportunity names): write the CSV files there instead of returning
Output = Optional[Tuple[str, List[str], List[str]]]
# the scenario of a call (peabiru._scenario.scenario_spec), None for none
Scenario = Optional[Dict[str, Any]]

__version__: str
NA_I32: int

class Network:
    def info(self) -> Dict[str, Any]: ...
    def transit_availability(self, date: int) -> Tuple[int, int]: ...
    def build_warnings(self) -> List[str]: ...

def core_version() -> str: ...
def check_output_ids(ids: List[str]) -> None: ...
def build_network(data_path: str, n_threads: int) -> Network: ...
def street_ttm(
    network: Network, olat: bytes, olon: bytes, dlat: bytes, dlon: bytes,
    mode: str, walk_speed: float, bike_speed: float, max_lts: int,
    max_trip_duration: int, n_threads: int, percentiles: List[int],
    scenario: Scenario, output: Output,
) -> Tuple[bytes, bytes, bytes]: ...
def transit_ttm(
    network: Network, olat: bytes, olon: bytes, dlat: bytes, dlon: bytes,
    transit_modes: str, access_mode: str, egress_mode: str, bike_speed: float,
    max_lts: int, max_bike_time: int, max_car_time: int,
    date: int, departure_time: int, time_window: int, percentiles: List[int],
    max_walk_time: int, max_trip_duration: int, walk_speed: float,
    max_rides: int, draws_per_minute: int, seed: Optional[int], n_threads: int,
    scenario: Scenario, output: Output, fare: Optional[Dict[str, Any]], max_fare: float,
) -> Tuple[bytes, bytes, List[bytes]]: ...
def accessibility(
    network: Network, olat: bytes, olon: bytes, dlat: bytes, dlon: bytes,
    opportunities: List[bytes], mode: str, transit_modes: str, egress_mode: str,
    decay_function: str, decay_value: float, cutoffs: List[int],
    percentiles: List[int], date: int, departure_time: int, time_window: int,
    max_walk_time: int, max_bike_time: int, max_car_time: int,
    max_trip_duration: int, walk_speed: float, bike_speed: float,
    max_lts: int, max_rides: int, draws_per_minute: int, seed: Optional[int],
    n_threads: int, scenario: Scenario, output: Output, fare: Optional[Dict[str, Any]],
    max_fare: float,
) -> bytes: ...
def expanded(
    network: Network, olat: bytes, olon: bytes, dlat: bytes, dlon: bytes,
    transit_modes: str, access_mode: str, egress_mode: str, bike_speed: float,
    max_lts: int, max_bike_time: int, max_car_time: int,
    arrival: bool, date: int, departure_time: int, time_window: int,
    max_walk_time: int, max_trip_duration: int, walk_speed: float,
    max_rides: int, draws_per_minute: int, seed: Optional[int], n_threads: int,
    breakdown: bool, scenario: Scenario, output: Output,
) -> Dict[str, Any]: ...
def isochrone_polygons(
    network: Network, olat: bytes, olon: bytes, mode: str, transit_modes: str,
    egress_mode: str, cutoffs: List[float], zoom: int, percentiles: List[int],
    date: int, departure_time: int, time_window: int, max_walk_time: int,
    max_bike_time: int, max_car_time: int, max_trip_duration: int,
    walk_speed: float, bike_speed: float, max_lts: int, max_rides: int,
    draws_per_minute: int, seed: Optional[int], n_threads: int,
) -> Dict[str, bytes]: ...
def street_edges(network: Network) -> Dict[str, Any]: ...
def isochrone_lines(
    n_origins: int, n_edges: int, minutes: bytes, cutoffs: List[float],
) -> Tuple[bytes, bytes, bytes, bytes]: ...
def itineraries(
    network: Network, olat: bytes, olon: bytes, dlat: bytes, dlon: bytes, mode: str,
    transit_modes: str, egress_mode: str, date: int, departure_time: int, time_window: int,
    max_walk_time: int, max_bike_time: int, max_car_time: int, max_trip_duration: int,
    walk_speed: float, bike_speed: float, max_rides: int, max_lts: int, shortest_path: bool,
    n_threads: int, scenario: Scenario, drop_geometry: bool, osm_link_ids: bool,
    output: Optional[Tuple[str, List[str], List[str]]],
    suboptimal_minutes: int = 0, fare: Optional[Dict[str, Any]] = None,
    max_fare: float = ...,
) -> Dict[str, Any]: ...
def trip_options_py(
    network: Network, olat: bytes, olon: bytes, dlat: bytes, dlon: bytes, mode: str,
    transit_modes: str, egress_mode: str, date: int, departure_time: int, time_window: int,
    criteria: List[str], max_walk_time: int, max_bike_time: int, max_car_time: int,
    max_trip_duration: int, walk_speed: float, bike_speed: float, max_rides: int,
    max_lts: int, n_threads: int, scenario: Scenario, fare: Optional[Dict[str, Any]],
    max_fare: float, max_options: int, itineraries: bool, drop_geometry: bool,
    osm_link_ids: bool, output: Optional[Tuple[str, List[str], List[str]]],
    walk_resolution: int = 60,
) -> Optional[Dict[str, Any]]: ...
def fare_setup(network: Network, base_fare: float, by: str) -> Dict[str, Any]: ...
def fare_read(bytes: bytes, encoding: str) -> Dict[str, Any]: ...
def fare_write(fare: Dict[str, Any]) -> bytes: ...
def fare_check(network: Network, fare: Dict[str, Any]) -> None: ...
def pareto(
    network: Network, olat: bytes, olon: bytes, dlat: bytes, dlon: bytes, mode: str,
    transit_modes: str, egress_mode: str, date: int, departure_time: int, time_window: int,
    percentiles: List[int], max_walk_time: int, max_bike_time: int, max_car_time: int,
    max_trip_duration: int, walk_speed: float, bike_speed: float, max_lts: int,
    max_rides: int, seed: Optional[int], n_threads: int, fare: Dict[str, Any],
    fare_cutoffs: List[float],
) -> Dict[str, bytes]: ...
def find_snap(
    network: Network, lat: bytes, lon: bytes, radius: float, mode: str,
) -> Dict[str, bytes]: ...
def street_bbox(network: Network) -> List[float]: ...
def street_vertices(network: Network) -> Dict[str, bytes]: ...
def transit_stops(network: Network) -> Dict[str, Any]: ...
def transit_routes(network: Network) -> Dict[str, Any]: ...
def transit_service_dates(network: Network) -> List[int]: ...
def feed_paths(network: Network) -> List[str]: ...
def gtfs_files(dir: str) -> List[str]: ...
def gtfs_errors(paths: List[str]) -> Dict[str, Any]: ...
