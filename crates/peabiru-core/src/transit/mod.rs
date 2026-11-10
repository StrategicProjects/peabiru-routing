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

use crate::gtfs::zip::Limits;
use crate::gtfs::GtfsError;
use crate::street::build::build_network_from_bytes;
use crate::street::{BuildOptions, StreetNetwork};
use crate::NetworkError;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

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
///
/// This reads the files and builds the network as [`build_from_bytes`]
/// does from the same bytes, with the feeds in the directory's order;
/// errors name the files by their path.
pub fn build_from_dir(
    dir: impl AsRef<Path>,
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
    let bytes = std::fs::read(&pbf).map_err(|e| {
        use crate::osm::pbf::PbfError;
        NetworkError::osm(PbfError::InFile {
            path: pbf.clone(),
            error: Box::new(PbfError::Io(e)),
        })
    })?;
    // The street network is built before the feeds are listed, and each
    // feed is read only when its turn comes, so that no more than one
    // `.zip` is held in memory at a time.
    build(&pbf, Cow::Owned(bytes), opts, || {
        let feeds = gtfs_files_in_dir(dir).map_err(io)?;
        Ok(feeds.into_iter().map(|p| layer::read_feed_file(&p)))
    })
}

/// Build the street network and the transit layer from data held in
/// memory: the bytes of an OpenStreetMap `.pbf` file and of any number of
/// GTFS `.zip` feeds, each with its file name. The network is the one
/// [`build_from_dir`] builds from a directory holding the same files, and
/// the stops are linked to the streets in the same way.
///
/// The feeds are loaded in the order of `feeds`. That order matters: the
/// stops, patterns and, where two feeds offer equally good trips, the
/// routes chosen follow it. [`build_from_dir`] loads them in the order the
/// operating system lists the directory (as R5 does), which differs from
/// one file system to another; to reproduce a network built from a
/// directory, pass the feeds in the order of its
/// [`TransitLayer::feed_reports`].
///
/// A feed's name stands for its file name: it names the feed in errors
/// and in [`FeedReport`](crate::FeedReport)s and, when the feed has no
/// `feed_info.feed_id`, gives its feed id (the name without a trailing
/// `.zip`), which prefixes its stop ids. Use the names the files have on
/// disk (such as `"poa_eptc.zip"`) and keep them distinct. Errors about
/// the `.pbf` data name it `OpenStreetMap data`. With no feeds, the layer
/// is empty, good for street routing only.
///
/// Feeds that r5r refuses to build a network from are built all the same;
/// [`TransitLayer::r5r_warnings`] says which and why.
///
/// ```no_run
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let pbf = std::fs::read("r/inst/extdata/spo/spo_osm.pbf")?;
/// let gtfs = std::fs::read("r/inst/extdata/spo/spo_gtfs.zip")?;
/// let opts = peabiru_core::BuildOptions::default();
/// let (net, layer) = peabiru_core::build_from_bytes(&pbf, &[("spo_gtfs.zip", &gtfs)], &opts)?;
/// assert_eq!(layer.feed_ids(), ["spo_gtfs"]);
/// # let _ = net;
/// # Ok(())
/// # }
/// ```
pub fn build_from_bytes(
    pbf: &[u8],
    feeds: &[(&str, &[u8])],
    opts: &BuildOptions,
) -> Result<(StreetNetwork, TransitLayer), NetworkError> {
    build(
        Path::new("OpenStreetMap data"),
        Cow::Borrowed(pbf),
        opts,
        || {
            Ok(feeds
                .iter()
                .map(|&(name, bytes)| Ok((PathBuf::from(name), Cow::Borrowed(bytes)))))
        },
    )
}

/// The one way a network is built: the streets from the `.pbf` bytes
/// (errors name it `pbf_name`; the bytes are let go of once read), then
/// the feeds `feeds()` lists, read one after the other, then the stops
/// linked to the streets.
fn build<'a, F>(
    pbf_name: &Path,
    pbf: Cow<'_, [u8]>,
    opts: &BuildOptions,
    feeds: impl FnOnce() -> Result<F, NetworkError>,
) -> Result<(StreetNetwork, TransitLayer), NetworkError>
where
    F: Iterator<Item = Result<(PathBuf, Cow<'a, [u8]>), GtfsError>>,
{
    let mut net = build_network_from_bytes(pbf_name, &pbf, opts)?;
    drop(pbf);
    let mut layer =
        TransitLayer::from_feeds(feeds()?, Limits::default()).map_err(NetworkError::gtfs)?;
    // The only change to a network after it is built, and before anyone
    // can route on it (see `StreetNetwork`).
    layer.link_stops(&mut net);
    Ok((net, layer))
}
