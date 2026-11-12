//! `peabiru info`, `peabiru snap`, `peabiru network` and `peabiru fares`.

use crate::app::{
    CheckNetwork, FaresCommand, Format, InfoArgs, Layer, NetworkArgs, NetworkExportArgs, SnapArgs,
};
use crate::common::*;
use peabiru_core::{
    find_snap, gtfs_errors, setup_fare_structure, street_edges, street_vertices, transit_routes,
    transit_service_dates, transit_stops, write_fare_structure, FareStructure,
};
use std::path::Path;

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn split(d: u32) -> (i64, i64, i64) {
    (
        i64::from(d / 10000),
        i64::from(d / 100 % 100),
        i64::from(d % 100),
    )
}

fn iso(d: u32) -> String {
    format!("{}-{:02}-{:02}", d / 10000, d / 100 % 100, d % 100)
}

pub fn info(a: InfoArgs) -> Res<()> {
    let ctx = Ctx::new("info", a.net.quiet);
    if a.gtfs_errors {
        // get_gtfs_errors(): the feeds as they are, without a build
        let feeds = feed_paths(&a.net)?;
        let mut out = Out::open(a.out.output.as_deref())?;
        out.row(&["file", "line", "type", "field", "id", "priority"])?;
        let mut n = 0;
        for p in &feeds {
            let bytes = std::fs::read(p)
                .map_err(|e| runtime(format!("cannot read {}: {e}", p.display())))?;
            let name = p.file_name().map_or_else(
                || p.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            for e in gtfs_errors(&name, &bytes).map_err(runtime)? {
                out.row(&[
                    e.file().unwrap_or("").to_string(),
                    e.line().to_string(),
                    e.error_type().to_string(),
                    e.field().unwrap_or("").to_string(),
                    e.id().unwrap_or("").to_string(),
                    e.priority().as_str().to_string(),
                ])?;
                n += 1;
            }
        }
        out.finish()?;
        ctx.progress(format!("{n} errors in {} feeds", feeds.len()));
        return Ok(());
    }
    let built = build(&ctx, &a.net)?;
    let net = &built.net;
    let dates: Option<Vec<u32>> = if a.service_dates {
        Some(transit_service_dates(net)?)
    } else if let Some(v) = &a.dates {
        Some(v.iter().map(|d| date("dates", d)).collect::<Res<_>>()?)
    } else if let (Some(s), Some(e)) = (&a.start_date, &a.end_date) {
        let (s, e) = (date("start-date", s)?, date("end-date", e)?);
        if s > e {
            return Err(usage(
                "--start-date must be before or the same as --end-date",
            ));
        }
        let (a0, b0) = (split(s), split(e));
        let (from, to) = (
            days_from_civil(a0.0, a0.1, a0.2),
            days_from_civil(b0.0, b0.1, b0.2),
        );
        Some(
            (from..=to)
                .map(|z| {
                    let (y, m, d) = civil_from_days(z);
                    (y * 10000 + m * 100 + d) as u32
                })
                .collect(),
        )
    } else {
        None
    };
    let mut out = Out::open(a.out.output.as_deref())?;
    if let Some(dates) = dates {
        // check_transit_availability()
        out.row(&["date", "total_services", "active_services", "pct_active"])?;
        for d in dates {
            let (total, active) = match net.transit() {
                Some(t) => (t.n_services(), t.n_services_active_on(d)?),
                None => (0, 0),
            };
            let pct = if total == 0 {
                0.0
            } else {
                active as f64 / total as f64
            };
            out.row(&[iso(d), total.to_string(), active.to_string(), num(pct)])?;
        }
        return out.finish();
    }
    let s = net.street();
    let b = s.bbox();
    let mut text = format!(
        "network: {}\nstreet vertices: {}\nstreet edges: {}\nturn restrictions: {}\n\
         bounding box: {} {} {} {} (xmin ymin xmax ymax)\nelevation: {}\n",
        a.net.network.display(),
        s.n_vertices(),
        s.n_edges(),
        s.n_restrictions(),
        b[0],
        b[1],
        b[2],
        b[3],
        s.elevation().name()
    );
    match net.transit() {
        Some(t) => {
            let dates = transit_service_dates(net)?;
            text.push_str(&format!(
                "feeds: {}\nstops: {} ({} linked to the streets)\ntrip patterns: {}\n\
                 services: {}\nfrequency-based trips: {}\n",
                t.feed_ids().join(", "),
                t.n_stops(),
                t.n_linked_stops(),
                t.n_patterns(),
                t.n_services(),
                if t.has_frequencies() { "yes" } else { "no" }
            ));
            match (dates.first(), dates.last()) {
                (Some(f), Some(l)) => text.push_str(&format!(
                    "service dates: {} to {} ({} dates with service)\n",
                    iso(*f),
                    iso(*l),
                    dates.len()
                )),
                _ => text.push_str("service dates: none\n"),
            }
        }
        None => text.push_str("public transport: none (no GTFS feed)\n"),
    }
    let w = net.build_warnings();
    text.push_str(&format!("warnings: {}\n", w.len()));
    out.write_str(&text)?;
    out.finish()
}

pub fn snap(a: SnapArgs) -> Res<()> {
    let ctx = Ctx::new("snap", a.net.quiet);
    let p = read_points(&ctx, &a.points, "points", true, &[])?;
    let radius = number("radius", &a.radius)?;
    if !radius.is_finite() || radius < 0.0 {
        return Err(usage(format!(
            "--radius must be a finite number >= 0, got '{}'",
            a.radius
        )));
    }
    let mode = street_mode("mode", &a.mode)?;
    let built = build(&ctx, &a.net)?;
    let snaps = find_snap(&built.net, &p.coords, radius, mode)?;
    let out = Out::open(a.out.output.as_deref())?;
    match a.format {
        Format::Csv => {
            let mut out = out;
            out.row(&[
                "point_id", "lat", "lon", "snap_lat", "snap_lon", "distance", "found",
            ])?;
            for (k, s) in snaps.iter().enumerate() {
                let (lat, lon) = p.coords[k];
                out.row(&[
                    p.ids[k].clone(),
                    num(lat),
                    num(lon),
                    s.as_ref().map_or_else(String::new, |s| num(s.lat)),
                    s.as_ref().map_or_else(String::new, |s| num(s.lon)),
                    s.as_ref().map_or_else(String::new, |s| num(s.distance_m)),
                    s.is_some().to_string(),
                ])?;
            }
            out.finish()?;
        }
        Format::Geojson => {
            let mut g = GeoJson::start(out)?;
            for (k, s) in snaps.iter().enumerate() {
                let (lat, lon) = p.coords[k];
                g.feature(
                    &[
                        ("point_id", J::S(&p.ids[k])),
                        ("lat", J::N(lat)),
                        ("lon", J::N(lon)),
                        (
                            "distance",
                            s.as_ref().map_or(J::Null, |s| J::N(s.distance_m)),
                        ),
                        ("found", J::B(s.is_some())),
                    ],
                    s.as_ref()
                        .map_or(Geom::None, |s| Geom::Point([s.lon, s.lat])),
                )?;
            }
            g.finish()?;
        }
    }
    ctx.progress(format!(
        "{} of {} points snapped",
        snaps.iter().filter(|s| s.is_some()).count(),
        snaps.len()
    ));
    Ok(())
}

/// A table written as CSV or GeoJSON: column names, then per row its
/// properties and geometry.
struct Layered {
    csv: Option<Out>,
    gj: Option<GeoJson>,
}

impl Layered {
    fn open(out: Out, format: Format, cols: &[&str], geometry: Option<&str>) -> Res<Layered> {
        Ok(match format {
            Format::Csv => {
                let mut out = out;
                let mut h: Vec<&str> = cols.to_vec();
                if let Some(g) = geometry {
                    h.push(g);
                }
                out.row(&h)?;
                Layered {
                    csv: Some(out),
                    gj: None,
                }
            }
            Format::Geojson => Layered {
                csv: None,
                gj: Some(GeoJson::start(out)?),
            },
        })
    }

    /// One row: `(column, value as CSV text, value as JSON)`.
    fn row(&mut self, props: &[(&str, String, J<'_>)], geom: Option<Geom<'_>>) -> Res<()> {
        if let Some(out) = self.csv.as_mut() {
            let mut rec: Vec<String> = props.iter().map(|p| p.1.clone()).collect();
            if let Some(g) = &geom {
                rec.push(g.wkt());
            }
            out.row(&rec)
        } else {
            let gj = self.gj.as_mut().expect("an output");
            let p: Vec<(&str, J<'_>)> = props
                .iter()
                .map(|(c, _, j)| {
                    let j = match j {
                        J::S(s) => J::S(s),
                        J::N(x) => J::N(*x),
                        J::I(i) => J::I(*i),
                        J::B(b) => J::B(*b),
                        J::Null => J::Null,
                    };
                    (*c, j)
                })
                .collect();
            gj.feature(&p, geom.unwrap_or(Geom::None))
        }
    }

    fn finish(self) -> Res<()> {
        if let Some(o) = self.csv {
            o.finish()?;
        }
        if let Some(g) = self.gj {
            g.finish()?;
        }
        Ok(())
    }
}

/// A line of points: one point is a point, none no geometry (as the
/// packages hold them).
fn line(pts: &[[f64; 2]]) -> Geom<'_> {
    match pts {
        [] => Geom::None,
        [p] => Geom::Point(*p),
        _ => Geom::Line(pts),
    }
}

pub fn network(a: NetworkExportArgs) -> Res<()> {
    let ctx = Ctx::new("network", a.net.quiet);
    let built = build(&ctx, &a.net)?;
    let net = &built.net;
    let out = Out::open(a.out.output.as_deref())?;
    let b = |x: bool| x.to_string();
    let mut n = 0;
    match a.layer {
        Layer::Bbox => {
            let [xmin, ymin, xmax, ymax] = net.street().bbox();
            let mut l = Layered::open(out, a.format, &["xmin", "ymin", "xmax", "ymax"], None)?;
            let ring = vec![vec![
                [xmin, ymin],
                [xmax, ymin],
                [xmax, ymax],
                [xmin, ymax],
                [xmin, ymin],
            ]];
            l.row(
                &[
                    ("xmin", num(xmin), J::N(xmin)),
                    ("ymin", num(ymin), J::N(ymin)),
                    ("xmax", num(xmax), J::N(xmax)),
                    ("ymax", num(ymax), J::N(ymax)),
                ],
                (a.format == Format::Geojson).then_some(Geom::Polygon(ring)),
            )?;
            l.finish()?;
            n = 1;
        }
        Layer::Edges => {
            let e = street_edges(net);
            let cols = [
                "edge_index",
                "osm_id",
                "from_vertex",
                "to_vertex",
                "street_class",
                "length",
                "walk",
                "car",
                "car_speed",
                "bicycle",
                "bicycle_lts",
            ];
            let mut l = Layered::open(out, a.format, &cols, Some("geometry"))?;
            for i in 0..e.len() {
                let class = e.street_class(i);
                l.row(
                    &[
                        (
                            "edge_index",
                            e.edge_index(i).to_string(),
                            J::I(i64::from(e.edge_index(i))),
                        ),
                        ("osm_id", e.osm_id(i).to_string(), J::I(e.osm_id(i))),
                        (
                            "from_vertex",
                            e.from_vertex(i).to_string(),
                            J::I(i64::from(e.from_vertex(i))),
                        ),
                        (
                            "to_vertex",
                            e.to_vertex(i).to_string(),
                            J::I(i64::from(e.to_vertex(i))),
                        ),
                        ("street_class", class.to_string(), J::S(class)),
                        ("length", num(e.length_m(i)), J::N(e.length_m(i))),
                        ("walk", b(e.walk(i)), J::B(e.walk(i))),
                        ("car", b(e.car(i)), J::B(e.car(i))),
                        (
                            "car_speed",
                            num(e.car_speed_kph(i)),
                            J::N(e.car_speed_kph(i)),
                        ),
                        ("bicycle", b(e.bicycle(i)), J::B(e.bicycle(i))),
                        (
                            "bicycle_lts",
                            e.bicycle_lts(i).to_string(),
                            J::I(i64::from(e.bicycle_lts(i))),
                        ),
                    ],
                    Some(line(e.geometry(i))),
                )?;
            }
            n = e.len();
            l.finish()?;
        }
        Layer::Vertices => {
            let v = street_vertices(net);
            let cols = ["index", "park_and_ride", "bike_sharing", "lon", "lat"];
            let csv_cols: &[&str] = if a.format == Format::Csv {
                &cols
            } else {
                &cols[..3]
            };
            let mut l = Layered::open(out, a.format, csv_cols, None)?;
            for k in 0..v.index.len() {
                let mut props = vec![
                    ("index", v.index[k].to_string(), J::I(i64::from(v.index[k]))),
                    (
                        "park_and_ride",
                        b(v.park_and_ride[k]),
                        J::B(v.park_and_ride[k]),
                    ),
                    (
                        "bike_sharing",
                        b(v.bike_sharing[k]),
                        J::B(v.bike_sharing[k]),
                    ),
                ];
                let geom = if a.format == Format::Csv {
                    props.push(("lon", num(v.lon[k]), J::Null));
                    props.push(("lat", num(v.lat[k]), J::Null));
                    None
                } else {
                    Some(Geom::Point([v.lon[k], v.lat[k]]))
                };
                l.row(&props, geom)?;
            }
            n = v.index.len();
            l.finish()?;
        }
        Layer::Stops => {
            let s = transit_stops(net);
            let cols = [
                "stop_index",
                "stop_id",
                "stop_name",
                "linked_to_street",
                "lon",
                "lat",
            ];
            let csv_cols: &[&str] = if a.format == Format::Csv {
                &cols
            } else {
                &cols[..4]
            };
            let mut l = Layered::open(out, a.format, csv_cols, None)?;
            for k in 0..s.stop_index.len() {
                let name = s.stop_name[k].as_deref();
                let mut props = vec![
                    (
                        "stop_index",
                        s.stop_index[k].to_string(),
                        J::I(i64::from(s.stop_index[k])),
                    ),
                    ("stop_id", s.stop_id[k].clone(), J::S(&s.stop_id[k])),
                    ("stop_name", name.unwrap_or("").to_string(), js(name)),
                    (
                        "linked_to_street",
                        b(s.linked_to_street[k]),
                        J::B(s.linked_to_street[k]),
                    ),
                ];
                let geom = if a.format == Format::Csv {
                    props.push(("lon", opt(s.lon[k]), J::Null));
                    props.push(("lat", opt(s.lat[k]), J::Null));
                    None
                } else {
                    match (s.lon[k], s.lat[k]) {
                        (Some(x), Some(y)) => Some(Geom::Point([x, y])),
                        _ => Some(Geom::None),
                    }
                };
                l.row(&props, geom)?;
            }
            n = s.stop_index.len();
            l.finish()?;
        }
        Layer::Routes => {
            let r = transit_routes(net)?;
            let cols = [
                "agency_id",
                "agency_name",
                "route_id",
                "long_name",
                "short_name",
                "mode",
            ];
            let mut l = Layered::open(out, a.format, &cols, Some("geometry"))?;
            for k in 0..r.len() {
                let g = r.geometry(k);
                if g.is_empty() {
                    continue;
                }
                let t = |v: &Option<String>| v.clone().unwrap_or_default();
                l.row(
                    &[
                        (
                            "agency_id",
                            t(&r.agency_id[k]),
                            js(r.agency_id[k].as_deref()),
                        ),
                        (
                            "agency_name",
                            t(&r.agency_name[k]),
                            js(r.agency_name[k].as_deref()),
                        ),
                        ("route_id", r.route_id[k].clone(), J::S(&r.route_id[k])),
                        (
                            "long_name",
                            t(&r.long_name[k]),
                            js(r.long_name[k].as_deref()),
                        ),
                        (
                            "short_name",
                            t(&r.short_name[k]),
                            js(r.short_name[k].as_deref()),
                        ),
                        ("mode", r.mode[k].to_string(), J::S(r.mode[k])),
                    ],
                    Some(line(g)),
                )?;
                n += 1;
            }
            l.finish()?;
        }
    }
    ctx.progress(format!("{n} features"));
    Ok(())
}

/// The network of `--network` (and `--gtfs`), for checking a structure.
fn check_network(ctx: &Ctx, c: &CheckNetwork) -> Res<Option<peabiru_core::Network>> {
    let Some(p) = &c.network else {
        return Ok(None);
    };
    let args = NetworkArgs {
        network: p.clone(),
        gtfs: c.gtfs.clone(),
        tif: None,
        elevation: Some(crate::app::ElevationArg::None),
        ignore_gtfs_errors: false,
        threads: 0,
        quiet: ctx.quiet,
    };
    Ok(Some(build(ctx, &args)?.net))
}

fn encoding(e: &str) -> Res<&str> {
    match e {
        "UTF-8" | "Latin-1" | "unknown" => Ok(e),
        _ => Err(usage(format!(
            "--encoding must be UTF-8, Latin-1 or unknown, got '{e}'"
        ))),
    }
}

fn write_zip(path: &Path, fs: &FareStructure) -> Res<()> {
    if !path.to_string_lossy().ends_with(".zip") {
        return Err(usage(format!(
            "the fare structure file must end in .zip: {}",
            path.display()
        )));
    }
    let bytes = write_fare_structure(fs)?;
    std::fs::write(path, bytes)
        .map_err(|e| runtime(format!("cannot write {}: {e}", path.display())))
}

fn summary(fs: &FareStructure) -> String {
    let mut s = format!(
        "max_discounted_transfers: {}\ntransfer_time_allowance: {} min\nfare_cap: {}\n",
        num(fs.max_discounted_transfers),
        num(fs.transfer_time_allowance),
        num(fs.fare_cap)
    );
    s.push_str(&format!("fare types: {}\n", fs.fares_per_type.len()));
    for t in &fs.fares_per_type {
        s.push_str(&format!(
            "  {}: {}\n",
            t.fare_type.as_deref().unwrap_or("NA"),
            num(t.fare)
        ));
    }
    s.push_str(&format!(
        "discounted transfers: {}\nroutes: {}\n",
        fs.fares_per_transfer.len(),
        fs.fares_per_route.len()
    ));
    s
}

pub fn fares(c: FaresCommand) -> Res<()> {
    match c {
        FaresCommand::Setup(a) => {
            let ctx = Ctx::new("fares setup", a.net.quiet);
            let base = number("base-fare", &a.base_fare)?;
            if !base.is_finite() || base < 0.0 {
                return Err(usage(format!(
                    "--base-fare must be a number >= 0, got '{}'",
                    a.base_fare
                )));
            }
            let by = a.by.trim().to_ascii_uppercase();
            if !["MODE", "AGENCY_ID", "AGENCY_NAME", "GENERIC"].contains(&by.as_str()) {
                return Err(usage(format!(
                    "--by must be MODE, AGENCY_ID, AGENCY_NAME or GENERIC, got '{}'",
                    a.by
                )));
            }
            let built = build(&ctx, &a.net)?;
            let fs = setup_fare_structure(&built.net, base, &by)?;
            write_zip(&a.output, &fs)?;
            ctx.progress(format!(
                "{} fare types, {} routes written to {}",
                fs.fares_per_type.len(),
                fs.fares_per_route.len(),
                a.output.display()
            ));
            Ok(())
        }
        FaresCommand::Read(a) => {
            let ctx = Ctx::new("fares read", false);
            let enc = encoding(&a.encoding)?;
            let bytes = std::fs::read(&a.file)
                .map_err(|e| runtime(format!("cannot read {}: {e}", a.file.display())))?;
            let fs = peabiru_core::read_fare_structure(&bytes, enc)
                .map_err(|e| runtime(format!("{}: {e}", a.file.display())))?;
            fs.validate()?;
            if let Some(net) = check_network(&ctx, &a.check)? {
                fs.validate_for(&net)?;
            }
            if let Some(dir) = &a.output_dir {
                let dir = output_dir(dir)?;
                for (name, text) in peabiru_core::unstable::fare_structure_files(&fs)? {
                    let p = Path::new(&dir).join(name);
                    std::fs::write(&p, text)
                        .map_err(|e| runtime(format!("cannot write {}: {e}", p.display())))?;
                }
                ctx.progress(format!("the structure's files written to {dir}"));
                return Ok(());
            }
            let mut out = Out::open(None)?;
            out.write_str(&summary(&fs))?;
            out.finish()
        }
        FaresCommand::Write(a) => {
            let ctx = Ctx::new("fares write", false);
            let enc = encoding(&a.encoding)?;
            let mut files = Vec::new();
            for name in [
                "global_settings.csv",
                "fares_per_type.csv",
                "fares_per_transfer.csv",
                "fares_per_route.csv",
                "debug_settings.csv",
            ] {
                let p = a.dir.join(name);
                if p.exists() {
                    let b = std::fs::read(&p)
                        .map_err(|e| runtime(format!("cannot read {}: {e}", p.display())))?;
                    files.push((name, b));
                }
            }
            let named: Vec<(&str, &[u8])> = files.iter().map(|(n, b)| (*n, b.as_slice())).collect();
            let zip = peabiru_core::unstable::zip_deflated(&named);
            let fs = peabiru_core::read_fare_structure(&zip, enc)
                .map_err(|e| runtime(format!("{}: {e}", a.dir.display())))?;
            fs.validate()?;
            if let Some(net) = check_network(&ctx, &a.check)? {
                fs.validate_for(&net)?;
            }
            write_zip(&a.output, &fs)?;
            ctx.progress(format!("written to {}", a.output.display()));
            Ok(())
        }
    }
}
