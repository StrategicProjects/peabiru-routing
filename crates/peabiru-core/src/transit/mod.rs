//! Public transport: the transit layer built from GTFS, stop linking,
//! transfers and RAPTOR, following R5 7.6 as used by r5r.

pub(crate) mod accessibility;
pub(crate) mod decay;
pub(crate) mod egress;
pub(crate) mod expanded;
pub(crate) mod layer;
pub(crate) mod matrix;
pub(crate) mod mcraptor;
pub(crate) mod pareto;
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
/// As r5r does, the build fails when a feed has GTFS errors of priority
/// `HIGH`, or is one gtfs-lib cannot load at all
/// ([`NetworkError::high_priority_gtfs_errors`],
/// [`gtfs_errors`](crate::gtfs_errors)), unless
/// [`BuildOptions::ignore_gtfs_errors`] is set.
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
/// As r5r does, the build fails when a feed has GTFS errors of priority
/// `HIGH`, or is one gtfs-lib cannot load at all
/// ([`NetworkError::high_priority_gtfs_errors`],
/// [`gtfs_errors`](crate::gtfs_errors)), unless
/// [`BuildOptions::ignore_gtfs_errors`] is set.
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
    let check = if opts.ignore_gtfs_errors {
        layer::FeedCheck::None
    } else if opts.n_threads != 1 && crate::sys::cores() > 1 {
        layer::FeedCheck::Parallel
    } else {
        layer::FeedCheck::Sequential
    };
    let mut layer =
        TransitLayer::from_feeds(feeds()?, Limits::default(), check).map_err(NetworkError::gtfs)?;
    // The only change to a network after it is built, and before anyone
    // can route on it (see `StreetNetwork`).
    layer.link_stops(&mut net);
    Ok((net, layer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtfs::zip::tests::make_zip;

    const PBF: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../r/inst/extdata/spo/spo_osm.pbf"
    );

    fn feed(changes: &[(&str, &str)]) -> Vec<u8> {
        let mut files = vec![
            (
                "agency.txt",
                "agency_id,agency_name,agency_url,agency_timezone\n\
                 A,A,http://a,America/Sao_Paulo\n",
            ),
            (
                "stops.txt",
                "stop_id,stop_name,stop_lat,stop_lon\n\
                 A,A,-23.5563,-46.6290\nB,B,-23.5577,-46.6321\n",
            ),
            ("routes.txt", "route_id,route_type\nR,3\n"),
            ("trips.txt", "route_id,service_id,trip_id\nR,S,T1\n"),
            (
                "stop_times.txt",
                "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
                 T1,14:00:00,14:00:00,A,1\nT1,14:05:00,14:05:00,B,2\n",
            ),
            (
                "calendar.txt",
                "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,\
                 start_date,end_date\nS,1,1,1,1,1,1,1,20190101,20191231\n",
            ),
        ];
        for &(name, text) in changes {
            files.retain(|f| f.0 != name);
            if !text.is_empty() {
                files.push((name, text));
            }
        }
        let files: Vec<(&str, &[u8])> = files.iter().map(|(n, t)| (*n, t.as_bytes())).collect();
        make_zip(&files, true)
    }

    fn build(feeds: &[(&str, &[u8])], ignore: bool, threads: usize) -> Result<usize, NetworkError> {
        let pbf = std::fs::read(PBF).unwrap();
        let opts = BuildOptions {
            n_threads: threads,
            ignore_gtfs_errors: ignore,
        };
        build_from_bytes(&pbf, feeds, &opts).map(|(_, l)| l.feed_ids().len())
    }

    #[test]
    fn feeds_with_high_priority_errors_are_refused_as_in_r5r() {
        let clean = feed(&[]);
        // a duplicated stop is MEDIUM, an agency URL that is not one LOW
        let medium = feed(&[
            (
                "stops.txt",
                "stop_id,stop_name,stop_lat,stop_lon\nA,A,-23.5563,-46.6290\n\
                 B,B,-23.5577,-46.6321\nB,B,-23.5577,-46.6321\n",
            ),
            (
                "agency.txt",
                "agency_id,agency_name,agency_url,agency_timezone\nA,A,www.a,America/Sao_Paulo\n",
            ),
        ]);
        let no_agency = feed(&[("agency.txt", "")]);
        let bad_number = feed(&[(
            "stops.txt",
            "stop_id,stop_name,stop_lat,stop_lon\nA,A,x,-46.6290\nB,B,-23.5577,y\n",
        )]);
        let unknown_stop = feed(&[(
            "stop_times.txt",
            "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
             T1,14:00:00,14:00:00,A,1\nT1,14:05:00,14:05:00,Z,2\n",
        )]);
        for threads in [1, 2] {
            let two = [("a.zip", &clean[..]), ("b.zip", &medium[..])];
            assert_eq!(build(&two, false, threads).unwrap(), 2);
            let four = [
                ("a.zip", &no_agency[..]),
                ("b.zip", &clean[..]),
                ("c.zip", &bad_number[..]),
                ("d.zip", &unknown_stop[..]),
            ];
            let e = build(&four, false, threads).unwrap_err();
            let found: Vec<(String, usize)> = e
                .high_priority_gtfs_errors()
                .unwrap()
                .iter()
                .map(|(p, n)| (p.display().to_string(), *n))
                .collect();
            assert_eq!(
                found,
                [
                    ("a.zip".into(), 1),
                    ("c.zip".into(), 2),
                    ("d.zip".into(), 1)
                ],
                "{threads} threads"
            );
            assert_eq!(
                e.to_string(),
                "high priority GTFS errors found in a.zip (1 error), c.zip (2 errors), \
                 d.zip (1 error); network build failed (as in r5r)"
            );
            for f in [&no_agency, &unknown_stop] {
                assert_eq!(build(&[("a.zip", f)], true, threads).unwrap(), 1);
            }
            // without the check, peabiru's own reading of the numbers fails
            let e = build(&[("c.zip", &bad_number)], true, threads).unwrap_err();
            assert!(e.high_priority_gtfs_errors().is_none());
        }
    }

    #[test]
    fn a_feed_gtfs_lib_fails_on_is_an_error() {
        let one_point = feed(&[
            (
                "trips.txt",
                "route_id,service_id,trip_id,shape_id\nR,S,T1,P\n",
            ),
            (
                "shapes.txt",
                "shape_id,shape_pt_lat,shape_pt_lon,shape_pt_sequence\nP,-23.55,-46.63,1\n",
            ),
        ]);
        for threads in [1, 2] {
            let e = build(&[("a.zip", &one_point)], false, threads).unwrap_err();
            assert!(e.high_priority_gtfs_errors().is_none());
            assert!(e.to_string().starts_with("a.zip: shape 'P'"), "{e}");
            assert!(build(&[("a.zip", &one_point)], true, threads).is_ok());
        }
    }

    #[test]
    fn a_feed_r5r_builds_and_peabiru_cannot_read_says_so() {
        // a TimeParseError is MEDIUM: r5r builds the network
        let short_time = feed(&[(
            "stop_times.txt",
            "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
             T1,14:00,14:00:00,A,1\nT1,14:05:00,14:05:00,B,2\n",
        )]);
        for threads in [1, 2] {
            let e = build(&[("a.zip", &short_time)], false, threads).unwrap_err();
            assert_eq!(
                e.to_string(),
                "a.zip: stop_times.txt line 2: invalid arrival_time '14:00' (r5r builds a \
                 network from this feed, whose GTFS errors are not of priority HIGH, but \
                 peabiru cannot read it)"
            );
            let e = build(&[("a.zip", &short_time)], true, threads).unwrap_err();
            assert_eq!(
                e.to_string(),
                "a.zip: stop_times.txt line 2: invalid arrival_time '14:00'"
            );
        }
    }
}
