"""Porto Alegre GTFS feeds with shape_dist_traveled, for
bench/compare/shape_dist_parity.R.

    python3 bench/compare/shape_dist_feeds.py <dir of poa_eptc.zip and
        poa_trensurb.zip> <out dir> [variant...]

Each variant is a directory <out>/<variant>/ with the two feeds rewritten:
the distances along each shape (haversine metres), and for each stop the
distance of its projection on its trip's shape (searching forward from the
previous stop), scaled or broken as the variant says (see `variants`).
"""
import csv, io, math, os, random, sys, zipfile

src, out = sys.argv[1], sys.argv[2]


def read(feed, name):
    with zipfile.ZipFile(os.path.join(src, feed + ".zip")) as z:
        if name not in z.namelist():
            return None, None
        text = z.read(name).decode("utf-8-sig")
    rows = list(csv.reader(io.StringIO(text, newline="")))
    return rows[0], [r for r in rows[1:] if r]


def hav(lat1, lon1, lat2, lon2):
    R = 6371008.8
    p1, p2 = math.radians(lat1), math.radians(lat2)
    dp, dl = p2 - p1, math.radians(lon2 - lon1)
    a = math.sin(dp / 2) ** 2 + math.cos(p1) * math.cos(p2) * math.sin(dl / 2) ** 2
    return 2 * R * math.asin(math.sqrt(a))


def load(feed):
    t = {}
    for n in ["agency", "calendar", "calendar_dates", "fare_attributes", "feed_info",
              "routes", "shapes", "stop_times", "stops", "trips", "fare_rules"]:
        h, rows = read(feed, n + ".txt")
        if h is not None:
            t[n] = (h, rows)
    return t


def shape_points(t):
    h, rows = t["shapes"]
    i = {c: h.index(c) for c in h}
    pts = {}
    for r in rows:
        pts.setdefault(r[i["shape_id"]], []).append(
            (int(r[i["shape_pt_sequence"]]), float(r[i["shape_pt_lat"]]), float(r[i["shape_pt_lon"]]), r))
    for k in pts:
        pts[k].sort(key=lambda x: x[0])
    return pts


def cum(points):
    d = [0.0]
    for a, b in zip(points, points[1:]):
        d.append(d[-1] + hav(a[1], a[2], b[1], b[2]))
    return d


def project(points, dists, lat, lon, start):
    """distance along the shape of the nearest point, searching segments from start"""
    best = (float("inf"), start, 0.0)
    xs = math.cos(math.radians(lat))
    for s in range(start, len(points) - 1):
        a, b = points[s], points[s + 1]
        ax, ay, bx, by = a[2] * xs, a[1], b[2] * xs, b[1]
        px, py = lon * xs, lat
        dx, dy = bx - ax, by - ay
        L = dx * dx + dy * dy
        f = 0.0 if L == 0 else max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / L))
        qx, qy = ax + f * dx, ay + f * dy
        dd = (qx - px) ** 2 + (qy - py) ** 2
        if dd < best[0]:
            best = (dd, s, f)
    _, s, f = best
    return dists[s] + f * (dists[s + 1] - dists[s]), s


def fmt(x):
    return repr(round(x, 3)) if isinstance(x, float) else str(x)


def build(feed, t, shape_scale, stop_scale, mutate=None, rng=None):
    """returns tables with shape_dist_traveled in shapes and stop_times
    (scale None = column left out / blank)"""
    pts = shape_points(t)
    sdist = {k: cum(v) for k, v in pts.items()}
    th, trows = t["trips"]
    ti = {c: th.index(c) for c in th}
    shape_of = {r[ti["trip_id"]]: r[ti["shape_id"]] for r in trows if "shape_id" in ti}
    sh, srows = t["stops"]
    si = {c: sh.index(c) for c in sh}
    stops = {r[si["stop_id"]]: (float(r[si["stop_lat"]]), float(r[si["stop_lon"]])) for r in srows}
    h, rows = t["stop_times"]
    hi = {c: h.index(c) for c in h}
    by_trip = {}
    for r in rows:
        by_trip.setdefault(r[hi["trip_id"]], []).append(r)
    stop_d = {}
    for trip, rs in by_trip.items():
        rs.sort(key=lambda r: int(r[hi["stop_sequence"]]))
        sid = shape_of.get(trip)
        if not sid or sid not in pts:
            continue
        seg = 0
        ds = []
        for r in rs:
            lat, lon = stops[r[hi["stop_id"]]]
            d, seg = project(pts[sid], sdist[sid], lat, lon, seg)
            ds.append(d)
        stop_d[trip] = ds
    if mutate:
        mutate(pts, sdist, by_trip, stop_d, shape_of, rng)
    # write shapes
    out = dict(t)
    if shape_scale is not None:
        hdr = ["shape_id", "shape_pt_lat", "shape_pt_lon", "shape_pt_sequence", "shape_dist_traveled"]
        new = []
        for k, v in pts.items():
            for (seq, lat, lon, _), d in zip(v, sdist[k]):
                new.append([k, repr(lat), repr(lon), str(seq), "" if d is None else (d if isinstance(d, str) else fmt(d * shape_scale))])
        out["shapes"] = (hdr, new)
    if stop_scale is not None:
        hdr = list(h) if "shape_dist_traveled" in h else list(h) + ["shape_dist_traveled"]
        k = hdr.index("shape_dist_traveled")
        new = []
        for trip, rs in by_trip.items():
            ds = stop_d.get(trip)
            for j, r in enumerate(rs):
                r2 = list(r) + [""] * (len(hdr) - len(r))
                if ds is not None:
                    d = ds[j]
                    r2[k] = "" if d is None else (d if isinstance(d, str) else fmt(d * stop_scale))
                new.append(r2)
        out["stop_times"] = (hdr, new)
    return out


def write(dirname, feed, t):
    os.makedirs(dirname, exist_ok=True)
    with zipfile.ZipFile(os.path.join(dirname, feed + ".zip"), "w", zipfile.ZIP_DEFLATED) as z:
        for n, (h, rows) in t.items():
            b = io.StringIO()
            w = csv.writer(b, lineterminator="\n")
            w.writerow(h)
            w.writerows(rows)
            z.writestr(n + ".txt", b.getvalue())


def gaps(pts, sdist, by_trip, stop_d, shape_of, rng):
    trips = sorted(stop_d)
    for trip in rng.sample(trips, len(trips) // 3):
        ds = stop_d[trip]
        ds[rng.randrange(len(ds))] = None
    shapes = sorted(sdist)
    for s in rng.sample(shapes, len(shapes) // 5):
        sdist[s][rng.randrange(len(sdist[s]))] = None


def errors(pts, sdist, by_trip, stop_d, shape_of, rng):
    """decreasing, duplicated, beyond the ends, noisy, negative"""
    shapes = sorted(sdist)
    rng.shuffle(shapes)
    n = len(shapes)
    for s in shapes[: n // 6]:  # a decreasing window in the shape
        d = sdist[s]
        if len(d) > 6:
            a = rng.randrange(1, len(d) - 4)
            d[a:a + 4] = d[a:a + 4][::-1]
    for s in shapes[n // 6: n // 3]:  # duplicated distances
        d = sdist[s]
        for _ in range(3):
            a = rng.randrange(0, len(d) - 1)
            d[a + 1] = d[a]
    for s in shapes[n // 3: n // 2]:  # shifted: starts at 1000 m
        sdist[s] = [x + 1000.0 for x in sdist[s]]
    for s in shapes[n // 2: 2 * n // 3]:  # one negative
        sdist[s][0] = -25.0
    trips = sorted(stop_d)
    rng.shuffle(trips)
    m = len(trips)
    for trip in trips[: m // 5]:  # beyond both ends
        ds = stop_d[trip]
        ds[0] = -50.0
        ds[-1] = ds[-1] + 750.0
    for trip in trips[m // 5: 2 * m // 5]:  # noise of +-300 m
        stop_d[trip] = [x + rng.uniform(-300, 300) for x in stop_d[trip]]
    for trip in trips[2 * m // 5: m // 2]:  # duplicated / decreasing stop distances
        ds = stop_d[trip]
        if len(ds) > 3:
            k = rng.randrange(1, len(ds) - 1)
            ds[k] = ds[k - 1]
            ds[k + 1], ds[k] = ds[k], ds[k + 1]
    for trip in trips[m // 2: m // 2 + m // 20]:  # notations Java reads
        ds = stop_d[trip]
        ds[0] = "%.6e" % ds[0]


def nan(pts, sdist, by_trip, stop_d, shape_of, rng):
    """every shape starts with two points at distance 0 and every first stop at 0"""
    for s in sdist:
        sdist[s][1] = sdist[s][0] = 0.0
    for trip in stop_d:
        stop_d[trip][0] = 0.0


def words(pts, sdist, by_trip, stop_d, shape_of, rng):
    """values Java's parseDouble reads: Infinity, NaN, spaces, suffixes, hex"""
    shapes = sorted(sdist)
    for s in shapes[::5]:
        sdist[s][-1] = "Infinity"
    for s in shapes[1::5]:
        sdist[s][1] = " %r " % sdist[s][1]
    for s in shapes[2::5]:
        sdist[s][2] = "%rd" % sdist[s][2]
    trips = sorted(stop_d)
    for t in trips[::7]:
        stop_d[t][1] = "NaN"
    for t in trips[1::7]:
        stop_d[t][0] = "0x1p6"
    for t in trips[2::7]:
        stop_d[t][-1] = "Infinity"
    for t in trips[3::7]:
        stop_d[t][0] = "-0.0"


def bad(pts, sdist, by_trip, stop_d, shape_of, rng):
    """a value Java cannot read: a HIGH error, both refuse the feed"""
    trips = sorted(stop_d)
    stop_d[trips[3]][-1] = "abc"


variants = {
    "m": dict(shape_scale=1.0, stop_scale=1.0),
    "km": dict(shape_scale=0.001, stop_scale=0.001),
    "mixed": dict(shape_scale=1.0, stop_scale=0.001),  # shape in m, stops in km
    "shapes_only": dict(shape_scale=1.0, stop_scale=None),
    "stops_only": dict(shape_scale=None, stop_scale=1.0),
    "gaps": dict(shape_scale=1.0, stop_scale=1.0, mutate=gaps),
    "errors": dict(shape_scale=1.0, stop_scale=1.0, mutate=errors),
    "nan": dict(shape_scale=1.0, stop_scale=1.0, mutate=nan),
    "words": dict(shape_scale=1.0, stop_scale=1.0, mutate=words),
    "bad": dict(shape_scale=1.0, stop_scale=1.0, mutate=bad),
}
for name in sys.argv[3:] or list(variants):
    v = variants[name]
    for feed in ["poa_eptc", "poa_trensurb"]:
        t2 = build(feed, load(feed), v["shape_scale"], v["stop_scale"], v.get("mutate"),
                   random.Random(len(name) * 7 + len(feed)))
        write(os.path.join(out, name), feed, t2)
    print(name, "ok")
