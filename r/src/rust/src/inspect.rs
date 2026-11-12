//! Bindings of r5r's network functions: `find_snap()`,
//! `street_network_bbox()`, `street_network_to_sf()`,
//! `transit_network_to_sf()`, `check_transit_availability()` and
//! `get_gtfs_errors()`. As in the rest of the package, the R code checks
//! the arguments and assembles the result; these only hand back columns.

use super::{err, network, points, street_mode};
use extendr_api::prelude::*;
use extendr_api::Result;
use peabiru_core::{
    find_snap, gtfs_errors, gtfs_files_in_dir, street_vertices, transit_routes,
    transit_service_dates, transit_stops,
};

/// Snapped points of r5r's `find_snap()`: `snap_lat`, `snap_lon`,
/// `distance` (NA where not found) and `found`, one per point.
/// @noRd
#[extendr]
fn rs_find_snap(ptr: Robj, lat: &[f64], lon: &[f64], radius: f64, mode: &str) -> Result<Robj> {
    let n = network(&ptr)?;
    let snaps = find_snap(&n.net, &points(lat, lon), radius, street_mode(mode)?).map_err(err)?;
    let col = |f: fn(&peabiru_core::Snap) -> f64| -> Vec<Option<f64>> {
        snaps.iter().map(|s| s.as_ref().map(f)).collect()
    };
    Ok(list!(
        snap_lat = col(|s| s.lat),
        snap_lon = col(|s| s.lon),
        distance = col(|s| s.distance_m),
        found = snaps.iter().map(|s| s.is_some()).collect::<Vec<_>>()
    )
    .into())
}

/// `c(xmin, ymin, xmax, ymax)` of the street network (R5's envelope).
/// @noRd
#[extendr]
fn rs_street_bbox(ptr: Robj) -> Result<Vec<f64>> {
    Ok(network(&ptr)?.net.street().bbox().to_vec())
}

/// The vertices of r5r's `street_network_to_sf()`, as columns.
/// @noRd
#[extendr]
fn rs_street_vertices(ptr: Robj) -> Result<Robj> {
    let v = street_vertices(&network(&ptr)?.net);
    Ok(list!(
        index = v.index.iter().map(|&i| i as i32).collect::<Vec<_>>(),
        lat = v.lat,
        lon = v.lon,
        park_and_ride = v.park_and_ride,
        bike_sharing = v.bike_sharing
    )
    .into())
}

/// The stops of r5r's `transit_network_to_sf()`, as columns (NA
/// coordinates for stops not linked to the streets).
/// @noRd
#[extendr]
fn rs_transit_stops(ptr: Robj) -> Result<Robj> {
    let n = network(&ptr)?;
    if n.net.transit().is_none() {
        return Ok(list!(
            stop_index = Vec::<i32>::new(),
            stop_id = Vec::<String>::new(),
            stop_name = Vec::<String>::new(),
            lat = Vec::<f64>::new(),
            lon = Vec::<f64>::new(),
            linked_to_street = Vec::<bool>::new()
        )
        .into());
    }
    let s = transit_stops(&n.net);
    Ok(list!(
        stop_index = s.stop_index.iter().map(|&i| i as i32).collect::<Vec<_>>(),
        stop_id = s.stop_id,
        stop_name = s.stop_name,
        lat = s.lat,
        lon = s.lon,
        linked_to_street = s.linked_to_street
    )
    .into())
}

/// The routes of r5r's `transit_network_to_sf()`, as columns, with the line
/// of each row as `n_points` and flat `x`, `y`.
/// @noRd
#[extendr]
fn rs_transit_routes(ptr: Robj) -> Result<Robj> {
    let n = network(&ptr)?;
    let r = match n.net.transit() {
        Some(_) => transit_routes(&n.net).map_err(err)?,
        None => Default::default(),
    };
    let (mut n_points, mut x, mut y) = (Vec::with_capacity(r.len()), Vec::new(), Vec::new());
    for i in 0..r.len() {
        let g = r.geometry(i);
        n_points.push(g.len() as i32);
        for &[px, py] in g {
            x.push(px);
            y.push(py);
        }
    }
    Ok(list!(
        agency_id = r.agency_id.clone(),
        agency_name = r.agency_name.clone(),
        route_id = r.route_id.clone(),
        long_name = r.long_name.clone(),
        short_name = r.short_name.clone(),
        mode = r.mode.clone(),
        n_points = n_points,
        x = x,
        y = y
    )
    .into())
}

/// The operating dates of the network's services (`yyyymmdd`, ascending),
/// as r5r's `check_transit_availability()` discovers them.
/// @noRd
#[extendr]
fn rs_transit_service_dates(ptr: Robj) -> Result<Vec<i32>> {
    let n = network(&ptr)?;
    Ok(transit_service_dates(&n.net)
        .map_err(err)?
        .into_iter()
        .map(|d| d as i32)
        .collect())
}

/// The paths of the network's GTFS feeds, in loading order.
/// @noRd
#[extendr]
fn rs_feed_paths(ptr: Robj) -> Result<Vec<String>> {
    let n = network(&ptr)?;
    Ok(n.net.transit().map_or_else(Vec::new, |t| {
        t.feed_reports()
            .iter()
            .map(|f| f.path.to_string_lossy().into_owned())
            .collect()
    }))
}

/// The GTFS feeds of a directory, in the order R5 loads them.
/// @noRd
#[extendr]
fn rs_gtfs_files(dir: &str) -> Result<Vec<String>> {
    Ok(gtfs_files_in_dir(dir)
        .map_err(|e| err(format!("{dir}: {e}")))?
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

/// The errors gtfs-lib reports for the feeds at `paths`, read in this
/// order, as the columns of r5r's `gtfs_errors.csv` (`line` NA never).
/// @noRd
#[extendr]
fn rs_gtfs_errors(paths: Vec<String>) -> Result<Robj> {
    let (mut file, mut line, mut kind, mut field, mut id, mut priority) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    for p in &paths {
        let bytes = std::fs::read(p).map_err(|e| err(format!("{p}: {e}")))?;
        let name = std::path::Path::new(p)
            .file_name()
            .map_or_else(|| p.clone(), |n| n.to_string_lossy().into_owned());
        for e in gtfs_errors(&name, &bytes).map_err(err)? {
            file.push(e.file().map(str::to_string));
            line.push(e.line());
            kind.push(e.error_type().to_string());
            field.push(e.field().map(str::to_string));
            id.push(e.id().map(str::to_string));
            priority.push(e.priority().as_str());
        }
    }
    let values: [Robj; 6] = [
        file.into(),
        line.into(),
        kind.into(),
        field.into(),
        id.into(),
        priority.into(),
    ];
    Ok(
        List::from_names_and_values(["file", "line", "type", "field", "id", "priority"], values)?
            .into(),
    )
}

extendr_module! {
    mod inspect;
    fn rs_find_snap;
    fn rs_street_bbox;
    fn rs_street_vertices;
    fn rs_transit_stops;
    fn rs_transit_routes;
    fn rs_transit_service_dates;
    fn rs_feed_paths;
    fn rs_gtfs_files;
    fn rs_gtfs_errors;
}
