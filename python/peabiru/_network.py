"""Building a transport network: ``build_network()``."""

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
        Whether the directory has a ``.tif`` raster. Walking matrices with
        more origins than destinations are routed from the destinations
        (and the columns swapped back), except when it has one.

    Examples
    --------
    >>> import peabiru
    >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
    >>> network.elevation, network.has_tif
    ('NONE', False)
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

        Examples
        --------
        >>> import peabiru
        >>> network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
        >>> info = network.info()
        >>> info["feeds"], info["frequencies"]
        (['spo_gtfs'], True)
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
            "Assertion on 'network' failed: Must be a 'peabiru.Network' "
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
    and optionally an elevation raster (``.tif``). The network lives in
    memory and is reused by every routing function; nothing is written to
    ``data_path``. The files are recognised by their extension in lower case
    only: a ``map.PBF`` or a ``feed.ZIP`` is ignored.

    A ``.tif`` raster in ``data_path`` gives walking and cycling times slope
    costs (``elevation="TOBLER"``, the default, or ``"MINETTI"``; never for
    cars): the raster is sampled every 10 m along each street at the nearest
    pixel, and each street's time is multiplied by the average cost of its
    slopes. Points outside the raster count as 0 m high. Without a ``.tif``
    raster the network is built with ``elevation="NONE"``.

    The raster must be a GeoTIFF with one band, north up, uncompressed or
    compressed with LZW, Deflate or PackBits (what GDAL writes by default),
    in WGS 84 longitude and latitude (EPSG:4326) or in another coordinate
    reference system whose transform peabiru reproduces exactly, given by
    its EPSG code (what GDAL writes): SIRGAS 2000 (4674), SAD69 (4618),
    Corrego Alegre 1970-72 (4225), their UTM zones used in Brazil
    (31965-31985, 6210, 6211, 5396; 29168-29172, 29187-29195; 22521-22525),
    SIRGAS 2000 / Brazil Polyconic (5880), WGS 84 / UTM (32601-32660,
    32701-32760) and WGS 84 / Pseudo-Mercator (3857). Other rasters are an
    error that says what to change, and so are a ``.prj`` file next to the
    raster, a raster whose name ends in ``.tiff`` and more than one raster.

    In a projected system, a street point within nanometres (5e-8 m, or
    2e-13 degrees) of the boundary between two pixels of different values,
    or of the raster's edge, reads the pixel that Java's ``StrictMath``
    gives, the same on every machine, and a ``UserWarning`` gives the number
    of such points (about one in a hundred million for 30 m pixels;
    reprojecting the raster to EPSG:4326 removes the ambiguity). A point
    that ``StrictMath`` puts on the raster's outer edge (inside its
    envelope, past the last pixel) is an error: there is no pixel to read.
    Elevations are kept, as in R5, in decimetres in 16-bit integers, so
    voids and values beyond +/-3276.7 m wrap around (SRTM's -32768 voids
    become 0 m, -9999 becomes 3108.2 m, a NaN no-data value becomes 0 m);
    a ``UserWarning`` counts them and the samples equal to the raster's
    no-data value. A routing call with a scenario (``new_carspeeds``,
    ``carspeed_scale``, ``new_lts``) on a network with elevation runs
    without the slope costs, with a warning. See [Elevation](elevation.md).

    The GTFS feeds are loaded in the order in which the operating system
    lists the directory. With more than one feed that order depends on the
    file system, so the same directory can give slightly different expanded
    and arrival matrices on two machines (another stop or route for a few
    trips); on one machine the results are always the same.

    The network is not built when a GTFS feed has errors of priority
    ``"HIGH"`` (a reference to a stop, route, trip, service or agency that
    does not exist, a number that cannot be read, tables in a subfolder of
    the ``.zip``, routes without any agency): the ``RuntimeError`` names the
    feeds and points to [`get_gtfs_errors()`][peabiru.get_gtfs_errors],
    which lists the errors. A feed that cannot be loaded at all is an error
    too.

    Parameters
    ----------
    data_path : str or os.PathLike
        The directory with the ``.pbf`` file, the GTFS feeds and the
        elevation raster.
    verbose : bool, default False
        Whether to report the build of the network (on standard error).
    temp_dir : bool, default False
        Only checked; it has no effect (see the Notes).
    elevation : {"TOBLER", "MINETTI", "NONE"}, default "TOBLER"
        The slope cost function for walking and cycling when ``data_path``
        has a ``.tif`` elevation raster: Tobler's hiking function,
        Minetti's energy cost of walking, or no slope costs. Case does not
        matter.
    overwrite : bool, default False
        Only checked; it has no effect: the network is always built anew.
    n_threads : int or math.inf, default math.inf
        The number of threads used to read the ``.pbf`` file; ``math.inf``
        (or ``None``) uses all cores.

    Returns
    -------
    Network
        The network to pass to the routing functions.

    Raises
    ------
    ValueError
        When ``data_path`` has no ``.pbf`` file or more than one, or
        ``elevation`` is not one of the choices.
    RuntimeError
        When a file cannot be read, a GTFS feed has errors of priority
        ``"HIGH"``, or the elevation raster is not one peabiru reads.

    Notes
    -----
    The network is that of Conveyal's R5 7.6, ported to Rust. Nothing is
    written to ``data_path``; ``temp_dir`` and ``overwrite`` are checked and
    have no effect (there is no file to keep or to rebuild). More than one
    ``.pbf`` file is an error. In a projected raster, the pixel read for a
    point within nanometres of a pixel boundary is the one Java's
    ``StrictMath`` gives, on every machine, with a warning that counts such
    points.

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

    # the elevation model
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
    # The inputs are found with these patterns. They are regular
    # expressions, so ".tif" also matches a name such as
    # "identificacao.csv"; any_tif keeps that loose match because it turns
    # off the swap of origins and destinations (see swap_od()).
    any_pbf = any(re.search(".pbf", f) for f in files)
    any_tif = any(re.search(".tif", f) for f in files)
    # lower case only, as the engine matches the extension
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
    for message in core.build_warnings():
        warnings.warn(message, stacklevel=2)
    if verbose:
        print(
            f"Finished building network at '{data_path}' in "
            f"{time.perf_counter() - t0:.2f} s.",
            file=sys.stderr,
        )
    return network
