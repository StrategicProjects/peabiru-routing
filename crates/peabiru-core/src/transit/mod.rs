//! Public transport: the transit layer built from GTFS, stop linking,
//! transfers and RAPTOR, following R5 7.6 as used by r5r.

pub mod accessibility;
pub mod decay;
pub mod expanded;
pub mod layer;
pub mod matrix;
pub mod raptor;
pub mod walk;

pub use layer::{gtfs_files_in_dir, TransitLayer};

use crate::street::{build_network, BuildOptions, StreetNetwork};

/// An error while building a network from a data directory.
#[derive(Debug)]
pub enum NetworkError {
    Io(std::io::Error),
    Osm(crate::osm::pbf::PbfError),
    Gtfs(crate::gtfs::GtfsError),
    NoPbf,
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::Io(e) => write!(f, "{e}"),
            NetworkError::Osm(e) => write!(f, "{e}"),
            NetworkError::Gtfs(e) => write!(f, "{e}"),
            NetworkError::NoPbf => f.write_str("the directory has no .pbf file"),
        }
    }
}

impl std::error::Error for NetworkError {}

/// Build the street network and transit layer from a directory holding one
/// OpenStreetMap `.pbf` and any number of GTFS `.zip` files, as r5r's
/// `build_network()` does (no elevation).
pub fn build_from_dir(
    dir: impl AsRef<std::path::Path>,
    opts: &BuildOptions,
) -> Result<(StreetNetwork, TransitLayer), NetworkError> {
    let dir = dir.as_ref();
    let mut pbf = None;
    for e in std::fs::read_dir(dir).map_err(NetworkError::Io)? {
        let p = e.map_err(NetworkError::Io)?.path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pbf")) {
            pbf = Some(p);
            break;
        }
    }
    let pbf = pbf.ok_or(NetworkError::NoPbf)?;
    let mut net = build_network(&pbf, opts).map_err(NetworkError::Osm)?;
    let feeds = gtfs_files_in_dir(dir).map_err(NetworkError::Io)?;
    let mut layer = TransitLayer::load(&feeds).map_err(NetworkError::Gtfs)?;
    layer.link_stops(&mut net);
    Ok((net, layer))
}
