use extendr_api::prelude::*;

/// Version of the Rust core this package was built against.
/// @noRd
#[extendr]
fn rs_core_version() -> &'static str {
    peabiru_core::VERSION
}

// Macro to generate exports.
// This ensures exported functions are registered with R.
// See corresponding C code in `entrypoint.c`.
extendr_module! {
    mod peabiru;
    fn rs_core_version;
}
