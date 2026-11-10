//! The error of building a network.

use crate::gtfs::GtfsError;
use crate::osm::pbf::PbfError;
use std::fmt;

/// Why a network could not be built: a file or directory that cannot be
/// read, a malformed or empty `.pbf` file, a malformed GTFS feed, or a
/// directory without a `.pbf` file. The message names the file at fault.
#[derive(Debug)]
pub struct NetworkError(Repr);

#[derive(Debug)]
enum Repr {
    Io(std::io::Error),
    Osm(PbfError),
    Gtfs(GtfsError),
    /// The data directory holds no `.pbf` file.
    NoPbf(std::path::PathBuf),
}

impl NetworkError {
    pub(crate) fn io(e: std::io::Error) -> Self {
        NetworkError(Repr::Io(e))
    }

    pub(crate) fn no_pbf(dir: &std::path::Path) -> Self {
        NetworkError(Repr::NoPbf(dir.to_path_buf()))
    }

    // Constructors rather than `From` impls, which would show the internal
    // error types in the documentation.
    pub(crate) fn osm(e: PbfError) -> Self {
        NetworkError(Repr::Osm(e))
    }

    pub(crate) fn gtfs(e: GtfsError) -> Self {
        NetworkError(Repr::Gtfs(e))
    }
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Repr::Io(e) => write!(f, "{e}"),
            Repr::Osm(e) => write!(f, "{e}"),
            Repr::Gtfs(e) => write!(f, "{e}"),
            Repr::NoPbf(dir) => {
                write!(f, "{}: the directory has no .pbf file", dir.display())
            }
        }
    }
}

impl std::error::Error for NetworkError {}
