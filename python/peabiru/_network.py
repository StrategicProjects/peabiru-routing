"""Building a transport network (r5r ``build_network()``)."""

from __future__ import annotations

import os
import re
import sys
import time
from typing import Any, Dict, Optional, Union

from . import _peabiru
from ._assign import assign_n_threads
from ._checks import assert_flag, assert_string, fail_assert, unsupported

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
        ``"NONE"``: elevation is not supported yet.
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
            ``vertices`` and ``edges`` of the street network; ``feeds`` (the
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
    n_threads: Optional[int] = None,
) -> Network:
    """Build a transport network.

    Builds a multimodal transport network from a directory with one
    OpenStreetMap extract (``.pbf``) and zero or more GTFS feeds (``.zip``),
    as ``r5r::build_network()`` does. The network lives in memory and is
    reused by every routing function; nothing is written to ``data_path``,
    and there is no ``network.dat`` cache.

    Elevation is not supported yet. As in r5r, when the directory has no
    ``.tif`` raster the network is built with ``elevation="NONE"``; when it
    has one, ``elevation="NONE"`` must be given explicitly (and, as in r5r,
    the raster still disables the swap of origins and destinations that r5r
    does for walking).

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
        Only ``"NONE"`` can be routed; the default is fine when the
        directory has no ``.tif`` raster.
    overwrite : bool, default False
        Accepted for compatibility with r5r, which uses it to rebuild a
        cached ``network.dat``; the network is always built anew.
    n_threads : int, optional
        The number of threads used to read the ``.pbf`` file. ``None`` (the
        default) or ``math.inf`` use all cores.

    Returns
    -------
    Network
        The network to pass to the routing functions.

    Raises
    ------
    UnsupportedError
        When the directory has a ``.tif`` raster and ``elevation`` is not
        ``"NONE"``.

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
    # r5r build_network() looks for its inputs with these (regular
    # expression) patterns
    any_pbf = any(re.search(".pbf", f) for f in files)
    any_tif = any(re.search(".tif", f) for f in files)
    pbf = [f for f in files if re.search(r"\.pbf$", f, re.IGNORECASE)]
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
    elif elevation != "NONE":
        unsupported(
            f'Elevation (elevation = "{elevation}", with the .tif raster in '
            f"'{data_path}'); use elevation = \"NONE\" to ignore the raster"
        )

    t0 = time.perf_counter()
    core = _peabiru.build_network(data_path, threads)
    network = Network(core, data_path, elevation, any_tif)
    if verbose:
        print(
            f"Finished building network at '{data_path}' in "
            f"{time.perf_counter() - t0:.2f} s.",
            file=sys.stderr,
        )
    return network
