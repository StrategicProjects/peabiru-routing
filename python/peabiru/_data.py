"""The sample network shipped with the package."""

from __future__ import annotations

import os

import pandas as pd

_SPO = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data", "spo")


def sample_data_path() -> str:
    """Directory of the sample network shipped with peabiru.

    A small network for examples and tests, cut from the Sao Paulo sample
    data of r5r: ``spo_osm.pbf``, the streets around Avenida Paulista (map
    data (c) OpenStreetMap contributors, available under the Open Database
    License, https://www.openstreetmap.org/copyright); ``spo_gtfs.zip``,
    the SPTrans frequency-based GTFS feed shipped with r5r, without
    ``shapes.txt``; and ``spo_points.csv``, 27 points of r5r's hexagonal
    grid with their population and opportunity counts.

    Returns
    -------
    str
        The absolute path of the directory, to pass to
        [`build_network()`][peabiru.build_network].

    Examples
    --------
    >>> import os
    >>> import peabiru
    >>> sorted(f for f in os.listdir(peabiru.sample_data_path()) if f.startswith("spo"))
    ['spo_gtfs.zip', 'spo_osm.pbf', 'spo_points.csv']
    """
    return _SPO


def sample_points() -> pd.DataFrame:
    """The 27 points of the sample network.

    Returns
    -------
    pandas.DataFrame
        ``id`` (string), ``lon``, ``lat``, ``population``, ``schools``,
        ``jobs`` and ``healthcare``, read from ``spo_points.csv`` in
        [`sample_data_path()`][peabiru.sample_data_path].

    Examples
    --------
    >>> import peabiru
    >>> points = peabiru.sample_points()
    >>> list(points.columns)
    ['id', 'lon', 'lat', 'population', 'schools', 'jobs', 'healthcare']
    >>> len(points)
    27
    """
    df = pd.read_csv(os.path.join(_SPO, "spo_points.csv"), dtype={"id": str})
    df["id"] = pd.Series(df["id"].tolist(), dtype=object, index=df.index)
    return df
