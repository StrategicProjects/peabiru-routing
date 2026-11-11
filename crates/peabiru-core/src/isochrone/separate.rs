//! Rings to polygons with holes, a port of `separate_polygons()` of the R
//! package isoband 0.3.0 (Claus O. Wilke, MIT; `src/separate-polygons.cpp`),
//! which `isoband::iso_to_sfg()` calls to make a `MULTIPOLYGON`.

use super::isoband::Ring;

/// A polygon: its shell, then its holes; every ring closed (the last point
/// repeats the first).
pub type Polygon = Vec<Vec<[f64; 2]>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum In {
    Inside,
    Outside,
    Undetermined,
}

/// isoband `ray_intersections`: how often a ray from `p` to the right
/// crosses the segment `p0`-`p1` (0 or 1), or -1 when `p` is on it.
fn ray_intersections(p: [f64; 2], p0: [f64; 2], p1: [f64; 2]) -> i32 {
    let ([px, py], [x0, y0], [x1, y1]) = (p, p0, p1);
    if y0 < y1 {
        if py < y0 || py > y1 {
            return 0;
        }
    } else if py > y0 || py < y1 {
        return 0;
    }
    if px > x0 && px > x1 {
        return 0;
    }
    let dy = y1 - y0;
    if dy == 0.0 {
        if py == y0 {
            return if px < x0 && px < x1 { 1 } else { -1 };
        }
        return 0;
    }
    let t = (py - y0) / dy;
    let xint = x0 + t * (x1 - x0);
    if xint < px {
        0
    } else if xint == px {
        -1
    } else {
        1
    }
}

/// isoband `point_in_polygon` over a closed ring.
fn point_in_polygon(p: [f64; 2], poly: &[[f64; 2]]) -> In {
    let n = poly.len();
    let mut intersections = 0;
    let mut istart = 0;
    while poly[istart][1] == p[1] {
        istart += 1;
        if istart == n - 1 {
            // every vertex is at the point's height
            let (mut xmin, mut xmax) = (poly[0][0], poly[0][0]);
            for q in &poly[1..n - 1] {
                xmin = xmin.min(q[0]);
                xmax = xmax.max(q[0]);
            }
            return if p[0] >= xmin && p[0] <= xmax {
                In::Undetermined
            } else {
                In::Outside
            };
        }
    }
    let mut i = istart;
    loop {
        let mut itr = ray_intersections(p, poly[i], poly[i + 1]);
        if itr < 0 {
            return In::Undetermined;
        }
        if itr > 0 && poly[i + 1][1] == p[1] {
            // the ray passes through a vertex: count it once, and only if
            // the ring crosses the ray there
            let from_above = poly[i][1] > poly[i + 1][1];
            let mut wrap_around = false;
            let mut j = i + 1;
            loop {
                if j == n - 1 {
                    j = 0;
                }
                if j == istart {
                    wrap_around = true;
                }
                if ray_intersections(p, poly[j], poly[j + 1]) < 0 {
                    return In::Undetermined;
                }
                j += 1;
                if poly[j][1] != poly[i + 1][1] {
                    break;
                }
            }
            if (!from_above && poly[j][1] < poly[i + 1][1])
                || (from_above && poly[j][1] > poly[i + 1][1])
            {
                itr = 0;
            }
            i = j;
            if wrap_around || i == istart {
                intersections += itr;
                break;
            }
            i -= 1;
        }
        intersections += itr;
        i += 1;
        if i == n - 1 {
            i = 0;
        }
        if i == istart {
            break;
        }
    }
    if intersections % 2 == 1 {
        In::Inside
    } else {
        In::Outside
    }
}

/// isoband `polygon_in_polygon` with `fast = true`: the first vertex of
/// `query` that is clearly inside or outside `reference` decides.
fn polygon_in_polygon(query: &[[f64; 2]], reference: &[[f64; 2]]) -> In {
    let (mut ins, mut out) = (0, 0);
    for &q in &query[..query.len() - 1] {
        match point_in_polygon(q, reference) {
            In::Inside => ins += 1,
            In::Outside => out += 1,
            In::Undetermined => {}
        }
        if ins > 0 || out > 0 {
            break;
        }
    }
    if ins > 0 && out == 0 {
        In::Inside
    } else if out > 0 && ins == 0 {
        In::Outside
    } else {
        In::Undetermined
    }
}

/// isoband `is_valid_ring`: at least 4 points, not all the same.
fn is_valid_ring(poly: &[[f64; 2]]) -> bool {
    poly.len() >= 4 && poly[1..].iter().any(|q| q != &poly[0])
}

/// isoband `separate_polygons`: close every ring, find which rings enclose
/// which, and make polygons of the outermost rings with the rings directly
/// inside them as holes (reversed), in ring order. `Err` when two rings
/// touch so that neither can be said to be inside or outside the other
/// (isoband stops there too).
pub(crate) fn separate_polygons(rings: Vec<Ring>) -> Result<Vec<Polygon>, String> {
    let mut polys = rings;
    for p in &mut polys {
        if p.first() != p.last() {
            let f = p[0];
            p.push(f);
        }
    }
    let n = polys.len();
    // ext[i]: the rings enclosing ring i (std::set<int>, kept sorted)
    let mut ext: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut active = vec![true; n];
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            match polygon_in_polygon(&polys[i], &polys[j]) {
                In::Inside => ext[i].push(j),
                In::Outside => {}
                In::Undetermined => {
                    return Err(
                        "Found polygons without undefined interior/exterior relationship.".into(),
                    )
                }
            }
        }
    }
    let top_level = |ext: &[Vec<usize>], active: &mut [bool]| -> Option<usize> {
        let i = (0..n).find(|&i| active[i] && ext[i].is_empty())?;
        active[i] = false;
        Some(i)
    };
    let mut out = Vec::new();
    while let Some(shell) = top_level(&ext, &mut active) {
        let holes: Vec<usize> = (0..n)
            .filter(|&i| active[i] && ext[i].len() == 1 && ext[i][0] == shell)
            .collect();
        for &h in &holes {
            active[h] = false;
        }
        for e in &mut ext {
            e.retain(|k| *k != shell && !holes.contains(k));
        }
        if is_valid_ring(&polys[shell]) {
            let mut rings = vec![polys[shell].clone()];
            for &h in &holes {
                if is_valid_ring(&polys[h]) {
                    rings.push(polys[h].iter().rev().copied().collect());
                }
            }
            out.push(rings);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, s: f64) -> Ring {
        vec![[x0, y0], [x0 + s, y0], [x0 + s, y0 + s], [x0, y0 + s]]
    }

    #[test]
    fn shells_holes_and_islands() {
        // outer square, a hole in it, an island in the hole, a separate square
        let rings = vec![
            square(2.0, 2.0, 6.0),
            square(0.0, 0.0, 10.0),
            square(4.0, 4.0, 2.0),
            square(20.0, 0.0, 1.0),
        ];
        let polys = separate_polygons(rings).unwrap();
        assert_eq!(polys.len(), 3);
        // ring 1 is the first top-level ring, with ring 0 as its hole
        assert_eq!(polys[0].len(), 2);
        assert_eq!(polys[0][0][0], [0.0, 0.0]);
        assert_eq!(polys[0][1][1], [2.0, 8.0]); // reversed, closed
        assert_eq!(polys[0][1].len(), 5);
        // then the island (inside the hole), then the separate square
        assert_eq!(polys[1][0][0], [4.0, 4.0]);
        assert_eq!(polys[2][0][0], [20.0, 0.0]);
    }

    #[test]
    fn points_on_vertices_and_edges() {
        let ring: Vec<[f64; 2]> = vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0], [0.0, 0.0]];
        assert_eq!(point_in_polygon([1.0, 1.0], &ring), In::Inside);
        assert_eq!(point_in_polygon([3.0, 1.0], &ring), In::Outside);
        assert_eq!(point_in_polygon([2.0, 1.0], &ring), In::Undetermined);
        assert_eq!(point_in_polygon([1.0, 0.0], &ring), In::Undetermined);
        // a ray through a vertex of a diamond
        let diamond: Vec<[f64; 2]> =
            vec![[1.0, 0.0], [2.0, 1.0], [1.0, 2.0], [0.0, 1.0], [1.0, 0.0]];
        assert_eq!(point_in_polygon([0.5, 1.0], &diamond), In::Inside);
        assert_eq!(point_in_polygon([-1.0, 1.0], &diamond), In::Outside);
    }
}
