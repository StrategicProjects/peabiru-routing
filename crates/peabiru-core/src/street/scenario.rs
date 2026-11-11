//! Scenarios: the car speeds and levels of traffic stress of a street
//! network changed for one routing call, as r5r 2.5.0 changes them with
//! the arguments `new_carspeeds`, `carspeed_scale` and `new_lts`.
//!
//! r5r applies them to a scenario copy of R5's network (`RoutingProperties
//! .getTransportNetworkForScenario`) with four modifications, outside R5's
//! scenario machinery, so that nothing is rebuilt afterwards:
//!
//! - car speeds by OSM way (`RoadCongestionOSM`): each directed edge whose
//!   way is listed gets its value, the others `carspeed_scale`; a value of
//!   0 forbids cars on the edge, a speed in km/h replaces the speed, a
//!   factor scales it (`(short)` of a `float` product);
//! - car speeds by polygon (R5 `RoadCongestion`): each directed edge takes
//!   the factor of the polygon it intersects with the highest priority
//!   (ties: the longest overlap), `carspeed_scale` outside every polygon
//!   (`(short)` of a `double` product; a factor of 0 leaves cars allowed);
//! - levels of traffic stress by OSM way (`SetLtsOsm`) and by line
//!   (`R5RShapefileLts`, `ShapefileMatcher`: each edge pair takes the level
//!   of the line closest to it by discrete Hausdorff distance among those
//!   whose bounding box meets the edge's).
//!
//! Connectors (transit stops, park-and-ride) are edges of OSM way -1, as
//! in R5. The scenario network is then used where r5r uses it: the street
//! searches from the origins, the links of the origins, and the speed of
//! the destination's edge. The destinations stay linked as on the base
//! network (R5 copies the base linkage), the egress tables from the stops
//! are the base network's (R5 copies them too) and so are the walking
//! trees and transfers.

mod jts;

use super::flags::*;
use super::geom::FIXED_FACTOR;
use super::StreetNetwork;
use crate::{Polygon, RouteError};
use jts::{Env, Pt, StrTree};
use std::collections::HashMap;

/// r5r's `new_carspeeds`.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub enum CarSpeeds {
    /// No change (`new_carspeeds = NULL`): `carspeed_scale` alone scales
    /// every edge.
    #[default]
    Unchanged,
    /// A data frame with `speed_type = "scale"`: per OSM way id, a factor
    /// for the car speed of its edges (0 forbids cars). A repeated id
    /// keeps its last value.
    ScaleByWay(Vec<(i64, f32)>),
    /// A data frame with `speed_type = "km/h"`: per OSM way id, the car
    /// speed of its edges in km/h (0 forbids cars). A repeated id keeps its
    /// last value.
    KmhByWay(Vec<(i64, f32)>),
    /// An `sf` data frame of polygons: each edge takes the factor of the
    /// polygon it meets with the highest priority.
    ByPolygon(Vec<SpeedPolygon>),
}

/// A polygon of r5r's `new_carspeeds` as an `sf` data frame: a
/// multipolygon in `[lon, lat]` degrees, a factor for the car speeds of
/// the edges it meets and a priority. r5r hands the polygons to R5 as
/// GeoJSON, written by GDAL with 15 decimals of each coordinate (and a
/// tail that looks like round-off trimmed): give the coordinates as R5
/// reads them back (the R package makes the same round trip; the Python
/// and JavaScript packages round to 15 decimals).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SpeedPolygon {
    /// The polygons (each a shell then its holes, every ring closed), in
    /// `[lon, lat]` degrees.
    pub polygons: Vec<Polygon>,
    /// The factor (`scale`).
    pub scale: f64,
    /// The priority (`priority`): the highest wins.
    pub priority: f64,
}

impl SpeedPolygon {
    /// A polygon of `new_carspeeds`.
    pub fn new(polygons: Vec<Polygon>, scale: f64, priority: f64) -> Self {
        SpeedPolygon {
            polygons,
            scale,
            priority,
        }
    }
}

/// r5r's `new_lts`.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub enum LtsChange {
    /// No change (`new_lts = NULL`).
    #[default]
    Unchanged,
    /// A data frame: per OSM way id, the level of traffic stress of its
    /// edges (below 2 is 1, above 4 is 4, 0 leaves the edge as it is). A
    /// repeated id keeps its last value.
    ByWay(Vec<(i64, i32)>),
    /// An `sf` data frame of lines: each edge pair takes the level of the
    /// line closest to it.
    ByLine(Vec<LtsLine>),
}

/// A line of r5r's `new_lts` as an `sf` data frame, in `[lon, lat]`
/// degrees, and its level of traffic stress.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LtsLine {
    /// The points of the line, `[lon, lat]` degrees.
    pub points: Vec<[f64; 2]>,
    /// The level of traffic stress (below 2 is 1, above 4 is 4).
    pub lts: i32,
}

impl LtsLine {
    /// A line of `new_lts`.
    pub fn new(points: Vec<[f64; 2]>, lts: i32) -> Self {
        LtsLine { points, lts }
    }
}

/// Changes to the street network for one routing call: r5r's
/// `new_carspeeds`, `carspeed_scale` and `new_lts`. The default changes
/// nothing. Fill it in by changing the fields of [`Scenario::default()`].
///
/// ```
/// use peabiru_core::{CarSpeeds, Scenario};
/// let mut s = Scenario::default();
/// s.car_speeds = CarSpeeds::ScaleByWay(vec![(27184648, 0.5)]);
/// s.car_speed_scale = 0.8; // every other edge
/// let mut p = peabiru_core::RouteParams::default();
/// p.scenario = s;
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Scenario {
    /// New car speeds (`new_carspeeds`).
    pub car_speeds: CarSpeeds,
    /// The factor for the car speeds of the edges `car_speeds` does not
    /// change (`carspeed_scale`, default 1). R5 reads it as a `float`.
    pub car_speed_scale: f32,
    /// New levels of traffic stress (`new_lts`).
    pub lts: LtsChange,
}

impl Default for Scenario {
    fn default() -> Self {
        Scenario {
            car_speeds: CarSpeeds::Unchanged,
            car_speed_scale: 1.0,
            lts: LtsChange::Unchanged,
        }
    }
}

impl Scenario {
    /// Whether the scenario changes nothing, so that r5r routes on the
    /// network as built (no `new_carspeeds`, `carspeed_scale` 1, no
    /// `new_lts`).
    pub fn is_unchanged(&self) -> bool {
        !self.changes_car_speeds() && self.lts == LtsChange::Unchanged
    }

    /// r5r `set_new_congestion`: `!is.null(new_carspeeds) || carspeed_scale != 1`.
    fn changes_car_speeds(&self) -> bool {
        self.car_speeds != CarSpeeds::Unchanged || self.car_speed_scale != 1.0
    }

    /// Check the values (r5r's checks where the core sees them): the
    /// factor finite and at least 0, no NaN, rings of at least 4 points
    /// and closed, lines of at least 2 points, finite coordinates.
    pub(crate) fn check(&self) -> Result<(), String> {
        let s = self.car_speed_scale;
        if !s.is_finite() || s < 0.0 {
            return Err(format!(
                "carspeed_scale must be a finite number of at least 0, got {s}"
            ));
        }
        match &self.car_speeds {
            CarSpeeds::Unchanged => {}
            CarSpeeds::ScaleByWay(v) | CarSpeeds::KmhByWay(v) => {
                if let Some((id, _)) = v.iter().find(|(_, x)| x.is_nan()) {
                    return Err(format!(
                        "new_carspeeds: the max_speed of OSM way {id} is NaN"
                    ));
                }
            }
            CarSpeeds::ByPolygon(polys) => {
                for (i, p) in polys.iter().enumerate() {
                    if p.scale.is_nan() || p.priority.is_nan() {
                        return Err(format!(
                            "new_carspeeds: the scale or priority of polygon {} is NaN",
                            i + 1
                        ));
                    }
                    let finite = p
                        .polygons
                        .iter()
                        .flatten()
                        .flatten()
                        .all(|c| c[0].is_finite() && c[1].is_finite());
                    let rings_ok = p
                        .polygons
                        .iter()
                        .flatten()
                        .all(|r| r.len() >= 4 && r.first() == r.last());
                    if !finite || !rings_ok {
                        return Err(format!(
                            "new_carspeeds: polygon {} must have finite coordinates and closed \
                             rings of at least 4 points",
                            i + 1
                        ));
                    }
                    if !p.polygons.iter().all(|poly| jts::polygon_is_simple(poly)) {
                        return Err(format!(
                            "new_carspeeds: polygon {} is not valid (a ring crosses or touches \
                             itself or another ring, or a hole is outside its shell); R5's \
                             geometry library may fail on it or give results peabiru cannot \
                             reproduce, so it is refused (sf::st_make_valid() can repair it)",
                            i + 1
                        ));
                    }
                }
            }
        }
        if let LtsChange::ByLine(lines) = &self.lts {
            for (i, l) in lines.iter().enumerate() {
                if l.points.len() < 2
                    || !l
                        .points
                        .iter()
                        .all(|c| c[0].is_finite() && c[1].is_finite())
                {
                    return Err(format!(
                        "new_lts: line {} must have at least 2 points, with finite coordinates",
                        i + 1
                    ));
                }
            }
        }
        Ok(())
    }

    /// The scenario network: a copy of `base` with the car speeds and
    /// levels of traffic stress changed, or `None` when the scenario
    /// changes nothing. The copy keeps no contraction hierarchy or grid
    /// linkage of `base`.
    pub(crate) fn apply(&self, base: &StreetNetwork) -> Result<Option<StreetNetwork>, RouteError> {
        self.check().map_err(RouteError::Invalid)?;
        if self.is_unchanged() {
            return Ok(None);
        }
        let mut net = base.clone();
        net.forget_kept();
        // R5's scenario copy of the edges (`EdgeStore.extendOnlyCopy`, made
        // by r5r's `getTransportNetworkForScenario`) leaves out the cost
        // fields: walking and cycling lose their slope costs (r5r warns of
        // this, issue #555).
        net.edge_slope = Vec::new();
        net.elevation = crate::Elevation::None;
        if self.changes_car_speeds() {
            match &self.car_speeds {
                CarSpeeds::ByPolygon(polys) => {
                    speeds_by_polygon(&mut net, polys, f64::from(self.car_speed_scale))?
                }
                CarSpeeds::Unchanged => speeds_by_way(&mut net, &[], false, self.car_speed_scale),
                CarSpeeds::ScaleByWay(v) => speeds_by_way(&mut net, v, false, self.car_speed_scale),
                CarSpeeds::KmhByWay(v) => speeds_by_way(&mut net, v, true, self.car_speed_scale),
            }
        }
        match &self.lts {
            LtsChange::Unchanged => {}
            LtsChange::ByWay(v) => lts_by_way(&mut net, v),
            LtsChange::ByLine(lines) => lts_by_line(&mut net, lines),
        }
        Ok(Some(net))
    }
}

/// Java's `Long.parseLong`: an optional sign and decimal digits, nothing
/// else (r5r hands R5 the ids as `as.character()` text, so a number R
/// writes as `1e+05` is refused).
pub fn java_parse_long(s: &str) -> Option<i64> {
    let digits = s.strip_prefix(['-', '+']).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Java's `Integer.parseInt`.
pub fn java_parse_int(s: &str) -> Option<i32> {
    java_parse_long(s).and_then(|v| i32::try_from(v).ok())
}

/// Java's `Float.parseFloat` for decimal text: blanks around it trimmed,
/// an optional sign, digits with an optional point and exponent, an
/// optional `f`/`d` suffix; `NaN` and `Infinity` as Java writes them (R's
/// `Inf` is refused, as in Java). Correctly rounded, as Java's.
pub fn java_parse_float(s: &str) -> Option<f32> {
    let t = s.trim_matches(|c: char| c <= ' ');
    let body = t.strip_prefix(['-', '+']).unwrap_or(t);
    match body {
        "NaN" => return Some(f32::NAN),
        "Infinity" => {
            return Some(if t.starts_with('-') {
                f32::NEG_INFINITY
            } else {
                f32::INFINITY
            })
        }
        _ => {}
    }
    let t = t.strip_suffix(['f', 'F', 'd', 'D']).unwrap_or(t);
    let body = t.strip_prefix(['-', '+']).unwrap_or(t);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let digits = mantissa.bytes().filter(u8::is_ascii_digit).count();
    let mantissa_ok = digits > 0
        && mantissa.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && mantissa.bytes().filter(|&b| b == b'.').count() <= 1;
    let exponent_ok = match exponent {
        None => true,
        Some(e) => {
            let d = e.strip_prefix(['-', '+']).unwrap_or(e);
            !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())
        }
    };
    if !mantissa_ok || !exponent_ok {
        return None;
    }
    t.parse().ok()
}

/// The OSM way id R5 gives directed edge `e`: -1 for a connector.
fn osm_id(net: &StreetNetwork, e: usize) -> i64 {
    if net.edge_flags[e] & LINK != 0 {
        -1
    } else {
        net.pair_way[e / 2]
    }
}

/// R5 `intToLts`: below 2 is 1, above 4 is 4.
fn lts_flag(lts: i32) -> u32 {
    match lts {
        i32::MIN..=1 => BIKE_LTS_1,
        2 => BIKE_LTS_2,
        3 => BIKE_LTS_3,
        _ => BIKE_LTS_4,
    }
}

/// `Edge.setFlag(BIKE_LTS_EXPLICIT)` and `Edge.setLts(lts)`.
fn set_lts(net: &mut StreetNetwork, e: usize, lts: i32) {
    let f = &mut net.edge_flags[e];
    *f = (*f & !LTS_MASK) | BIKE_LTS_EXPLICIT | lts_flag(lts);
}

/// r5r `RoadCongestionOSM.apply`.
fn speeds_by_way(net: &mut StreetNetwork, values: &[(i64, f32)], kmh: bool, default: f32) {
    let map: HashMap<i64, f32> = values.iter().copied().collect();
    for e in 0..net.n_edges() {
        let value = map.get(&osm_id(net, e)).copied();
        let scaling = value.unwrap_or(default);
        if scaling == 0.0 {
            net.edge_flags[e] &= !ALLOWS_CAR;
        } else if let (Some(v), true) = (value, kmh) {
            // Edge.setSpeedKph(double): (short)(kph / 3.6 * 100)
            net.edge_speed_cms[e] = (f64::from(v) / 3.6 * 100.0) as i32 as i16;
        } else {
            // (short)(edge.getSpeed() * scaling), a float product
            net.edge_speed_cms[e] = (f32::from(net.edge_speed_cms[e]) * scaling) as i32 as i16;
        }
    }
}

/// The points of directed edge `e`, `[lon, lat]` degrees in its direction
/// (R5 `Edge.getGeometry`).
fn edge_points(net: &StreetNetwork, e: usize, out: &mut Vec<Pt>) {
    out.clear();
    out.extend(
        net.pair_points(e / 2)
            .map(|(lat, lon)| [f64::from(lon) / FIXED_FACTOR, f64::from(lat) / FIXED_FACTOR]),
    );
    if e % 2 == 1 {
        out.reverse();
    }
}

/// R5 `RoadCongestion.apply`, with the polygons as r5r writes them
/// (`scale`, `priority`) and `default` (`carspeed_scale`) outside them.
fn speeds_by_polygon(
    net: &mut StreetNetwork,
    polys: &[SpeedPolygon],
    default: f64,
) -> Result<(), RouteError> {
    let envs: Vec<Option<Env>> = polys
        .iter()
        .map(|p| {
            let pts: Vec<Pt> = p.polygons.iter().flatten().flatten().copied().collect();
            Env::of(&pts)
        })
        .collect();
    let tree = StrTree::build(&envs);
    let mut line = Vec::new();
    let mut found = Vec::new();
    for e in 0..net.n_edges() {
        edge_points(net, e, &mut line);
        let env = Env::of(&line).expect("an edge has two points");
        tree.query(&env, &mut found);
        let mut winner: Option<usize> = None;
        for &c in &found {
            let cand = &polys[c];
            if !jts::polygon_intersects_line(&cand.polygons, &line) {
                continue;
            }
            match winner {
                None => winner = Some(c),
                Some(w) if cand.priority > polys[w].priority => winner = Some(c),
                Some(w) if cand.priority == polys[w].priority && cand.scale != polys[w].scale => {
                    // both hold the whole edge: the same length, the first wins
                    if jts::line_in_interior(&polys[w].polygons, &line)
                        && jts::line_in_interior(&cand.polygons, &line)
                    {
                        continue;
                    }
                    let lw = jts::intersection_length(&polys[w].polygons, &line);
                    let lc = jts::intersection_length(&cand.polygons, &line);
                    if (lw - lc).abs() <= 1e-9 * lw.max(lc) {
                        return Err(RouteError::Invalid(format!(
                            "new_carspeeds: polygons {} and {} have the same priority, different \
                             scales and overlap OSM way {} by the same length; R5 settles such a \
                             tie by a computation peabiru does not reproduce exactly, so the \
                             scenario is not supported (give one of them a higher priority)",
                            w + 1,
                            c + 1,
                            osm_id(net, e)
                        )));
                    }
                    if lc > lw {
                        winner = Some(c);
                    }
                }
                Some(_) => {}
            }
        }
        let scale = winner.map_or(default, |w| polys[w].scale);
        // (short)(edge.getSpeed() * winner.scale), a double product
        net.edge_speed_cms[e] = (f64::from(net.edge_speed_cms[e]) * scale) as i32 as i16;
    }
    Ok(())
}

/// r5r `SetLtsOsm.apply`.
fn lts_by_way(net: &mut StreetNetwork, values: &[(i64, i32)]) {
    let map: HashMap<i64, i32> = values.iter().copied().collect();
    for e in 0..net.n_edges() {
        let lts = map.get(&osm_id(net, e)).copied().unwrap_or(0);
        if lts != 0 {
            set_lts(net, e, lts);
        }
    }
}

/// R5 `ShapefileMatcher.match` as r5r runs it (`R5RShapefileLts`).
fn lts_by_line(net: &mut StreetNetwork, lines: &[LtsLine]) {
    let envs: Vec<Option<Env>> = lines.iter().map(|l| Env::of(&l.points)).collect();
    let tree = StrTree::build(&envs);
    let mut edge = Vec::new();
    let mut found = Vec::new();
    for k in 0..net.n_edges() / 2 {
        edge_points(net, 2 * k, &mut edge);
        let env = Env::of(&edge).expect("an edge has two points");
        tree.query(&env, &mut found);
        // ShapefileMatcher.findBestMatch: the first of equal distances wins
        let (mut best, mut best_distance) = (None, f64::INFINITY);
        for &c in &found {
            let d = jts::discrete_hausdorff(&lines[c].points, &edge);
            if best_distance > d {
                best_distance = d;
                best = Some(c);
            }
        }
        if let Some(c) = best {
            set_lts(net, 2 * k, lines[c].lts);
            set_lts(net, 2 * k + 1, lines[c].lts);
        }
    }
}

/// The network the street searches of a call with scenario `s` run on
/// ([`Scenario::apply`]), checked for cars ([`check_car_speeds`]) when
/// `by_car`; `None` when the scenario changes nothing.
pub(crate) fn scenario_network(
    base: &StreetNetwork,
    s: &Scenario,
    by_car: bool,
) -> Result<Option<StreetNetwork>, RouteError> {
    let net = s.apply(base)?;
    if let (Some(n), true) = (&net, by_car) {
        check_car_speeds(n)?;
    }
    Ok(net)
}

/// When a car search runs on a scenario network, the car speeds R5 would
/// divide by: an edge cars may use with a speed of 0 or less (a factor too
/// small, or a speed beyond the 327 m/s R5 stores, which wraps around)
/// makes R5 fail or route with negative times. That is an error here.
pub(crate) fn check_car_speeds(net: &StreetNetwork) -> Result<(), RouteError> {
    let bad: Vec<usize> = (0..net.n_edges())
        .filter(|&e| net.edge_flags[e] & ALLOWS_CAR != 0 && net.edge_speed_cms[e] <= 0)
        .collect();
    if let Some(&e) = bad.first() {
        return Err(RouteError::Invalid(format!(
            "the scenario leaves {} street edges open to cars with a speed of 0 or less (such \
             as an edge of OSM way {}, at {} cm/s): R5 keeps car speeds in cm/s as 16-bit \
             integers, so a factor or speed that brings a speed below 1 cm/s, or above \
             32767 cm/s (about 1180 km/h), makes r5r fail or route with negative times; use \
             another factor or speed, or a factor or speed of 0 to close the road",
            bad.len(),
            osm_id(net, e),
            net.edge_speed_cms[e]
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_from_dir, BuildOptions};

    const SPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../r/inst/extdata/spo");

    #[test]
    fn java_parsers() {
        assert_eq!(java_parse_long("27184648"), Some(27184648));
        assert_eq!(java_parse_long("-1"), Some(-1));
        assert_eq!(java_parse_long("1e+05"), None);
        assert_eq!(java_parse_long(" 1"), None);
        assert_eq!(java_parse_long("1.0"), None);
        assert_eq!(java_parse_int("4294967296"), None);
        assert_eq!(java_parse_float("0.5"), Some(0.5));
        assert_eq!(java_parse_float(" 1e-04 "), Some(1e-4));
        assert_eq!(java_parse_float("0.333333333333333"), Some(1.0 / 3.0));
        assert_eq!(java_parse_float("2f"), Some(2.0));
        assert_eq!(java_parse_float("Infinity"), Some(f32::INFINITY));
        assert_eq!(java_parse_float("Inf"), None);
        assert_eq!(java_parse_float("inf"), None);
        assert_eq!(java_parse_float("1e"), None);
        assert_eq!(java_parse_float(""), None);
    }

    #[test]
    fn speeds_and_levels_as_r5r_modifications() {
        let (base, _) = build_from_dir(SPO, &BuildOptions::default()).unwrap();
        let e = (0..base.n_edges())
            .find(|&e| base.edge_flags[e] & ALLOWS_CAR != 0 && base.edge_flags[e] & LINK == 0)
            .unwrap();
        let way = base.pair_way[e / 2];
        let speed = base.edge_speed_cms[e];
        let mut s = Scenario::default();
        assert!(s.is_unchanged() && s.apply(&base).unwrap().is_none());
        // a factor: a float product, truncated
        s.car_speeds = CarSpeeds::ScaleByWay(vec![(way, 2.0), (way, 0.7)]);
        let net = s.apply(&base).unwrap().unwrap();
        assert_eq!(net.edge_speed_cms[e], (f32::from(speed) * 0.7f32) as i16);
        // carspeed_scale for the others, connectors included (way -1)
        s.car_speed_scale = 0.5;
        let net = s.apply(&base).unwrap().unwrap();
        let link = (0..base.n_edges())
            .find(|&e| base.edge_flags[e] & LINK != 0)
            .unwrap();
        assert_eq!(osm_id(&base, link), -1);
        assert_eq!(net.edge_speed_cms[link], base.edge_speed_cms[link] / 2);
        // km/h replaces; 0 closes to cars, speed unchanged
        s.car_speeds = CarSpeeds::KmhByWay(vec![(way, 36.0)]);
        assert_eq!(s.apply(&base).unwrap().unwrap().edge_speed_cms[e], 1000);
        s.car_speeds = CarSpeeds::KmhByWay(vec![(way, 0.0)]);
        let net = s.apply(&base).unwrap().unwrap();
        assert_eq!(net.edge_flags[e] & ALLOWS_CAR, 0);
        assert_eq!(net.edge_speed_cms[e], speed);
        // a speed beyond 32767 cm/s wraps around and is refused for cars
        s.car_speeds = CarSpeeds::KmhByWay(vec![(way, 2000.0)]);
        let net = s.apply(&base).unwrap().unwrap();
        assert!(net.edge_speed_cms[e] < 0);
        assert!(check_car_speeds(&net).is_err());
        // levels: below 2 is 1, above 4 is 4, 0 leaves the edge
        let mut s = Scenario::default();
        for (lts, flag) in [(-3, BIKE_LTS_1), (3, BIKE_LTS_3), (9, BIKE_LTS_4)] {
            s.lts = LtsChange::ByWay(vec![(way, lts)]);
            let f = s.apply(&base).unwrap().unwrap().edge_flags[e];
            assert_eq!(f & LTS_MASK, flag);
            assert_ne!(f & BIKE_LTS_EXPLICIT, 0);
        }
        s.lts = LtsChange::ByWay(vec![(way, 0)]);
        assert_eq!(
            s.apply(&base).unwrap().unwrap().edge_flags[e],
            base.edge_flags[e]
        );
        // the base network is never changed
        assert_eq!(base.edge_speed_cms[e], speed);
    }

    #[test]
    fn invalid_scenarios_are_errors() {
        let (base, _) = build_from_dir(SPO, &BuildOptions::default()).unwrap();
        let s = Scenario {
            car_speed_scale: -1.0,
            ..Default::default()
        };
        assert!(s.apply(&base).is_err());
        let mut s = Scenario::default();
        let bowtie = vec![vec![
            [0.0, 0.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [0.0, 0.0],
        ]];
        s.car_speeds = CarSpeeds::ByPolygon(vec![SpeedPolygon::new(vec![bowtie], 0.5, 1.0)]);
        assert!(s
            .apply(&base)
            .unwrap_err()
            .to_string()
            .contains("not valid"));
        let s = Scenario {
            lts: LtsChange::ByLine(vec![LtsLine::new(vec![[0.0, 0.0]], 2)]),
            ..Default::default()
        };
        assert!(s.apply(&base).is_err());
    }
}
