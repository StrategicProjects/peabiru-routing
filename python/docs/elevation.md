# Elevation

A `.tif` elevation raster in the data directory gives walking and cycling
times slope costs: [`build_network()`][peabiru.build_network] samples the
raster every 10 m along each street, at the nearest pixel, and multiplies
the time of each street by the average cost of its slopes (the parts of
streets at the origin and the destination keep their plain times). Car
times never change. The cost function is chosen with `elevation`:

- `"TOBLER"` (the default): Tobler's hiking function;
- `"MINETTI"`: Minetti's energy cost of walking on slopes;
- `"NONE"`: no slope costs, even with a raster.

The slope costs are those of Conveyal's R5 7.6, ported to Rust.

The examples use the sample network with three small synthetic rasters
made for the package's tests (smooth hills between 743 and 834 m over the
sample area, not measured data), downloaded once into the temporary
directory (about 9 kB):

```python
import shutil
import tempfile
import urllib.request
import warnings
from pathlib import Path

import peabiru

raw = ("https://raw.githubusercontent.com/StrategicProjects/peabiru-routing/"
       "7184f3f2fdadc4cf05ede19d832f2ad5606be712/python/tests/data/")
rasters = Path(tempfile.gettempdir()) / "peabiru-rasters"
rasters.mkdir(exist_ok=True)
for name in ["spo_hills.tif", "spo_hills_31983.tif", "spo_voids.tif"]:
    if not (rasters / name).exists():
        urllib.request.urlretrieve(raw + name, rasters / name)


def data_dir(raster=None, name="hills.tif"):
    """A new directory with the sample .pbf and GTFS feed, and a raster."""
    d = Path(tempfile.mkdtemp())
    for f in ["spo_osm.pbf", "spo_gtfs.zip"]:
        shutil.copy(Path(peabiru.sample_data_path()) / f, d)
    if raster is not None:
        shutil.copy(rasters / raster, d / name)
    return d


points = peabiru.sample_points()
```

## Slope costs

The same directory, built with each cost function:

```python
hills = data_dir("spo_hills.tif")
networks = {e: peabiru.build_network(hills, elevation=e, n_threads=2)
            for e in ["NONE", "TOBLER", "MINETTI"]}
print({e: n.elevation for e, n in networks.items()})
```

```text
{'NONE': 'NONE', 'TOBLER': 'TOBLER', 'MINETTI': 'MINETTI'}
```

The mean travel time between the 27 sample points, by mode and cost
function:

```python
import pandas as pd

rows = {}
for e, network in networks.items():
    rows[e] = {
        mode: peabiru.travel_time_matrix(network, points, points, mode=mode, n_threads=2)
        ["travel_time_p50"].astype(float).mean().round(2)
        for mode in ["WALK", "BICYCLE", "CAR"]
    }
print(pd.DataFrame(rows).T)
```

```text
          WALK  BICYCLE   CAR
NONE     19.70    11.82  7.46
TOBLER   20.40    12.17  7.46
MINETTI  21.06    12.46  7.46
```

`network.elevation` tells which function a network uses. Without any `.tif`
file in the directory the network is built with `"NONE"`, whatever
`elevation` says; as with the `.pbf` and the GTFS feeds, only a lower-case
extension counts (a `HILLS.TIF` is ignored).

## Rasters in other coordinate systems

The raster must be a one-band GeoTIFF, north up, uncompressed or compressed
with LZW, Deflate or PackBits (what GDAL writes by default). Besides WGS 84
longitude and latitude (EPSG:4326), peabiru reads the systems whose
transform it reproduces exactly, given by their EPSG code (what GDAL
writes): SIRGAS 2000 (4674), SAD69 (4618), Corrego Alegre 1970-72 (4225),
their UTM zones used in Brazil, SIRGAS 2000 / Brazil Polyconic (5880),
WGS 84 / UTM (32601-32660, 32701-32760) and WGS 84 / Pseudo-Mercator
(3857). The same hills in SIRGAS 2000 / UTM zone 23S (EPSG:31983):

```python
utm = peabiru.build_network(data_dir("spo_hills_31983.tif"), n_threads=2)
walk = peabiru.travel_time_matrix(utm, points, points, mode="WALK", n_threads=2)
print(utm.elevation, walk["travel_time_p50"].astype(float).mean().round(2))
```

```text
TOBLER 20.38
```

The mean differs a little from the EPSG:4326 raster's (20.40): the
reprojected pixels are not the same pixels.

### The same result on every machine

In a projected system, which pixel a street point falls in is computed
with sines and cosines. For a point within nanometres (5e-8 m, or 2e-13
degrees) of the boundary between two pixels of different values, or of the
raster's edge, the answer depends on the last bits of the trigonometry.
peabiru reads the pixel that Java's `StrictMath` gives (R5 is written in
Java), the same on every machine, and warns once with the
number of such points: about one in a hundred million for 30 m pixels.
Reprojecting the raster to EPSG:4326 removes the ambiguity. The sample has
none, so building it gives no warning:

```python
with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always")
    peabiru.build_network(data_dir("spo_hills_31983.tif"), n_threads=2)
print(len(caught), "warnings")
```

```text
0 warnings
```

A point that `StrictMath` itself puts on the raster's outer edge (inside
its envelope, past the last pixel) is an error: there is no pixel to read.

## Voids and out-of-range values

Elevations are kept, as in R5, in decimetres in 16-bit integers. Voids and
values beyond +/-3276.7 m therefore wrap around: SRTM's -32768 voids become
0 m, -9999 becomes 3108.2 m, and a NaN no-data value is stored as 0 m. The
slope costs are computed on the wrapped values; peabiru warns,
counting such samples and those equal to the raster's no-data value. A
raster with voids:

```python
with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always")
    voids = peabiru.build_network(data_dir("spo_voids.tif"), n_threads=2)
for w in caught:
    print(w.category.__name__, w.message)
```

```text
UserWarning of the 21899 elevation samples on the network, 6298 with a value beyond +/-3276.7 m, which r5r stores wrapped around as 16-bit decimetres (an SRTM void of -32768 becomes 0 m, -9999 becomes 3108.2 m, 3500 m becomes -3053.6 m); the slope costs are r5r's, but probably not what you want there: fill the voids or crop the raster
```

Fill the voids (with `gdal_fillnodata`, for instance) or crop the raster to
the study area before building the network.

## Rasters peabiru refuses

These are a `RuntimeError` that says what to change:

- a `.prj` file next to the raster (it would contradict or duplicate the
  GeoTIFF's own coordinate system);
- a raster whose name ends in `.tiff` (rename it to `.tif`);
- more than one raster in the directory (which one to read would be
  arbitrary);
- a raster in another coordinate system, or with another layout or
  compression.

```python
d = data_dir("spo_hills.tif")
(d / "hills.prj").write_text("")
try:
    peabiru.build_network(d, n_threads=2)
except RuntimeError as e:
    print("RuntimeError:", str(e).split(": ", 1)[1][:60], "...")
```

```text
RuntimeError: the raster has a hills.prj file next to it, whose coordinate ...
```

## Elevation and scenarios

A routing call with a scenario (`new_carspeeds`, `carspeed_scale` or
`new_lts`, see [Scenarios](scenarios.md)) runs on a copy of the network
without the slope costs, as in R5, with a warning:

```python
tobler = networks["TOBLER"]
with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always")
    scenario = peabiru.travel_time_matrix(
        tobler, points, points, mode="WALK", carspeed_scale=0.8, n_threads=2
    )
print(caught[0].message)
flat = peabiru.travel_time_matrix(networks["NONE"], points, points, mode="WALK", n_threads=2)
print(scenario.equals(flat))
```

```text
A scenario was used for this calculation: WALK and BICYCLE are routed without the slope costs of the network's elevation.
True
```

A network with elevation also turns off one shortcut: a walking matrix with
more origins than destinations is routed from the destinations (and the
columns swapped back) only when the data directory has no `.tif` file.
