use pyo3::prelude::*;

/// Version of the Rust core this extension was built against.
#[pyfunction]
fn core_version() -> &'static str {
    peabiru_core::VERSION
}

#[pymodule]
fn _peabiru(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_function(wrap_pyfunction!(core_version, m)?)?;
    Ok(())
}
