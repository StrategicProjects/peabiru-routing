"""Street travel-time matrices with general-purpose Python graph libraries,
called by bench/site/run.py.

    python engine_py_streets.py --probe --engine networkx|igraph|pandana
    python engine_py_streets.py --engine networkx --city poa --dir DIR
        --points CSV --task ttm_walk --threads 1 --reps 3 --params JSON
        [--origins N] [--result out.csv]

Prints one JSON line {"build_s", "times", "rows", "version", "notes",
"origins_used", "threads_used"}; with --result, also writes the last timed
result as from_id,to_id,travel_time (minutes, pairs reached within
max_trip_duration).

What a Python user does without r5py: OSMnx cannot read a .pbf (it reads
Overpass downloads or .osm XML), so the street network comes from pyrosm,
which reads the .pbf, filters the ways with OSMnx's network filters
("walking", "cycling", "driving") and exports an OSMnx-compatible networkx
MultiDiGraph (simplified, largest connected component). Edge weights are
travel times: length / speed for walking (3.6 km/h) and cycling (12 km/h);
for driving, OSMnx's add_edge_speeds (the maxspeed tag, else the mean
maxspeed of the road class in the same graph) and add_edge_travel_times.
That one graph is then routed by:

  networkx  single_source_dijkstra_path_length per origin, with the time
            limit as cutoff (pure Python, one thread)
  igraph    Graph.distances(origins, destinations) (C, one thread, no cutoff)
  pandana   Network.shortest_path_lengths over all origin-destination pairs
            (C++ contraction hierarchies; OpenMP threads from OMP_NUM_THREADS)

Points are snapped to the nearest graph node (OSMnx nearest_nodes for
networkx and igraph, pandana's own get_node_ids); snapping is inside the
timed call, as it is for r5r and peabiru. The build time covers reading the
.pbf, building the graph, the weights and the conversion to igraph or pandana
(and pandana's contraction).
"""

import json
import os
import sys
import time

ENGINES = ("networkx", "igraph", "pandana")
NETWORK = {"ttm_walk": "walking", "ttm_bicycle": "cycling", "ttm_car": "driving"}


def arg(name, default=None):
    if "--" + name not in sys.argv:
        if default is not None:
            return default
        raise SystemExit(f"missing --{name}")
    return sys.argv[sys.argv.index("--" + name) + 1]


def versions(engine):
    import importlib.metadata as md

    v = {p: md.version(p) for p in ("pyrosm", "osmnx", "networkx")}
    if engine == "igraph":
        v["igraph"] = md.version("igraph")
    if engine == "pandana":
        v["pandana"] = md.version("pandana")
    main = f"{engine} {v.get(engine)}"
    rest = ", ".join(f"{k} {x}" for k, x in v.items() if k != engine)
    return f"{main} ({rest})"


def main():
    engine = arg("engine")
    if engine not in ENGINES:
        print(f"unknown engine {engine}")
        sys.exit(2)
    threads = int(arg("threads", "1"))
    if engine == "pandana":  # before pandana (and its OpenMP runtime) is loaded
        os.environ["OMP_NUM_THREADS"] = str(threads)
    try:
        import networkx  # noqa: F401
        import osmnx  # noqa: F401
        import pyrosm  # noqa: F401

        if engine == "igraph":
            import igraph  # noqa: F401
        if engine == "pandana":
            import pandana  # noqa: F401
    except Exception as e:  # noqa: BLE001
        print(f"cannot import the packages for {engine} ({type(e).__name__}: {e})")
        sys.exit(3)
    version = versions(engine)
    if "--probe" in sys.argv:
        print(version)
        return

    import numpy as np
    import networkx as nx
    import osmnx as ox
    import pandas as pd
    from pyrosm import OSM

    P = json.loads(arg("params"))
    task, reps = arg("task"), int(arg("reps"))
    if task not in NETWORK:
        print(json.dumps({"skip": f"{engine} routes on a street graph only; {task} needs a "
                                  "public transport model"}))
        return
    d = arg("dir")
    pbf = next(os.path.join(d, f) for f in sorted(os.listdir(d)) if f.endswith(".pbf"))
    pts = pd.read_csv(arg("points"), dtype={"id": str})
    n_orig = int(arg("origins", "0")) or len(pts)
    orig = pts.iloc[:n_orig]
    limit_s = P["max_trip_duration"] * 60.0
    notes = []

    # ------------------------------------------------------------- build
    t0 = time.perf_counter()
    osm = OSM(pbf)
    nodes, edges = osm.get_network(network_type=NETWORK[task], nodes=True)
    G = osm.to_graph(nodes, edges, graph_type="networkx", progress=False)
    if task == "ttm_car":
        G = ox.add_edge_speeds(G)
        G = ox.add_edge_travel_times(G)
        notes.append("car speeds from OSMnx add_edge_speeds: maxspeed tag, else the mean "
                     "maxspeed of the road class in this graph; no turn restrictions")
    else:
        speed = (P["walk_speed"] if task == "ttm_walk" else P["bike_speed"]) / 3.6
        for _, _, dd in G.edges(data=True):
            dd["travel_time"] = float(dd["length"]) / speed
        if task == "ttm_bicycle":
            notes.append("no traffic-stress (LTS) limit: every way of pyrosm's cycling "
                         "network is usable")
    node_ids = np.array(list(G.nodes))
    xs = np.array([G.nodes[n]["x"] for n in node_ids])
    ys = np.array([G.nodes[n]["y"] for n in node_ids])
    if engine == "igraph":
        import igraph as ig

        pos = {n: i for i, n in enumerate(node_ids)}
        el = [(pos[u], pos[v]) for u, v in G.edges()]
        w = [dd["travel_time"] for _, _, dd in G.edges(data=True)]
        g = ig.Graph(n=len(node_ids), edges=el, directed=True)
        g.es["w"] = w
    elif engine == "pandana":
        import pandana

        uu, vv, ww = [], [], []
        for u, v, dd in G.edges(data=True):
            uu.append(u)
            vv.append(v)
            ww.append(dd["travel_time"])
        net = pandana.Network(pd.Series(xs, index=node_ids), pd.Series(ys, index=node_ids),
                              pd.Series(uu), pd.Series(vv), pd.DataFrame({"tt": ww}),
                              twoway=False)
    build_s = time.perf_counter() - t0
    notes.append(f"graph: {G.number_of_nodes():,} nodes, {G.number_of_edges():,} edges "
                 "(pyrosm, simplified, largest connected component); points snap to the "
                 "nearest node")

    ox_ = orig.lon.to_numpy()
    oy_ = orig.lat.to_numpy()
    dx_ = pts.lon.to_numpy()
    dy_ = pts.lat.to_numpy()

    # ------------------------------------------------------------- route
    def route():
        """Return (origin index, destination index, seconds) of reached pairs."""
        if engine == "pandana":
            on = net.get_node_ids(ox_, oy_).to_numpy()
            dn = net.get_node_ids(dx_, dy_).to_numpy()
            a = np.repeat(on, len(dn))
            b = np.tile(dn, len(on))
            s = np.asarray(net.shortest_path_lengths(a, b, imp_name="tt"), dtype=float)
            ok = s <= limit_s
            idx = np.nonzero(ok)[0]
            return idx // len(dn), idx % len(dn), s[ok]
        on = ox.distance.nearest_nodes(G, ox_, oy_)
        dn = ox.distance.nearest_nodes(G, dx_, dy_)
        if engine == "networkx":
            oi, di, ss = [], [], []
            for i, src in enumerate(on):
                lengths = nx.single_source_dijkstra_path_length(G, src, cutoff=limit_s,
                                                                weight="travel_time")
                for j, dst in enumerate(dn):
                    s = lengths.get(dst)
                    if s is not None:
                        oi.append(i)
                        di.append(j)
                        ss.append(s)
            return np.array(oi), np.array(di), np.array(ss)
        # igraph: distances() refuses repeated targets, so route between the
        # distinct snapped nodes and expand back to the points
        vid = {n: i for i, n in enumerate(node_ids)}
        uo, io = np.unique([vid[n] for n in on], return_inverse=True)
        ud, jd = np.unique([vid[n] for n in dn], return_inverse=True)
        m = np.array(g.distances(source=uo.tolist(), target=ud.tolist(), weights="w",
                                 mode="out"), dtype=float)[np.ix_(io, jd)]
        oi, di = np.nonzero(m <= limit_s)
        return oi, di, m[oi, di]

    res = route()  # warm-up
    times = []
    for _ in range(reps):
        t = time.perf_counter()
        res = route()
        times.append(time.perf_counter() - t)
    oi, di, ss = res
    if engine == "networkx":
        notes.append("networkx is pure Python and single-threaded")
    elif engine == "igraph":
        notes.append("igraph's distances() is single-threaded and has no time limit "
                     "(times above the limit are dropped afterwards)")
    else:
        notes.append("threads set with OMP_NUM_THREADS; no time limit in the query (times "
                     "above the limit are dropped afterwards)")
    if arg("result", ""):
        out = pd.DataFrame({"from_id": orig.id.to_numpy()[oi], "to_id": pts.id.to_numpy()[di],
                            "travel_time": np.round(ss / 60.0, 3)})
        out.to_csv(arg("result"), index=False)
    print(json.dumps({"build_s": build_s, "times": times, "rows": int(len(ss)),
                      "version": version, "notes": notes, "origins_used": n_orig,
                      "threads_used": threads if engine == "pandana" else 1}))


if __name__ == "__main__":
    main()
