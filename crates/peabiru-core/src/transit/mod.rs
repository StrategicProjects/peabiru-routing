//! Public transport: the transit layer built from GTFS, stop linking,
//! transfers and RAPTOR, following R5 7.6.

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

/// The work of [`crate::build_from_dir`].
pub(crate) fn build_from_dir(
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
    // Of the files of the directory, the last `.pbf` listed is kept, and
    // the last file whose name ends in `.tif` or `.tiff` as the raster.
    let mut pbf = None;
    let mut rasters = Vec::new();
    for e in std::fs::read_dir(dir).map_err(io)? {
        let e = e.map_err(io)?;
        let name = e.file_name();
        if layer::has_suffix(&name, ".pbf") {
            pbf = Some(e.path());
        }
        if layer::has_suffix(&name, ".tif") || layer::has_suffix(&name, ".tiff") {
            rasters.push(e.path());
        }
    }
    let pbf = pbf.ok_or_else(|| NetworkError::no_pbf(dir))?;
    let raster = if opts.elevation == crate::Elevation::None {
        None
    } else {
        raster_in_dir(dir, rasters)?
    };
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
    build(&pbf, Cow::Owned(bytes), raster, opts, || {
        let feeds = gtfs_files_in_dir(dir).map_err(io)?;
        Ok(feeds.into_iter().map(|p| layer::read_feed_file(&p)))
    })
}

/// A file's name and bytes.
type Named = (PathBuf, Cow<'static, [u8]>);

/// The elevation raster of a data directory, read, from the files whose
/// names end in `.tif` or `.tiff` (in lower case): none, or exactly one
/// `.tif` file. Which of several rasters would be read depends on the
/// order the file system lists them in, so more than one is an error; and
/// for a `.tiff` file R5 looks for the same name ending in `.tif` and
/// fails, which is an error here too, as is a `.prj` file of the same
/// name (see below).
fn raster_in_dir(dir: &Path, mut rasters: Vec<PathBuf>) -> Result<Option<Named>, NetworkError> {
    rasters.sort();
    match rasters.as_slice() {
        [] => Ok(None),
        [one] => {
            if layer::has_suffix(one.as_os_str(), ".tiff") {
                return Err(NetworkError::elevation(
                    one,
                    "the name of an elevation raster must end in .tif, not .tiff: rename it to \
                     .tif"
                        .into(),
                ));
            }
            // GeoTools takes the coordinate reference system of a `.prj`
            // file next to the raster over the GeoTIFF's own
            let prj = one.with_extension("prj");
            if prj.exists() {
                return Err(NetworkError::elevation(
                    one,
                    format!(
                        "the raster has a {} file next to it, whose coordinate reference \
                         system would take the place of the GeoTIFF's (as in GeoTools), and which \
                         peabiru does not read; remove it (the GeoTIFF's own system is then \
                         used)",
                        prj.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                    ),
                ));
            }
            let bytes = std::fs::read(one).map_err(|e| {
                NetworkError::io(std::io::Error::new(e.kind(), format!("{}: {e}", one.display())))
            })?;
            Ok(Some((one.clone(), Cow::Owned(bytes))))
        }
        many => Err(NetworkError::elevation(
            dir,
            format!(
                "the directory has {} elevation rasters ({}); only one can be used, so keep exactly one",
                many.len(),
                many.iter()
                    .map(|p| p.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

/// The work of [`crate::build_from_bytes`].
pub(crate) fn build_from_bytes(
    pbf: &[u8],
    feeds: &[(&str, &[u8])],
    opts: &BuildOptions,
) -> Result<(StreetNetwork, TransitLayer), NetworkError> {
    build(
        Path::new("OpenStreetMap data"),
        Cow::Borrowed(pbf),
        None,
        opts,
        || {
            Ok(feeds
                .iter()
                .map(|&(name, bytes)| Ok((PathBuf::from(name), Cow::Borrowed(bytes)))))
        },
    )
}

/// The work of [`crate::build_from_bytes_with_elevation`].
pub(crate) fn build_from_bytes_with_elevation(
    pbf: &[u8],
    feeds: &[(&str, &[u8])],
    raster: &[u8],
    opts: &BuildOptions,
) -> Result<(StreetNetwork, TransitLayer), NetworkError> {
    build(
        Path::new("OpenStreetMap data"),
        Cow::Borrowed(pbf),
        Some((PathBuf::from("elevation raster"), Cow::Borrowed(raster))),
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
    raster: Option<(PathBuf, Cow<'_, [u8]>)>,
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
    // The slope costs are added last, as in R5,
    // when every edge (stop connectors and park-and-rides included) exists.
    if let Some((name, bytes)) = raster {
        if opts.elevation != crate::Elevation::None {
            let threads = if opts.n_threads == 0 {
                crate::sys::cores()
            } else {
                opts.n_threads.min(crate::sys::cores())
            };
            crate::elevation::apply(&mut net, &bytes, opts.elevation, threads)
                .map_err(|m| NetworkError::elevation(&name, m))?;
        }
    }
    Ok((net, layer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtfs::zip::tests::make_zip;

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

    fn build(
        spo: &str,
        feeds: &[(&str, &[u8])],
        ignore: bool,
        threads: usize,
    ) -> Result<usize, NetworkError> {
        let pbf = std::fs::read(format!("{spo}/spo_osm.pbf")).unwrap();
        let opts = BuildOptions {
            n_threads: threads,
            ignore_gtfs_errors: ignore,
            ..BuildOptions::default()
        };
        build_from_bytes(&pbf, feeds, &opts).map(|(_, l)| l.feed_ids().len())
    }

    #[test]
    fn feeds_with_high_priority_errors_are_refused() {
        let Some(spo) = crate::sample_data::spo() else {
            return;
        };
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
            assert_eq!(build(spo, &two, false, threads).unwrap(), 2);
            let four = [
                ("a.zip", &no_agency[..]),
                ("b.zip", &clean[..]),
                ("c.zip", &bad_number[..]),
                ("d.zip", &unknown_stop[..]),
            ];
            let e = build(spo, &four, false, threads).unwrap_err();
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
                 d.zip (1 error); network build failed"
            );
            for f in [&no_agency, &unknown_stop] {
                assert_eq!(build(spo, &[("a.zip", f)], true, threads).unwrap(), 1);
            }
            // without the check, numbers that do not parse are read as
            // gtfs-lib reads them (NaN coordinates: the stops link to no
            // street)
            assert_eq!(
                build(spo, &[("c.zip", &bad_number)], true, threads).unwrap(),
                1
            );
        }
    }

    #[test]
    fn a_feed_gtfs_lib_fails_on_is_an_error() {
        let Some(spo) = crate::sample_data::spo() else {
            return;
        };
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
            let e = build(spo, &[("a.zip", &one_point)], false, threads).unwrap_err();
            assert!(e.high_priority_gtfs_errors().is_none());
            assert!(e.to_string().starts_with("a.zip: shape 'P'"), "{e}");
            // R5 fails on it too, when it draws the shape of the pattern
            let e = build(spo, &[("a.zip", &one_point)], true, threads).unwrap_err();
            assert!(e.to_string().contains("IllegalArgumentException"), "{e}");
        }
    }

    /// Feeds whose errors are only of priority MEDIUM or LOW are built:
    /// values gtfs-lib cannot read take its defaults.
    #[test]
    fn feeds_with_lower_priority_errors_are_read() {
        let Some(spo) = crate::sample_data::spo() else {
            return;
        };
        // a TimeParseError (the time is missing, then interpolated), a
        // missing route_type (0, tram), stop_sequence (0: one stop time per
        // trip) and calendar start (0)
        for changes in [
            &[(
                "stop_times.txt",
                "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
                 T1,14:00,14:00:00,A,1\nT1,14:05:00,14:05:00,B,2\n",
            )][..],
            &[("routes.txt", "route_id\nR\n")],
            &[(
                "stop_times.txt",
                "trip_id,arrival_time,departure_time,stop_id\n\
                 T1,14:00:00,14:00:00,A\nT1,14:05:00,14:05:00,B\n",
            )],
            &[(
                "calendar.txt",
                "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,\
                 start_date,end_date\nS,1,1,1,1,1,1,1,,20191231\n",
            )],
        ] {
            for threads in [1, 2] {
                assert_eq!(
                    build(spo, &[("a.zip", &feed(changes))], false, threads).unwrap(),
                    1
                );
            }
        }
        // a trip without route_id: gtfs-lib records an EmptyFieldError
        // (MEDIUM), then R5 fails on it
        let no_route = feed(&[("trips.txt", "route_id,service_id,trip_id\n,S,T1\n")]);
        let e = build(spo, &[("a.zip", &no_route)], false, 1).unwrap_err();
        assert_eq!(
            e.to_string(),
            "a.zip: trip 'T1' has no route_id, and R5 fails building its transit layer from the \
             feed (NullPointerException)"
        );
    }
}
