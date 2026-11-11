"""Random lon/lat test points (hex bits), mixed distributions. usage: pts.py n out [seed]"""
import random, struct, sys

def h(v):
    return struct.pack('>d', v).hex()

n = int(sys.argv[1])
rng = random.Random(int(sys.argv[3]) if len(sys.argv) > 3 else 1)
out = open(sys.argv[2], 'w')
cms = [-177 + 6 * i for i in range(60)]
for i in range(n):
    k = i % 8
    if k in (0, 1, 2):  # Brazil and around
        lon, lat = rng.uniform(-75, -28), rng.uniform(-35, 7)
    elif k == 3:  # near a zone's central meridian, within the zone
        lon, lat = rng.choice(cms) + rng.uniform(-3.2, 3.2), rng.uniform(-80, 84)
    elif k == 4:  # world
        lon, lat = rng.uniform(-180, 180), rng.uniform(-85, 85)
    elif k == 5:  # near the equator
        lon, lat = rng.uniform(-75, -28), rng.uniform(-1e-3, 1e-3) * rng.choice([1, 1e-3, 1e-9])
    elif k == 6:  # Porto Alegre at street scale, fixed-point degrees as R5 stores them
        lon = round(rng.uniform(-51.3, -51.0), 7)
        lat = round(rng.uniform(-30.3, -29.9), 7)
    else:  # special values
        lon = rng.choice([-180.0, 180.0, -51.0, -57.0, -33.0, 0.0, -0.0, -75.0, 177.0, -177.0,
                          179.9999999, -179.9999999, -48.0 + 1e-12])
        lat = rng.choice([0.0, -0.0, -30.0, 5.0, 1e-300, -1e-300, -84.9, 84.9, -23.55])
    out.write(h(lon) + ',' + h(lat) + '\n')
