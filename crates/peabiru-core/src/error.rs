//! The error of building a network.

use crate::gtfs::GtfsError;
use crate::osm::pbf::PbfError;
use std::fmt;

/// Why a network could not be built: a file or directory that cannot be
/// read, a malformed or empty `.pbf` file, a malformed GTFS feed or one
/// with errors that make r5r refuse it, or a directory without a `.pbf`
/// file. The message names the file at fault.
#[derive(Debug)]
pub struct NetworkError(Repr);

#[derive(Debug)]
enum Repr {
    Io(std::io::Error),
    Osm(PbfError),
    Gtfs(GtfsError),
    /// The data directory holds no `.pbf` file.
    NoPbf(std::path::PathBuf),
    /// The elevation raster cannot be read or applied.
    Elevation(std::path::PathBuf, String),
}

impl NetworkError {
    pub(crate) fn io(e: std::io::Error) -> Self {
        NetworkError(Repr::Io(e))
    }

    pub(crate) fn no_pbf(dir: &std::path::Path) -> Self {
        NetworkError(Repr::NoPbf(dir.to_path_buf()))
    }

    pub(crate) fn elevation(file: &std::path::Path, msg: String) -> Self {
        NetworkError(Repr::Elevation(file.to_path_buf(), msg))
    }

    // Constructors rather than `From` impls, which would show the internal
    // error types in the documentation.
    pub(crate) fn osm(e: PbfError) -> Self {
        NetworkError(Repr::Osm(e))
    }

    pub(crate) fn gtfs(e: GtfsError) -> Self {
        NetworkError(Repr::Gtfs(e))
    }

    /// When the build failed because GTFS feeds have errors of priority
    /// `HIGH` (as r5r's `build_network()` fails): each such feed, as named
    /// in the build, with its number of `HIGH` errors, in loading order.
    /// [`gtfs_errors`](crate::gtfs_errors) lists the errors. `None` for any
    /// other error.
    pub fn high_priority_gtfs_errors(&self) -> Option<Vec<(&std::path::Path, usize)>> {
        match &self.0 {
            Repr::Gtfs(GtfsError::HighPriority(feeds)) => {
                Some(feeds.iter().map(|(p, n)| (p.as_path(), *n)).collect())
            }
            _ => None,
        }
    }
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Repr::Io(e) => write!(f, "{e}"),
            Repr::Osm(e) => write!(f, "{e}"),
            Repr::Gtfs(e) => write!(f, "{e}"),
            Repr::Elevation(file, msg) => write!(f, "{}: {msg}", file.display()),
            Repr::NoPbf(dir) => {
                write!(f, "{}: the directory has no .pbf file", dir.display())
            }
        }
    }
}

impl std::error::Error for NetworkError {}
