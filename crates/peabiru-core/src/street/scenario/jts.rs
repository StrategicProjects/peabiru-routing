//! The pieces of JTS 1.18.1 (Eclipse Public License 2.0 / Eclipse
//! Distribution License 1.0) that R5's scenario modifications run, ported
//! for the same results: the `STRtree` spatial index (its query order
//! decides between candidates), `DiscreteHausdorffDistance` between two
//! lines, and the predicate `Polygon.intersects(LineString)`, made with
//! exact orientation tests. `Polygon.intersection(LineString).getLength()`
//! is computed in floating point; JTS's overlay can differ from it in the
//! last bits, so its callers treat near ties as undecidable.

/// A point `[x, y]` (`[lon, lat]` degrees).
pub(super) type Pt = [f64; 2];

/// JTS `Envelope`: `(min_x, max_x, min_y, max_y)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Env {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
}

impl Env {
    /// The envelope of `pts`, `None` when there are none.
    pub(super) fn of(pts: &[Pt]) -> Option<Env> {
        let first = pts.first()?;
        let mut e = Env {
            min_x: first[0],
            max_x: first[0],
            min_y: first[1],
            max_y: first[1],
        };
        for p in &pts[1..] {
            e.expand(&Env {
                min_x: p[0],
                max_x: p[0],
                min_y: p[1],
                max_y: p[1],
            });
        }
        Some(e)
    }

    /// `Envelope.expandToInclude(Envelope)`.
    fn expand(&mut self, o: &Env) {
        if o.min_x < self.min_x {
            self.min_x = o.min_x;
        }
        if o.max_x > self.max_x {
            self.max_x = o.max_x;
        }
        if o.min_y < self.min_y {
            self.min_y = o.min_y;
        }
        if o.max_y > self.max_y {
            self.max_y = o.max_y;
        }
    }

    /// `Envelope.intersects(Envelope)` (closed).
    pub(super) fn intersects(&self, o: &Env) -> bool {
        !(o.min_x > self.max_x
            || o.max_x < self.min_x
            || o.min_y > self.max_y
            || o.max_y < self.min_y)
    }

    fn centre_x(&self) -> f64 {
        (self.min_x + self.max_x) / 2.0
    }

    fn centre_y(&self) -> f64 {
        (self.min_y + self.max_y) / 2.0
    }
}

/// `AbstractSTRtree.compareDoubles`, as an ordering for a stable sort.
fn compare(a: f64, b: f64) -> std::cmp::Ordering {
    if a > b {
        std::cmp::Ordering::Greater
    } else if a < b {
        std::cmp::Ordering::Less
    } else {
        std::cmp::Ordering::Equal
    }
}

/// A node of the tree: its envelope and children (items or nodes).
#[derive(Debug, Clone)]
enum Child {
    Item(usize),
    Node(usize),
}

#[derive(Debug, Clone)]
struct Node {
    env: Env,
    children: Vec<Child>,
}

/// JTS `STRtree` with its default node capacity of 10, built at once from
/// items inserted in order: [`StrTree::query`] gives the items in the
/// order JTS's `query(Envelope)` gives them.
#[derive(Debug, Clone)]
pub(super) struct StrTree {
    item_env: Vec<Env>,
    nodes: Vec<Node>,
    root: Option<usize>,
}

const NODE_CAPACITY: usize = 10;

impl StrTree {
    /// The tree of items `0..envs.len()`; an item without envelope (an
    /// empty geometry, which `STRtree.insert` skips) is left out.
    pub(super) fn build(envs: &[Option<Env>]) -> StrTree {
        let mut t = StrTree {
            item_env: envs
                .iter()
                .map(|e| {
                    e.unwrap_or(Env {
                        min_x: 0.0,
                        max_x: 0.0,
                        min_y: 0.0,
                        max_y: 0.0,
                    })
                })
                .collect(),
            nodes: Vec::new(),
            root: None,
        };
        let items: Vec<Child> = envs
            .iter()
            .enumerate()
            .filter(|(_, e)| e.is_some())
            .map(|(i, _)| Child::Item(i))
            .collect();
        if !items.is_empty() {
            t.root = Some(t.higher_levels(items));
        }
        t
    }

    fn env(&self, c: &Child) -> Env {
        match *c {
            Child::Item(i) => self.item_env[i],
            Child::Node(n) => self.nodes[n].env,
        }
    }

    /// `createHigherLevels`.
    fn higher_levels(&mut self, level: Vec<Child>) -> usize {
        let parents = self.parent_boundables(level);
        if parents.len() == 1 {
            match parents[0] {
                Child::Node(n) => n,
                Child::Item(_) => unreachable!("parents are nodes"),
            }
        } else {
            self.higher_levels(parents)
        }
    }

    /// `STRtree.createParentBoundables`: sorted by the centre's x, cut into
    /// vertical slices, each sorted by the centre's y and cut into nodes.
    fn parent_boundables(&mut self, children: Vec<Child>) -> Vec<Child> {
        let min_leaf_count = (children.len() as f64 / NODE_CAPACITY as f64).ceil() as usize;
        let mut sorted = children;
        sorted.sort_by(|a, b| compare(self.env(a).centre_x(), self.env(b).centre_x()));
        let slice_count = (min_leaf_count as f64).sqrt().ceil() as usize;
        let slice_capacity = (sorted.len() as f64 / slice_count as f64).ceil() as usize;
        let mut parents = Vec::new();
        let mut it = sorted.into_iter();
        for _ in 0..slice_count {
            let slice: Vec<Child> = it.by_ref().take(slice_capacity).collect();
            parents.extend(self.parents_of_slice(slice));
        }
        parents
    }

    /// `AbstractSTRtree.createParentBoundables` (one vertical slice; an
    /// empty slice still makes one empty node, as in JTS).
    fn parents_of_slice(&mut self, mut slice: Vec<Child>) -> Vec<Child> {
        slice.sort_by(|a, b| compare(self.env(a).centre_y(), self.env(b).centre_y()));
        let mut groups: Vec<Vec<Child>> = vec![Vec::new()];
        for c in slice {
            if groups.last().expect("one group").len() == NODE_CAPACITY {
                groups.push(Vec::new());
            }
            groups.last_mut().expect("one group").push(c);
        }
        groups
            .into_iter()
            .map(|children| {
                let mut env: Option<Env> = None;
                for c in &children {
                    let e = self.env(c);
                    match &mut env {
                        None => env = Some(e),
                        Some(b) => b.expand(&e),
                    }
                }
                self.nodes.push(Node {
                    // an empty node has a null envelope, which intersects
                    // nothing
                    env: env.unwrap_or(Env {
                        min_x: f64::INFINITY,
                        max_x: f64::NEG_INFINITY,
                        min_y: f64::INFINITY,
                        max_y: f64::NEG_INFINITY,
                    }),
                    children,
                });
                Child::Node(self.nodes.len() - 1)
            })
            .collect()
    }

    /// The items whose envelope intersects `search`, in JTS's order.
    pub(super) fn query(&self, search: &Env, out: &mut Vec<usize>) {
        out.clear();
        if let Some(r) = self.root {
            if self.nodes[r].env.intersects(search) {
                self.query_node(r, search, out);
            }
        }
    }

    fn query_node(&self, n: usize, search: &Env, out: &mut Vec<usize>) {
        for c in &self.nodes[n].children {
            if !self.env(c).intersects(search) {
                continue;
            }
            match *c {
                Child::Node(m) => self.query_node(m, search, out),
                Child::Item(i) => out.push(i),
            }
        }
    }
}

/// `Coordinate.distance`: `a - b`, then the square root.
fn distance(a: Pt, b: Pt) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

/// `LineSegment.projectionFactor`.
fn projection_factor(p0: Pt, p1: Pt, p: Pt) -> f64 {
    if p == p0 {
        return 0.0;
    }
    if p == p1 {
        return 1.0;
    }
    let dx = p1[0] - p0[0];
    let dy = p1[1] - p0[1];
    let len = dx * dx + dy * dy;
    if len <= 0.0 {
        return f64::NAN;
    }
    ((p[0] - p0[0]) * dx + (p[1] - p0[1]) * dy) / len
}

/// `LineSegment.closestPoint`.
fn closest_point(p0: Pt, p1: Pt, p: Pt) -> Pt {
    let factor = projection_factor(p0, p1, p);
    if factor > 0.0 && factor < 1.0 {
        // LineSegment.project, which computes the factor again
        let r = projection_factor(p0, p1, p);
        return [p0[0] + r * (p1[0] - p0[0]), p0[1] + r * (p1[1] - p0[1])];
    }
    let dist0 = distance(p0, p);
    let dist1 = distance(p1, p);
    if dist0 < dist1 {
        p0
    } else {
        p1
    }
}

/// `DistanceToPoint.computeDistance(LineString, pt)`: the least distance
/// from `pt` to the segments of `line`.
fn distance_to_line(line: &[Pt], pt: Pt) -> f64 {
    let mut best = f64::NAN;
    let mut first = true;
    for w in line.windows(2) {
        let d = distance(closest_point(w[0], w[1], pt), pt);
        if first || d < best {
            best = d;
            first = false;
        }
    }
    best
}

/// `DiscreteHausdorffDistance.distance(g0, g1)` of two lines (no
/// densification): the largest distance from a vertex of either line to
/// the other line.
pub(super) fn discrete_hausdorff(g0: &[Pt], g1: &[Pt]) -> f64 {
    let mut best = f64::NAN;
    let mut first = true;
    for (a, b) in [(g0, g1), (g1, g0)] {
        // MaxPointDistanceFilter: the largest of the vertices' distances
        let mut max = f64::NAN;
        let mut max_first = true;
        for &p in a {
            let d = distance_to_line(b, p);
            if max_first || d > max {
                max = d;
                max_first = false;
            }
        }
        if first || max > best {
            best = max;
            first = false;
        }
    }
    best
}

// ---------------------------------------------------------------------------
// Exact orientation (Shewchuk's expansion arithmetic).

/// `a + b` as an exact pair `(sum, error)`.
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bv = s - a;
    let av = s - bv;
    (s, (a - av) + (b - bv))
}

/// `a * b` as an exact pair `(product, error)`.
fn two_product(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    (p, a.mul_add(b, -p))
}

/// The exact sum of the expansions `e` and `f` (nonoverlapping, in any
/// order), zeros dropped.
fn expansion_sum(e: &[f64], f: &[f64]) -> Vec<f64> {
    let mut h: Vec<f64> = e.to_vec();
    for &b in f {
        let mut q = b;
        let mut next = Vec::with_capacity(h.len() + 1);
        for &a in &h {
            let (s, err) = two_sum(q, a);
            if err != 0.0 {
                next.push(err);
            }
            q = s;
        }
        next.push(q);
        h = next;
    }
    h
}

/// The exact sign of the expansion `e` (its largest component decides
/// once the components are nonoverlapping and increasing, which
/// [`expansion_sum`] keeps).
fn expansion_sign(e: &[f64]) -> i32 {
    match e.iter().rev().find(|&&x| x != 0.0) {
        Some(&x) if x > 0.0 => 1,
        Some(_) => -1,
        None => 0,
    }
}

/// The exact sign of the orientation of `c` with respect to the line from
/// `a` to `b`: 1 to the left (counterclockwise), -1 to the right, 0 on it.
pub(super) fn orientation(a: Pt, b: Pt, c: Pt) -> i32 {
    // fast path: the floating determinant when it is far from zero
    let l = (a[0] - c[0]) * (b[1] - c[1]);
    let r = (a[1] - c[1]) * (b[0] - c[0]);
    let det = l - r;
    let bound = 1e-12 * (l.abs() + r.abs());
    if det > bound {
        return 1;
    }
    if -det > bound {
        return -1;
    }
    // exact: (ax - cx)(by - cy) - (ay - cy)(bx - cx), each difference an
    // exact pair
    let (acx, acx_e) = two_sum(a[0], -c[0]);
    let (bcy, bcy_e) = two_sum(b[1], -c[1]);
    let (acy, acy_e) = two_sum(a[1], -c[1]);
    let (bcx, bcx_e) = two_sum(b[0], -c[0]);
    let mut terms: Vec<f64> = Vec::with_capacity(16);
    for (x, y, sign) in [
        (acx, bcy, 1.0),
        (acx, bcy_e, 1.0),
        (acx_e, bcy, 1.0),
        (acx_e, bcy_e, 1.0),
        (acy, bcx, -1.0),
        (acy, bcx_e, -1.0),
        (acy_e, bcx, -1.0),
        (acy_e, bcx_e, -1.0),
    ] {
        let (p, e) = two_product(x, y);
        terms.push(sign * p);
        terms.push(sign * e);
    }
    let mut sum: Vec<f64> = Vec::new();
    for t in terms {
        if t != 0.0 {
            sum = expansion_sum(&sum, &[t]);
        }
    }
    expansion_sign(&sum)
}

/// Whether `p` lies on the closed segment `a`-`b`, exactly.
fn on_segment(a: Pt, b: Pt, p: Pt) -> bool {
    orientation(a, b, p) == 0
        && p[0] >= a[0].min(b[0])
        && p[0] <= a[0].max(b[0])
        && p[1] >= a[1].min(b[1])
        && p[1] <= a[1].max(b[1])
}

/// Whether the closed segments `p1`-`p2` and `q1`-`q2` share a point,
/// exactly.
fn segments_intersect(p1: Pt, p2: Pt, q1: Pt, q2: Pt) -> bool {
    let d1 = orientation(q1, q2, p1);
    let d2 = orientation(q1, q2, p2);
    let d3 = orientation(p1, p2, q1);
    let d4 = orientation(p1, p2, q2);
    if d1 * d2 < 0 && d3 * d4 < 0 {
        return true;
    }
    (d1 == 0 && on_segment(q1, q2, p1))
        || (d2 == 0 && on_segment(q1, q2, p2))
        || (d3 == 0 && on_segment(p1, p2, q1))
        || (d4 == 0 && on_segment(p1, p2, q2))
}

/// Where `p` is with respect to a closed ring: 1 inside, 0 on it, -1
/// outside (crossing number with exact orientation tests).
fn ring_location(ring: &[Pt], p: Pt) -> i32 {
    let mut inside = false;
    for w in ring.windows(2) {
        let (a, b) = (w[0], w[1]);
        if on_segment(a, b, p) {
            return 0;
        }
        // the edge crosses the horizontal ray to the right of p
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let o = orientation(a, b, p);
            // p left of an upward edge, or right of a downward one
            if (b[1] > a[1] && o > 0) || (b[1] < a[1] && o < 0) {
                inside = !inside;
            }
        }
    }
    if inside {
        1
    } else {
        -1
    }
}

/// Whether `p` is in the closed polygon (shell, then holes).
fn in_polygon(poly: &[Vec<Pt>], p: Pt) -> bool {
    let Some(shell) = poly.first() else {
        return false;
    };
    if ring_location(shell, p) < 0 {
        return false;
    }
    !poly[1..].iter().any(|hole| ring_location(hole, p) > 0)
}

/// JTS `Geometry.intersects` of a (multi)polygon and a line: whether they
/// share a point, the polygons' boundaries included.
pub(super) fn polygon_intersects_line(polys: &[Vec<Vec<Pt>>], line: &[Pt]) -> bool {
    polys.iter().any(|poly| {
        line.iter().any(|&p| in_polygon(poly, p))
            || poly.iter().any(|ring| {
                ring.windows(2).any(|r| {
                    line.windows(2)
                        .any(|s| segments_intersect(s[0], s[1], r[0], r[1]))
                })
            })
    })
}

/// Whether `line` lies in the interior of the (multi)polygon, touching no
/// ring: then JTS's intersection is the line itself, so two polygons that
/// both hold a line give it exactly the same length.
pub(super) fn line_in_interior(polys: &[Vec<Vec<Pt>>], line: &[Pt]) -> bool {
    let touches = polys.iter().flatten().any(|ring| {
        ring.windows(2).any(|r| {
            line.windows(2)
                .any(|s| segments_intersect(s[0], s[1], r[0], r[1]))
        })
    });
    !touches
        && line
            .first()
            .is_some_and(|&p| polys.iter().any(|poly| in_polygon(poly, p)))
}

/// Whether a polygon (shell then holes) is one JTS can work with: no ring
/// crosses or touches itself, other than at the shared ends of
/// consecutive segments, or another ring, and every hole is inside the
/// shell. An exact test.
pub(super) fn polygon_is_simple(poly: &[Vec<Pt>]) -> bool {
    let segs: Vec<(usize, usize, Pt, Pt)> = poly
        .iter()
        .enumerate()
        .flat_map(|(r, ring)| {
            ring.windows(2)
                .enumerate()
                .map(move |(i, w)| (r, i, w[0], w[1]))
        })
        .collect();
    for (a, &(ra, ia, p1, p2)) in segs.iter().enumerate() {
        for &(rb, ib, q1, q2) in &segs[a + 1..] {
            if ra == rb {
                let n = poly[ra].len() - 1;
                let adjacent = ib == ia + 1 || (ia == 0 && ib + 1 == n);
                if adjacent {
                    // they share one end: they must not overlap otherwise
                    let shared = if ib == ia + 1 { p2 } else { p1 };
                    let (other_a, other_b) = if ib == ia + 1 { (p1, q2) } else { (p2, q1) };
                    if on_segment(q1, q2, other_a) && other_a != shared
                        || on_segment(p1, p2, other_b) && other_b != shared
                    {
                        return false;
                    }
                    continue;
                }
            }
            if segments_intersect(p1, p2, q1, q2) {
                return false;
            }
        }
    }
    poly[1..].iter().all(|hole| {
        hole.first()
            .is_some_and(|&p| ring_location(&poly[0], p) > 0)
    })
}

/// The length of the part of `line` in the closed (multi)polygon, in
/// degrees, as `polygon.intersection(line).getLength()` (in floating
/// point: see the module).
pub(super) fn intersection_length(polys: &[Vec<Vec<Pt>>], line: &[Pt]) -> f64 {
    let mut total = 0.0;
    for s in line.windows(2) {
        let (a, b) = (s[0], s[1]);
        let len = distance(a, b);
        if len == 0.0 {
            continue;
        }
        let mut ts = vec![0.0, 1.0];
        for poly in polys {
            for ring in poly {
                for r in ring.windows(2) {
                    if let Some(t) = crossing(a, b, r[0], r[1]) {
                        ts.push(t);
                    }
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        for w in ts.windows(2) {
            if w[1] <= w[0] {
                continue;
            }
            let m = (w[0] + w[1]) / 2.0;
            let mid = [a[0] + m * (b[0] - a[0]), a[1] + m * (b[1] - a[1])];
            if polys.iter().any(|poly| in_polygon(poly, mid)) {
                total += (w[1] - w[0]) * len;
            }
        }
    }
    total
}

/// Where along `a`-`b` (0 to 1) it meets the segment `c`-`d`, in floating
/// point; `None` when they are parallel or do not meet.
fn crossing(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<f64> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let s = [d[0] - c[0], d[1] - c[1]];
    let den = r[0] * s[1] - r[1] * s[0];
    if den == 0.0 {
        return None;
    }
    let qp = [c[0] - a[0], c[1] - a[1]];
    let t = (qp[0] * s[1] - qp[1] * s[0]) / den;
    let u = (qp[0] * r[1] - qp[1] * r[0]) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_orientation_of_nearly_collinear_points() {
        assert_eq!(orientation([0.0, 0.0], [1.0, 1.0], [2.0, 2.0]), 0);
        assert_eq!(orientation([0.0, 0.0], [1.0, 0.0], [0.5, 1e-300]), 1);
        assert_eq!(orientation([0.0, 0.0], [1.0, 0.0], [0.5, -1e-300]), -1);
        // a classic case the floating determinant gets wrong
        let (a, b) = ([0.5, 0.5], [12.0, 12.0]);
        let c = [24.0, 24.0];
        assert_eq!(orientation(a, b, c), 0);
        let c = [0.5 + f64::EPSILON, 0.5];
        assert_eq!(orientation(a, b, c), -1);
        let x = 0.1 + 0.2; // 0.30000000000000004
        assert_eq!(orientation([0.1, 0.1], [0.2, 0.2], [x, x]), 0);
        assert_eq!(orientation([0.1, 0.1], [0.2, 0.2], [x, 0.3]), -1);
    }

    #[test]
    fn polygon_and_line_predicates() {
        let square = vec![vec![vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            [0.0, 0.0],
        ]]];
        assert!(polygon_intersects_line(&square, &[[0.5, 0.5], [0.6, 0.6]]));
        assert!(polygon_intersects_line(&square, &[[-1.0, 0.5], [2.0, 0.5]]));
        assert!(polygon_intersects_line(&square, &[[1.0, 2.0], [1.0, 1.0]]));
        assert!(!polygon_intersects_line(
            &square,
            &[[1.1, 2.0], [1.1, -1.0]]
        ));
        let holed = vec![vec![
            square[0][0].clone(),
            vec![
                [0.25, 0.25],
                [0.25, 0.75],
                [0.75, 0.75],
                [0.75, 0.25],
                [0.25, 0.25],
            ],
        ]];
        assert!(!polygon_intersects_line(&holed, &[[0.4, 0.4], [0.6, 0.6]]));
        assert!(polygon_intersects_line(&holed, &[[0.4, 0.4], [0.25, 0.6]]));
        let l = intersection_length(&square, &[[-1.0, 0.5], [2.0, 0.5]]);
        assert!((l - 1.0).abs() < 1e-12);
        let l = intersection_length(&holed, &[[-1.0, 0.5], [2.0, 0.5]]);
        assert!((l - 0.5).abs() < 1e-12);
    }

    #[test]
    fn hausdorff_as_jts() {
        let a = [[0.0, 0.0], [1.0, 0.0]];
        let b = [[0.0, 1.0], [1.0, 1.0], [2.0, 1.0]];
        // from (2, 1) to (1, 0)
        assert_eq!(discrete_hausdorff(&a, &b), 2f64.sqrt());
        assert_eq!(discrete_hausdorff(&a, &a), 0.0);
    }

    #[test]
    fn tree_queries_in_jts_order() {
        // 25 unit boxes on a diagonal: leaves of 10, 10 and 5 in x order
        let envs: Vec<Option<Env>> = (0..25)
            .map(|i| {
                let x = f64::from(i);
                Some(Env {
                    min_x: x,
                    max_x: x + 1.0,
                    min_y: x,
                    max_y: x + 1.0,
                })
            })
            .collect();
        let t = StrTree::build(&envs);
        let mut out = Vec::new();
        t.query(
            &Env {
                min_x: 5.5,
                max_x: 12.5,
                min_y: 0.0,
                max_y: 100.0,
            },
            &mut out,
        );
        assert_eq!(out, (5..=12).collect::<Vec<_>>());
        let t = StrTree::build(&[None, envs[3], None]);
        t.query(&envs[3].unwrap(), &mut out);
        assert_eq!(out, [1]);
    }
}
