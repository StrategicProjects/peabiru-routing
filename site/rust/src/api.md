# API reference

The rustdoc reference of `peabiru-core` is published with this guide:
**[open the API reference](api/peabiru_core/index.html)**.

Main entry points:

| Item | What it does |
|---|---|
| [`transit::build_from_dir`](api/peabiru_core/transit/fn.build_from_dir.html) | street network + transit layer from a directory |
| [`street::build_network`](api/peabiru_core/street/build/fn.build_network.html) | street network from a `.pbf` |
| [`transit::matrix::TransitData`](api/peabiru_core/transit/matrix/struct.TransitData.html) | per-network walking tables |
| [`street::travel_time_matrix`](api/peabiru_core/street/router/fn.travel_time_matrix.html) | street matrix |
| [`transit::matrix::travel_time_matrix`](api/peabiru_core/transit/matrix/fn.travel_time_matrix.html) | walk + transit matrix |
| [`transit::accessibility`](api/peabiru_core/transit/accessibility/index.html) | accessibility |
| [`transit::expanded`](api/peabiru_core/transit/expanded/index.html) | expanded and arrival matrices |

Locally, `cargo doc --no-deps -p peabiru-core --open` builds the same pages.
