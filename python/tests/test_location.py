import itertools

import numpy as np
import pandas as pd
import pytest

import peabiru


def split(points):
    schools = points[points["schools"] > 0].reset_index(drop=True)
    others = points[points["schools"] == 0].reset_index(drop=True)
    return schools, others


def walk_table(network, points, sites, minutes):
    return peabiru.travel_time_matrix(
        network, points, sites, mode="WALK", max_trip_duration=minutes, n_threads=2
    ).dropna()


def covered_by(table, points, site_ids, cutoff):
    """Population within `cutoff` of any of the sites, from the matrix."""
    near = table[(table["travel_time_p50"] <= cutoff) & table["to_id"].isin(site_ids)]
    return float(points.set_index("id").loc[near["from_id"].unique(), "population"].sum())


def test_max_cover_tables_and_brute_force(network, points):
    schools, others = split(points)
    cutoff, p = 12, 2
    found = peabiru.locate_facilities(
        network, points, others, existing=schools, model="max_cover", n_facilities=p,
        cutoff=cutoff, demand_colname="population", mode="WALK", n_threads=2,
    )
    sites, candidates, demand, summary = found
    assert list(sites.columns) == ["id", "lon", "lat", "order", "marginal", "cumulative"]
    assert list(candidates.columns) == ["id", "lon", "lat", "potential", "chosen"]
    assert list(demand.columns) == [
        "id", "lon", "lat", "demand", "facility_id", "travel_time", "covered"
    ]
    assert len(candidates) == len(others) and len(demand) == len(points)
    assert list(sites["order"]) == list(range(1, len(sites) + 1))
    assert set(sites["id"]) == set(candidates["id"][candidates["chosen"]])
    assert summary["model"] == "max_cover" and summary["n_sites"] == len(sites)

    # against the matrix and an exhaustive search
    all_sites = pd.concat([schools, others], ignore_index=True)
    table = walk_table(network, points, all_sites, cutoff)
    before = covered_by(table, points, list(schools["id"]), cutoff)
    assert summary["covered_before"] == before
    best = max(
        covered_by(table, points, list(schools["id"]) + list(c), cutoff)
        for c in itertools.combinations(others["id"], p)
    )
    got = covered_by(table, points, list(schools["id"]) + list(sites["id"]), cutoff)
    assert summary["covered"] == got == summary["objective"]
    assert got == best
    assert summary["objective_greedy"] <= got
    assert demand["demand"][demand["covered"]].sum() == got
    assert sites["cumulative"].iloc[-1] == got
    assert np.isclose(sites["marginal"].sum(), got - before)
    for row in candidates.itertuples():
        alone = covered_by(table, points, list(schools["id"]) + [row.id], cutoff)
        assert row.potential == alone - before

    # the same from the table, without a network or coordinates
    again = peabiru.locate_facilities(
        None, points[["id", "population"]], others[["id"]], existing=schools[["id"]],
        model="max_cover", n_facilities=p, cutoff=cutoff, demand_colname="population",
        travel_times=table,
    )
    assert list(again.sites.columns) == ["id", "order", "marginal", "cumulative"]
    assert list(again.sites["id"]) == list(sites["id"])
    assert again.summary == summary


def test_set_cover_covers_what_can_be_covered(network, points):
    cutoff = 10
    found = peabiru.locate_facilities(
        network, points, points, model="set_cover", cutoff=cutoff,
        demand_colname="population", mode="WALK", n_threads=2,
    )
    table = walk_table(network, points, points, cutoff)
    reachable = set(table["from_id"])
    with_demand = set(points["id"][points["population"] > 0])
    covered = set(found.demand["id"][found.demand["covered"]])
    assert with_demand & reachable <= covered
    unserved = points["population"][~points["id"].isin(reachable)].sum()
    assert found.summary["unservable_demand"] == unserved
    assert found.summary["objective"] == len(found.sites)
    assert found.summary["objective"] <= found.summary["objective_greedy"]


@pytest.mark.parametrize("model", ["p_median", "p_center"])
def test_median_and_center_against_brute_force(network, points, model):
    found = peabiru.locate_facilities(
        network, points, points, model=model, n_facilities=2,
        demand_colname="population", mode="WALK", max_trip_duration=60, n_threads=2,
    )
    table = walk_table(network, points, points, 60)
    wide = table.pivot(index="from_id", columns="to_id", values="travel_time_p50").astype(float)
    pop = points.set_index("id")["population"].astype(float)
    served = [i for i in wide.index if pop[i] > 0]
    penalty = 2 * np.nanmax(wide.to_numpy())

    def objective(sites):
        t = wide.loc[served, list(sites)].min(axis=1).fillna(penalty)
        return float((t * pop[served]).sum()) if model == "p_median" else float(t.max())

    best = min(objective(c) for c in itertools.combinations(points["id"], 2))
    assert np.isclose(found.summary["objective"], objective(found.sites["id"]))
    assert found.summary["objective"] <= found.summary["objective_greedy"]
    assert found.summary["objective"] >= best - 1e-9
    # on this small instance the heuristic reaches the optimum
    assert np.isclose(found.summary["objective"], best)
    assert found.summary["covered"] is None
    d = found.demand.dropna(subset=["facility_id"])
    assert set(d["facility_id"]) <= set(found.sites["id"])
    nearest = wide.loc[list(d["id"]), list(found.sites["id"])].min(axis=1).to_numpy()
    assert (d["travel_time"].to_numpy() == nearest).all()


def test_threads_and_transit(network, points, departure):
    kw = dict(model="p_median", n_facilities=3, demand_colname="population",
              mode=["WALK", "TRANSIT"], departure_datetime=departure, seed=1,
              max_trip_duration=60)
    one = peabiru.locate_facilities(network, points, points, n_threads=1, **kw)
    two = peabiru.locate_facilities(network, points, points, n_threads=2, **kw)
    for a, b in zip(one[:3], two[:3]):
        pd.testing.assert_frame_equal(a, b)
    assert one.summary == two.summary
    assert len(one.sites) == 3


def test_floating_catchment_by_hand_and_on_the_network(network, points):
    times = pd.DataFrame({
        "from_id": ["a", "a", "b", "b"],
        "to_id": ["f", "g", "f", "g"],
        "travel_time": [5, 20, 12, 8],
    })
    demand = pd.DataFrame({"id": ["a", "b"], "residents": [100, 200]})
    supply = pd.DataFrame({"id": ["f", "g"], "places": [10, 30]})
    fca = peabiru.floating_catchment(
        None, demand, supply, cutoff=15, demand_colname="residents",
        supply_colname="places", travel_times=times,
    )
    assert list(fca.demand.columns) == ["id", "demand", "accessibility"]
    assert list(fca.supply.columns) == ["id", "supply", "catchment_demand", "ratio"]
    assert list(fca.supply["catchment_demand"]) == [300.0, 200.0]
    assert np.allclose(fca.supply["ratio"], [10 / 300, 30 / 200])
    assert np.allclose(fca.demand["accessibility"], [10 / 300, 10 / 300 + 30 / 200])

    schools, _ = split(points)
    for fun, value in [("step", None), ("exponential", None), ("linear", 10), ("logistic", 5)]:
        fca = peabiru.floating_catchment(
            network, points, schools, cutoff=20, decay_function=fun, decay_value=value,
            demand_colname="population", supply_colname="schools", mode="WALK", n_threads=2,
        )
        shared = (fca.demand["demand"] * fca.demand["accessibility"]).sum()
        reached = fca.supply["supply"][fca.supply["catchment_demand"] > 0].sum()
        assert np.isclose(shared, reached), fun
        assert (fca.demand["accessibility"] >= 0).all()
    fixed = peabiru.floating_catchment(
        network, points, schools, decay_function="fixed_exponential", decay_value=0.001,
        demand_colname="population", supply_colname="schools", mode="WALK", n_threads=2,
    )
    assert (fixed.demand["accessibility"] > 0).any()


def test_bad_arguments(network, points):
    schools, others = split(points)
    ok = dict(model="max_cover", cutoff=10, mode="WALK")
    with pytest.raises(ValueError, match="model"):
        peabiru.locate_facilities(network, points, others, model="median", cutoff=10)
    with pytest.raises(ValueError, match="cutoff"):
        peabiru.locate_facilities(network, points, others, model="max_cover")
    with pytest.raises(ValueError, match="cutoff"):
        peabiru.locate_facilities(network, points, others, model="p_median", cutoff=10)
    with pytest.raises(ValueError, match="n_facilities"):
        peabiru.locate_facilities(network, points, others, n_facilities=0, **ok)
    with pytest.raises(ValueError, match="only \\d+ candidate"):
        peabiru.locate_facilities(network, points, others, n_facilities=28, **ok)
    with pytest.raises(ValueError, match="share ids"):
        peabiru.locate_facilities(network, points, points, existing=schools, **ok)
    with pytest.raises(ValueError, match="no column 'residents'"):
        peabiru.locate_facilities(network, points, others, demand_colname="residents", **ok)
    with pytest.raises(ValueError, match="network"):
        peabiru.locate_facilities(None, points, others, **ok)
    with pytest.raises(TypeError, match="routing arguments"):
        peabiru.locate_facilities(
            None, points, others, travel_times=pd.DataFrame(), model="max_cover", cutoff=10,
            mode="WALK",
        )
    bad = points.assign(population=-1.0)
    with pytest.raises(ValueError, match="not negative"):
        peabiru.locate_facilities(network, bad, others, demand_colname="population", **ok)
    with pytest.raises(ValueError, match="cutoff"):
        peabiru.floating_catchment(network, points, schools, mode="WALK")
    with pytest.raises(ValueError, match="cutoff"):
        peabiru.floating_catchment(
            network, points, schools, cutoff=10, decay_function="fixed_exponential",
            decay_value=0.001,
        )
