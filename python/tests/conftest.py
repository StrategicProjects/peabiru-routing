"""Fixtures: the small network shipped with the package (a crop of r5r's
Sao Paulo sample: streets around Avenida Paulista and the SPTrans
frequency-based GTFS), with 27 points."""

import datetime as dt

import pytest

import peabiru

DEPARTURE = dt.datetime(2019, 5, 13, 14, 0)


@pytest.fixture(scope="session")
def network():
    return peabiru.build_network(peabiru.sample_data_path(), n_threads=2)


@pytest.fixture
def points():
    return peabiru.sample_points()


@pytest.fixture
def departure():
    return DEPARTURE
