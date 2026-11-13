//! The lines of rides: R5's `TripPattern.getHopGeometries` with
//! `saveShapes = true`. A pattern with a GTFS shape is cut along it at the
//! stops' locations, which R5 finds when it builds the network by
//! projecting each stop onto the shape in a local coordinate system
//! (`LocationIndexedLineInLocalCoordinateSystem`, JTS 1.18.1
//! `LocationIndexOfPoint`, the fraction kept as a `float`) and cuts with
//! JTS's `ExtractLineByLocation`; when the shape's points and the stop
//! times of the pattern's first trip all carry `shape_dist_traveled`, R5
//! locates the stops by those distances instead
//! ([`crate::transit::layer::cuts_by_distance`]). A pattern without a shape
//! gets straight lines between its stops' street vertices.
//!
//! The functions below are written for this project to behave as JTS
//! 1.18.1's `LinearLocation`, `LocationIndexOfPoint` and
//! `ExtractLineByLocation` do in the cases R5 exercises (see
//! `LICENSE-THIRD-PARTY`).

use super::geom::Line;
use crate::street::StreetNetwork;
use crate::transit::TransitLayer;

/// A JTS `LinearLocation` on a single line (component 0), normalized.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Loc {
    seg: usize,
    frac: f64,
}

impl Loc {
    /// `new LinearLocation(segmentIndex, segmentFraction)`.
    fn new(seg: i64, frac: f64) -> Loc {
        let mut seg = seg;
        let mut frac = frac.clamp(0.0, 1.0);
        if seg < 0 {
            seg = 0;
            frac = 0.0;
        }
        if frac == 1.0 {
            frac = 0.0;
            seg += 1;
        }
        Loc {
            seg: seg as usize,
            frac,
        }
    }

    fn is_vertex(&self) -> bool {
        self.frac <= 0.0 || self.frac >= 1.0
    }

    /// `compareLocationValues` against `(seg, frac)`.
    fn cmp_values(&self, seg: usize, frac: f64) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        if self.seg < seg {
            return Less;
        }
        if self.seg > seg {
            return Greater;
        }
        if self.frac < frac {
            return Less;
        }
        if self.frac > frac {
            return Greater;
        }
        Equal
    }

    fn cmp(&self, o: &Loc) -> std::cmp::Ordering {
        self.cmp_values(o.seg, o.frac)
    }

    /// `getCoordinate(line)`.
    fn coordinate(&self, line: &[[f64; 2]]) -> [f64; 2] {
        let p0 = line[self.seg];
        if self.seg + 1 >= line.len() {
            return p0;
        }
        let p1 = line[self.seg + 1];
        if self.frac <= 0.0 {
            return p0;
        }
        if self.frac >= 1.0 {
            return p1;
        }
        [
            (p1[0] - p0[0]) * self.frac + p0[0],
            (p1[1] - p0[1]) * self.frac + p0[1],
        ]
    }
}

/// `Distance.pointToSegment(p, a, b)`.
fn point_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let dist = |u: [f64; 2], v: [f64; 2]| {
        let (dx, dy) = (u[0] - v[0], u[1] - v[1]);
        (dx * dx + dy * dy).sqrt()
    };
    if a[0] == b[0] && a[1] == b[1] {
        return dist(p, a);
    }
    let len2 = (b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1]);
    let r = ((p[0] - a[0]) * (b[0] - a[0]) + (p[1] - a[1]) * (b[1] - a[1])) / len2;
    if r <= 0.0 {
        return dist(p, a);
    }
    if r >= 1.0 {
        return dist(p, b);
    }
    let s = ((a[1] - p[1]) * (b[0] - a[0]) - (a[0] - p[0]) * (b[1] - a[1])) / len2;
    s.abs() * len2.sqrt()
}

/// `LineSegment.segmentFraction(p)`.
fn segment_fraction(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let f = if p == a {
        0.0
    } else if p == b {
        1.0
    } else {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx * dx + dy * dy;
        if len <= 0.0 {
            f64::NAN
        } else {
            ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len
        }
    };
    if f < 0.0 {
        0.0
    } else if f > 1.0 || f.is_nan() {
        1.0
    } else {
        f
    }
}

/// `LocationIndexOfPoint.indexOf(pt)` on `line`; `None` for a line
/// without segments.
fn index_of(line: &[[f64; 2]], pt: [f64; 2]) -> Option<Loc> {
    let mut min_distance = f64::MAX;
    let mut best = None;
    for i in 0..line.len().saturating_sub(1) {
        let (a, b) = (line[i], line[i + 1]);
        let d = point_to_segment(pt, a, b);
        let frac = segment_fraction(pt, a, b);
        if d < min_distance {
            min_distance = d;
            best = Some((i, frac));
        }
    }
    best.map(|(i, f)| Loc::new(i as i64, f))
}

/// `ExtractLineByLocation.computeLinear(start, end)`; `None` when JTS would
/// not give a single line.
fn compute_linear(line: &[[f64; 2]], start: Loc, end: Loc) -> Option<Line> {
    let mut pts: Line = Vec::new();
    if !start.is_vertex() {
        pts.push(start.coordinate(line));
    }
    let mut vi = if start.frac > 0.0 {
        start.seg + 1
    } else {
        start.seg
    };
    let mut ended = false;
    while vi < line.len() {
        if end.cmp_values(vi, 0.0) == std::cmp::Ordering::Less {
            break;
        }
        pts.push(line[vi]);
        if vi + 1 >= line.len() {
            ended = true; // endLine(): a point after it would start a second line
        }
        vi += 1;
    }
    if !end.is_vertex() {
        if ended {
            return None;
        }
        pts.push(end.coordinate(line));
    }
    match pts.len() {
        0 => None,
        1 => Some(vec![pts[0], pts[0]]),
        _ => Some(pts),
    }
}

/// `ExtractLineByLocation.extract(line, start, end)`.
fn extract(line: &[[f64; 2]], start: Loc, end: Loc) -> Option<Line> {
    if end.cmp(&start) == std::cmp::Ordering::Less {
        let mut l = compute_linear(line, end, start)?;
        l.reverse();
        Some(l)
    } else {
        compute_linear(line, start, end)
    }
}

/// R5's projected stop locations along a pattern's shape: `(segment,
/// fraction as float)` per stop of the pattern.
fn stop_locations(
    layer: &TransitLayer,
    pattern: usize,
    shape: &[[f64; 2]],
) -> Option<Vec<(usize, f32)>> {
    let first = shape.first()?;
    let x_scale = first[1].to_radians().cos().abs();
    let projected: Line = shape.iter().map(|p| [p[0] / x_scale, p[1]]).collect();
    layer.patterns[pattern]
        .stops
        .iter()
        .map(|&s| {
            let s = s as usize;
            let pt = [layer.stop_lon[s] / x_scale, layer.stop_lat[s]];
            index_of(&projected, pt).map(|l| (l.seg, l.frac as f32))
        })
        .collect()
}

/// The line of a ride from stop position `from` to `to` of a pattern:
/// its hops' points one after the other (the shared stop repeated, as
/// `TripLeg.augmentTransitLeg` adds them). `Err` with a reason when R5
/// would fail on this pattern.
pub(crate) fn ride_line(
    net: &StreetNetwork,
    layer: &TransitLayer,
    pattern: usize,
    from: usize,
    to: usize,
) -> Result<Line, &'static str> {
    let pat = &layer.patterns[pattern];
    let mut out: Line = Vec::new();
    match layer.pattern_shape.get(pattern).copied().flatten() {
        Some(s) => {
            let shape = &layer.shapes[s as usize];
            let locs = match layer
                .pattern_shape_cuts
                .get(pattern)
                .and_then(Option::as_ref)
            {
                Some(cuts) => cuts.iter().map(|&(seg, f)| (seg as usize, f)).collect(),
                None => stop_locations(layer, pattern, shape)
                    .ok_or("a GTFS shape with fewer than two points")?,
            };
            // R5 cuts every hop of the pattern, and fails if any one fails
            let mut hops = Vec::with_capacity(locs.len().saturating_sub(1));
            for w in locs.windows(2) {
                let a = Loc::new(w[0].0 as i64, f64::from(w[0].1));
                let b = Loc::new(w[1].0 as i64, f64::from(w[1].1));
                hops.push(
                    extract(shape, a, b).ok_or("a GTFS shape that cannot be cut at its stops")?,
                );
            }
            for hop in &hops[from..to] {
                out.extend_from_slice(hop);
            }
        }
        None => {
            let point = |stop: u32| {
                let v = layer.stop_vertex[stop as usize].max(0) as usize;
                [f64::from(net.v_lon[v]) / 1e7, f64::from(net.v_lat[v]) / 1e7]
            };
            for i in from..to {
                out.push(point(pat.stops[i]));
                out.push(point(pat.stops[i + 1]));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_follows_jts() {
        let line = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
        // inside one segment
        let l = extract(&line, Loc::new(0, 0.25), Loc::new(0, 0.75)).unwrap();
        assert_eq!(l, vec![[0.25, 0.0], [0.75, 0.0]]);
        // across vertices
        let l = extract(&line, Loc::new(0, 0.5), Loc::new(2, 0.5)).unwrap();
        assert_eq!(l, vec![[0.5, 0.0], [1.0, 0.0], [2.0, 0.0], [2.5, 0.0]]);
        // backwards: reversed
        let l = extract(&line, Loc::new(2, 0.5), Loc::new(0, 0.5)).unwrap();
        assert_eq!(l, vec![[2.5, 0.0], [2.0, 0.0], [1.0, 0.0], [0.5, 0.0]]);
        // a single vertex: two equal points
        let l = extract(&line, Loc::new(1, 0.0), Loc::new(1, 0.0)).unwrap();
        assert_eq!(l, vec![[1.0, 0.0], [1.0, 0.0]]);
        // a fraction of 1 is the next vertex
        assert_eq!(Loc::new(0, 1.0), Loc { seg: 1, frac: 0.0 });
        let p = index_of(&line, [1.5, 1.0]).unwrap();
        assert_eq!(p, Loc { seg: 1, frac: 0.5 });
    }
}
