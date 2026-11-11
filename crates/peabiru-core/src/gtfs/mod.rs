//! GTFS input: a minimal zip reader, CSV parsing and the feed tables.

pub mod csv;
pub mod feed;
mod flex;
pub mod jparse;
mod population;
pub mod validate;
pub mod zip;

pub use feed::GtfsError;
