"""Compare GeoTools' transform (Steps.java output, STEPS=0) with
`peabiru crs-transform` for one EPSG code.

Usage: python3 compare.py <points> <geotools out> <code> <peabiru out>

Prints the number of points whose x or y differ in any bit, and the largest
absolute difference (in the system's units).
"""
import struct
import sys


def h(s):
    return struct.unpack('>d', bytes.fromhex(s.rjust(16, '0')))[0]


points, java, code, ours = sys.argv[1:5]
rows, on = [], False
for line in open(java):
    if line.startswith('# '):
        on = line.split()[1] == code
        continue
    if on:
        rows.append(line.split()[:2])
mine = [line.split() for line in open(ours)]
assert len(rows) == len(mine), (len(rows), len(mine))
differ, worst, errors = 0, 0.0, 0
for j, p in zip(rows, mine):
    if j[0] == 'ERR':
        errors += 1
        continue
    if j != p:
        differ += 1
        worst = max(worst, abs(h(j[0]) - h(p[0])), abs(h(j[1]) - h(p[1])))
print(f"EPSG:{code:6} points {len(rows)} differ {differ} largest difference {worst:.3g}"
      f" (GeoTools errors {errors})")
