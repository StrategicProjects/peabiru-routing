//! GTFS input: a minimal zip reader, CSV parsing and the feed tables.

pub mod csv;
pub mod feed;
pub mod zip;

pub use feed::GtfsError;
