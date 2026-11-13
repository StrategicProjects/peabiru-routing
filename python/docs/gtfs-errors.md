# GTFS errors

GTFS feeds often have errors: a trip that stops at a stop the feed does not
have, two services with the same id, a time that cannot be read.
[`get_gtfs_errors()`][peabiru.get_gtfs_errors] lists them as the validator
gtfs-lib (Conveyal) reports them, with every check of its table
loaders and of its validation after loading, and
[`build_network()`][peabiru.build_network] refuses feeds with errors of
priority `"HIGH"`.

```python
import shutil
import tempfile
import zipfile
from pathlib import Path

import peabiru

network = peabiru.build_network(peabiru.sample_data_path(), n_threads=2)
errors = peabiru.get_gtfs_errors(network)
print(errors.to_string())
```

```text
       file  line               type       field   id priority
0    agency     3  DuplicateKeyError   agency_id    1   MEDIUM
1  calendar     8  DuplicateKeyError  service_id  USD   MEDIUM
2  calendar     9  DuplicateKeyError  service_id  U__   MEDIUM
3  calendar    10  DuplicateKeyError  service_id  US_   MEDIUM
4  calendar    11  DuplicateKeyError  service_id  _SD   MEDIUM
5  calendar    12  DuplicateKeyError  service_id  __D   MEDIUM
6  calendar    13  DuplicateKeyError  service_id  _S_   MEDIUM
```

Each row is one error: the table (`file`, without `.txt`), the line of the
record (the header being line 1; 0 for a whole table), gtfs-lib's error
class (`type`), the `field` and `id` involved (`None` when there is none)
and the `priority`, `"HIGH"`, `"MEDIUM"`, `"LOW"` or `"UNKNOWN"`. The rows
are listed feed by feed, in the order gtfs-lib finds the errors. The
feeds are read again on each call; nothing is written to the data
directory.

## Feeds that are refused

A feed with errors of priority `"HIGH"` (a reference to a stop, route,
trip, service or agency that does not exist, a number that cannot be read,
tables in a subfolder of the `.zip`, routes without any agency) would be
routed on broken data, so `build_network()` raises a `RuntimeError` that
names the feeds. Here the first stop time of the sample feed is made to
point to a stop that does not exist:

```python
broken = Path(tempfile.mkdtemp())
sample = Path(peabiru.sample_data_path())
shutil.copy(sample / "spo_osm.pbf", broken)
with zipfile.ZipFile(sample / "spo_gtfs.zip") as zin, \
        zipfile.ZipFile(broken / "spo_gtfs.zip", "w") as zout:
    for name in zin.namelist():
        data = zin.read(name).decode("utf-8")
        if name == "stop_times.txt":
            header, first, rest = data.split("\n", 2)
            fields = first.split(",")
            fields[header.split(",").index("stop_id")] = "nowhere"
            data = "\n".join([header, ",".join(fields), rest])
        zout.writestr(name, data)

try:
    peabiru.build_network(broken, n_threads=2)
except RuntimeError as e:
    print(str(e).split(". Use")[0])
```

```text
High priority GTFS errors found; network build failed (spo_gtfs.zip: 1 error)
```

The message points to `get_gtfs_errors()`, which also takes a directory of
`.zip` files, so it works on feeds that cannot be built:

```python
errors = peabiru.get_gtfs_errors(broken)
print(errors[errors["priority"] == "HIGH"].to_string())
```

```text
         file  line                       type    field    id priority
7  stop_times     2  ReferentialIntegrityError  stop_id  None     HIGH
```

Fix the feed (here, the `stop_id` of line 2 of `stop_times.txt`) or remove
the records at fault (with gtfs-kit in Python or gtfstools in R, for
instance), and build the network again.

A feed on which gtfs-lib fails instead of listing errors (a date that is
not `YYYYMMDD`, a frequency of a trip that does not exist, a service or fare
without an id, the first or last stop of a trip without a name) makes both
functions raise a `RuntimeError` naming the cause.
