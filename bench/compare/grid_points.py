"""Write a regular grid of points over a bounding box as id,lon,lat CSV.

Usage: python grid_points.py min_lon min_lat max_lon max_lat spacing_m out.csv
"""
import math
import sys


def main(min_lon, min_lat, max_lon, max_lat, spacing_m, out):
    min_lon, min_lat, max_lon, max_lat, spacing_m = map(float, (min_lon, min_lat, max_lon, max_lat, spacing_m))
    dlat = spacing_m / 111_111.111
    dlon = dlat / math.cos(math.radians((min_lat + max_lat) / 2))
    n = 0
    with open(out, "w") as f:
        f.write("id,lon,lat\n")
        lat = min_lat
        r = 0
        while lat <= max_lat:
            lon = min_lon
            c = 0
            while lon <= max_lon:
                f.write(f"r{r}c{c},{lon:.7f},{lat:.7f}\n")
                n += 1
                lon += dlon
                c += 1
            lat += dlat
            r += 1
    print(f"{n} points -> {out}")


if __name__ == "__main__":
    main(*sys.argv[1:7])
