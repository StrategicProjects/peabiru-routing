# API reference

The rustdoc reference of `peabiru-core` is published with this guide:
**[open the API reference](api/peabiru_core/index.html)**.

Everything public is at the root of the crate (`use peabiru_core::{...}`).
Main entry points:

| Item | What it does |
|---|---|
| [`build_from_dir`](api/peabiru_core/fn.build_from_dir.html) | street network + transit layer from a directory |
| [`build_network`](api/peabiru_core/fn.build_network.html) | street network from a `.pbf` |
| [`TransitData`](api/peabiru_core/struct.TransitData.html) | per-network walking tables |
| [`street_travel_time_matrix`](api/peabiru_core/fn.street_travel_time_matrix.html) | street matrix ([`StreetMatrix`](api/peabiru_core/struct.StreetMatrix.html)) |
| [`transit_travel_time_matrix`](api/peabiru_core/fn.transit_travel_time_matrix.html) | walk + transit matrix ([`TransitMatrix`](api/peabiru_core/struct.TransitMatrix.html)) |
| [`transit_accessibility`](api/peabiru_core/fn.transit_accessibility.html), [`street_accessibility`](api/peabiru_core/fn.street_accessibility.html) | accessibility ([`Accessibility`](api/peabiru_core/struct.Accessibility.html)) |
| [`expanded_travel_time_matrix`](api/peabiru_core/fn.expanded_travel_time_matrix.html), [`arrival_travel_time_matrix`](api/peabiru_core/fn.arrival_travel_time_matrix.html) | expanded and arrival matrices ([`ExpandedMatrix`](api/peabiru_core/struct.ExpandedMatrix.html)) |

The parameter structs ([`BuildOptions`](api/peabiru_core/struct.BuildOptions.html),
[`RouteParams`](api/peabiru_core/struct.RouteParams.html),
[`TransitParams`](api/peabiru_core/struct.TransitParams.html),
[`AccessibilityParams`](api/peabiru_core/struct.AccessibilityParams.html))
are filled in by changing the fields of their `default()`. Routing errors are
a [`RouteError`](api/peabiru_core/enum.RouteError.html), network-building
errors a [`NetworkError`](api/peabiru_core/struct.NetworkError.html).

Locally, `cargo doc --no-deps -p peabiru-core --open` builds the same pages.
