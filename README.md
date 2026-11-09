# peabiru

Multimodal routing for OpenStreetMap street networks and GTFS public
transport: travel-time matrices, accessibility and isochrones. One Rust
engine, published three ways:

| Package | Registry | Directory |
|---|---|---|
| `peabiru-core` | crates.io | `crates/peabiru-core` |
| `peabiru` | PyPI | `python/` (PyO3 + maturin) |
| `peabiru` | CRAN | `r/` (extendr) |

No Java runtime. The goal is an alternative to
[r5r](https://github.com/ipea/r5r) / [r5py](https://github.com/r5py/r5py)
(both wrappers of Conveyal's [R5](https://github.com/conveyal/r5)) with the
same results, measured in `bench/`, and less time and memory.

The name comes from the Peabiru, the pre-Columbian network of trails that
crossed South America from the Atlantic to the Andes.

**Status:** early development, nothing usable yet.

## Development

```sh
cargo test                                  # core + CLI
cd python && maturin develop --uv           # Python extension into python/.venv
Rscript -e 'devtools::test("r")'            # R package
```

License: MIT.
