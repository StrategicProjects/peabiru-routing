"""Building a transport network (r5r ``build_network()``)."""

from __future__ import annotations

import math
import os
import re
import sys
import time
import warnings
from typing import Any, Dict, Optional, Union

from . import _peabiru
from ._assign import assign_n_threads
from ._checks import assert_flag, assert_string, fail_assert

PathLike = Union[str, "os.PathLike[str]"]


class Network:
    """A transport network built by [`build_network()`][peabiru.build_network].

    It holds the street network and, when the data directory had GTFS
    feeds, the public transport layer, in memory. Pass it to the routing
    functions. It cannot be pickled: build it again in each process (it
    takes seconds even for a whole city).

    Attributes
    ----------
    data_path : str
        The directory the network was built from.
    elevation : str
        The slope cost function in use: ``"TOBLER"``, ``"MINETTI"`` or
        ``"NONE"`` (also when the directory has no ``.tif`` raster).
    has_tif : bool
        Whether the directory has a ``.tif`` raster (which, as in r5r,
        disables the swap of origins and destinations for walking).
    """

    def __init__(self, core: Any, data_path: str, elevation: str, has_tif: bool) -> None:
        self._core = core
        self.data_path = data_path
        self.elevation = elevation
        self.has_tif = has_tif

    def info(self) -> Dict[str, Any]:
        """Sizes of the network.

        Returns
        -------
        dict
            ``vertices`` and ``edges`` of the street network and its
            ``elevation`` model (``"TOBLER"``, ``"MINETTI"`` or ``"NONE"``); ``feeds`` (the
            GTFS feed ids), ``stops``, ``patterns``, ``services`` and
            ``frequencies`` (whether any trip is frequency-based) of the
            public transport layer.
        """
        return dict(self._core.info())

    def __repr__(self) -> str:
        i = self.info()
        lines = [
            "<peabiru.Network>",
            f"  data path: {self.data_path}",
            f"  streets:   {i['vertices']:,} vertices, {i['edges']:,} edges",
        ]
        if not i["feeds"]:
            lines.append("  transit:   none (no GTFS feed)")
        else:
            freq = ", frequency-based trips" if i["frequencies"] else ""
            lines.append(
                f"  transit:   {len(i['feeds'])} feed(s) ({', '.join(i['feeds'])}), "
                f"{i['stops']:,} stops, {i['patterns']:,} patterns, "
                f"{i['services']:,} services{freq}"
            )
        lines.append(f"  elevation: {self.elevation}")
        return "\n".join(lines)

    def __reduce__(self) -> Any:
        raise TypeError(
            "a peabiru Network cannot be pickled; build it again with build_network()"
        )


def assert_network(x: Any) -> None:
    if not isinstance(x, Network):
        raise TypeError(
            "Assertion on 'r5r_network' failed: Must be a 'peabiru.Network' "
            "(see build_network())."
        )


def build_network(
    data_path: PathLike,
    verbose: bool = False,
    temp_dir: bool = False,
    elevation: str = "TOBLER",
    overwrite: bool = False,
    n_threads: Optional[Union[int, float]] = math.inf,
) -> Network:
    """Build a transport network.

    Builds a multimodal transport network from a directory with one
    OpenStreetMap extract (``.pbf``) and zero or more GTFS feeds (``.zip``),
    as ``r5r::build_network()`` does. The network lives in memory and is
    reused by every routing function; nothing is written to ``data_path``,
    and there is no ``network.dat`` cache.

    As in r5r, the files are recognised by their extension in lower case
    only: a ``map.PBF`` or a ``feed.ZIP`` is ignored.

    As in r5r, a ``.tif`` raster in ``data_path`` gives walking and cycling
    times slope costs (``elevation="TOBLER"``, the default, or
    ``"MINETTI"``; never for cars), identical to r5r's: the raster is
    sampled every 10 m along each street at the nearest pixel, and each
    street's time is multiplied by the average cost of its slopes. Points
    outside the raster count as 0 m high, as in r5r. Without a ``.tif``
    raster the network is built with ``elevation="NONE"``.

    The raster must be a GeoTIFF with one band in WGS 84 longitude and
    latitude (EPSG:4326) or another coordinate reference system whose
    transform peabiru reproduces exactly: SIRGAS 2000 (4674), SAD69 (4618),
    Corrego Alegre 1970-72 (4225), their UTM zones used in Brazil
    (31965-31985, 6210, 6211, 5396; 29168-29172, 29187-29195; 22521-22525),
    SIRGAS 2000 / Brazil Polyconic (5880), WGS 84 / UTM (32601-32660,
    32701-32760) and WGS 84 / Pseudo-Mercator (3857), given by its EPSG code
    (what GDAL writes), north up, uncompressed or compressed with LZW,
    Deflate or PackBits (what GDAL writes by default); other rasters are an
    error that says what to change, and so are a ``.prj`` file next to it
    (r5r would use its system instead), a raster whose name ends in
    ``.tiff`` (r5r fails to read it) and more than one raster (r5r reads the
    last one the file system lists).
    In a system other than EPSG:4326, r5r's own result depends on the
    machine it runs on for a street point within nanometres (5e-8 m, or
    2e-13 degrees) of the boundary between two pixels of different values,
    or of the raster's edge: the last bits of Java's sines and cosines
    decide which pixel it reads. peabiru reads the pixel Java's
    ``StrictMath`` gives, the same on every machine, and warns once
    (``UserWarning``) with the number of such points (about one in a hundred
    million for 30 m pixels; reprojecting the raster to EPSG:4326 removes
    the ambiguity). A point that ``StrictMath`` itself puts on the raster's
    outer edge (inside its envelope, past the last pixel) remains an error,
    since there r5r fails on every machine and there is no pixel to read.
    Voids and values beyond +/-3276.7 m are
    stored wrapped around as r5r stores them (R5 keeps decimetres in 16-bit
    integers: SRTM's -32768 voids become 0 m, -9999 becomes 3108.2 m), so
    the slope costs are r5r's, with a warning (``UserWarning``) that r5r
    does not give, counting them and the samples equal to the raster's
    no-data value. As in r5r, a raster also disables the swap of origins and
    destinations that r5r does for walking, and a routing call with a
    scenario (``new_carspeeds``, ``carspeed_scale``, ``new_lts``) runs
    without the slope costs, with r5r's warning.

    The GTFS feeds are loaded in the order in which the operating system
    lists the directory, as R5 does. With more than one feed that order
    depends on the file system, so the same directory can give slightly
    different expanded and arrival matrices on two machines (another stop
    or route for a few trips); r5r behaves in the same way, and on one
    machine the results are always the same.

    As in r5r, the network is not built when a GTFS feed has errors of
    priority ``"HIGH"`` (a reference to a stop, route, trip, service or
    agency that does not exist, a number that cannot be read, tables in a
    subfolder of the ``.zip``, routes without any agency): the
    ``RuntimeError`` names the feeds and points to
    [`get_gtfs_errors()`][peabiru.get_gtfs_errors], which lists the errors.
    Unlike r5r, nothing is written to ``data_path`` (r5r writes them to
    ``gtfs_errors.csv``). A feed that r5r's GTFS library fails to load at
    all is an error too.

    Parameters
    ----------
    data_path : str or os.PathLike
        The directory with the ``.pbf`` file and the GTFS feeds.
    verbose : bool, default False
        Whether to print informative messages (to standard error).
    temp_dir : bool, default False
        Accepted for compatibility with r5r, which uses it to choose where
        to keep the R5 jar; it has no effect.
    elevation : {"TOBLER", "MINETTI", "NONE"}, default "TOBLER"
        The slope cost function for walking and cycling when ``data_path``
        has a ``.tif`` elevation raster: Tobler's hiking function,
        Minetti's energy cost of walking, or no slope costs.
    overwrite : bool, default False
        Accepted for compatibility with r5r, which uses it to rebuild a
        cached ``network.dat``; the network is always built anew.
    n_threads : int or math.inf, default math.inf
        The number of threads used to read the ``.pbf`` file; ``math.inf``
        (or ``None``) uses all cores.

    Returns
    -------
    Network
        The network to pass to the routing functions.

    Raises
    ------
    RuntimeError
        When a file cannot be read, a GTFS feed has errors of priority
        ``"HIGH"``, or the elevation raster is not one peabiru reads.

    Examples
    --------
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> network.info()["feeds"]
    ['spo_gtfs']
    """
    if isinstance(data_path, os.PathLike):
        data_path = os.fspath(data_path)
    assert_string(data_path, "data_path")
    if not os.path.isdir(os.path.expanduser(data_path)):
        fail_assert("data_path", f"Directory '{data_path}' does not exist")
    assert_flag(verbose, "verbose")
    assert_flag(temp_dir, "temp_dir")
    assert_string(elevation, "elevation")
    assert_flag(overwrite, "overwrite")
    threads = assign_n_threads(n_threads)

    # r5r set_elevation()
    elevation = elevation.upper()
    valid = ["TOBLER", "MINETTI", "NONE"]
    if elevation not in valid:
        raise ValueError(
            f"Invalid value for 'elevation': \"{elevation}\". Must be one of: "
            + ", ".join(f'"{v}"' for v in valid)
            + "."
        )

    data_path = os.path.realpath(os.path.expanduser(data_path))
    # R's list.files(): no hidden files
    files = sorted(f for f in os.listdir(data_path) if not f.startswith("."))
    # r5r build_network() looks for its inputs with these patterns. They
    # are regular expressions, so ".tif" also matches a name such as
    # "identificacao.csv"; any_tif keeps that loose match because r5r uses
    # it to turn off the swap of origins and destinations.
    any_pbf = any(re.search(".pbf", f) for f in files)
    any_tif = any(re.search(".tif", f) for f in files)
    # lower case only, as r5r and the engine match the extension
    pbf = [f for f in files if re.search(r"\.pbf$", f)]
    if not any_pbf or not pbf:
        raise ValueError(f"An OSM .pbf file is required in '{data_path}' to build a network.")
    if len(pbf) > 1:
        raise ValueError(
            f"'{data_path}' has more than one .pbf file ({', '.join(pbf)}); keep only one."
        )

    if not any_tif:
        elevation = "NONE"
        if verbose:
            print(
                f"No raster .tif files found in '{data_path}'. Using elevation = \"NONE\".",
                file=sys.stderr,
            )

    t0 = time.perf_counter()
    core = _peabiru.build_network(data_path, threads, elevation)
    # the model in use: "NONE" when no file ends in .tif (R5 finds no raster)
    network = Network(core, data_path, core.info()["elevation"], any_tif)
    for message in core.r5r_warnings():
        warnings.warn(message, stacklevel=2)
    if verbose:
        print(
            f"Finished building network at '{data_path}' in "
            f"{time.perf_counter() - t0:.2f} s.",
            file=sys.stderr,
        )
    return network
