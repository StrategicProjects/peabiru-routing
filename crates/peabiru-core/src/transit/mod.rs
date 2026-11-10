//! Public transport: the transit layer built from GTFS, stop linking,
//! transfers and RAPTOR, following R5 7.6 as used by r5r.

pub(crate) mod accessibility;
pub(crate) mod decay;
pub(crate) mod expanded;
pub(crate) mod layer;
pub(crate) mod matrix;
pub(crate) mod raptor;
pub(crate) mod walk;

pub(crate) use layer::{gtfs_files_in_dir, TransitLayer};

use crate::street::{build_network, BuildOptions, StreetNetwork};
use crate::NetworkError;

/// Build the street network and the transit layer from a directory holding
/// one OpenStreetMap `.pbf` file and any number of GTFS `.zip` feeds, as
/// r5r's `build_network()` does (no elevation), and link the stops to the
/// streets.
///
/// Files are recognised by their extension in lower case only (`.pbf`,
/// `.zip`), as r5r does; with several `.pbf` files the last one listed is
/// used, and the feeds are loaded in the order the operating system lists
/// them, which is the order R5 loads them in. A directory without GTFS
/// feeds gives an empty layer ([`TransitLayer::feed_ids`] is empty), good
/// for street routing only.
///
/// Feeds that r5r refuses to build a network from are built all the same;
/// [`TransitLayer::r5r_warnings`] says which and why.
pub fn build_from_dir(
    dir: impl AsRef<std::path::Path>,
    opts: &BuildOptions,
) -> Result<(StreetNetwork, TransitLayer), NetworkError> {
    let dir = dir.as_ref();
    let io = |e: std::io::Error| {
        NetworkError::io(std::io::Error::new(
            e.kind(),
            format!("{}: {e}", dir.display()),
        ))
    };
    // r5r's NetworkBuilder.loadDirectory keeps the last `.pbf` listed.
    let mut pbf = None;
    for e in std::fs::read_dir(dir).map_err(io)? {
        let e = e.map_err(io)?;
        if layer::has_suffix(&e.file_name(), ".pbf") {
            pbf = Some(e.path());
        }
    }
    let pbf = pbf.ok_or_else(|| NetworkError::no_pbf(dir))?;
    let mut net = build_network(&pbf, opts)?;
    let feeds = gtfs_files_in_dir(dir).map_err(io)?;
    let mut layer = TransitLayer::load(&feeds).map_err(NetworkError::gtfs)?;
    // The only change to a network after it is built, and before anyone
    // can route on it (see `StreetNetwork`).
    layer.link_stops(&mut net);
    Ok((net, layer))
}
