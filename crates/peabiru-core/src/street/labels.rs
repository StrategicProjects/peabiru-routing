//! Tag interpretation for street edges: which ways are routable, per-mode
//! permissions and one-way rules, car speeds and bicycle level of traffic
//! stress (LTS).
//!
//! These rules follow Conveyal R5 7.6 (`StreetLayer.isWayRoutable`,
//! `USTraversalPermissionLabeler`, `SpeedLabeler` with the default
//! `SpeedConfig`, `LevelOfTrafficStressLabeler`, `TypeOfEdgeLabeler`), MIT
//! licensed, so travel times are R5's. Quirks are kept on purpose and noted
//! where they occur.

use super::flags::*;

/// Value of `key` in a tag list.
#[inline]
pub(crate) fn tag<'a>(tags: &'a [(String, String)], key: &str) -> Option<&'a str> {
    tags.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

#[inline]
fn has(tags: &[(String, String)], key: &str, value: &str) -> bool {
    tag(tags, key) == Some(value)
}

// ---------------------------------------------------------------------------
// Routable ways

/// Whether a way becomes street edges (R5 `isWayRoutable` +
/// `actuallyExistsInReality`).
pub fn is_way_routable(tags: &[(&str, &str)]) -> bool {
    let get = |k: &str| tags.iter().find(|(kk, _)| *kk == k).map(|(_, v)| *v);
    let highway = get("highway");
    let routable = match highway {
        Some(h) => !matches!(h, "services" | "rest_area" | "raceway"),
        None => {
            let platform =
                get("public_transport") == Some("platform") || get("railway") == Some("platform");
            platform && get("usage") != Some("tourism")
        }
    };
    if !routable {
        return false;
    }
    let gone = |v: Option<&str>| {
        matches!(
            v,
            Some("construction" | "abandoned" | "removed" | "proposed" | "propossed" | "unbuilt")
        )
    };
    !(gone(highway)
        || gone(get("cycleway"))
        || get("construction").is_some()
        || get("proposed").is_some())
}

// ---------------------------------------------------------------------------
// Permissions

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Label {
    Yes,
    No,
    NoThruTraffic,
    Unknown,
    Limited,
}

impl Label {
    fn from_tag(v: &str) -> Label {
        let v = v.split(';').next().unwrap_or("").trim().to_lowercase();
        match v.as_str() {
            "yes" | "1" | "true" | "official" | "unknown" | "public" | "permissive"
            | "designated" | "lane" | "track" | "shared_lane" | "share_busway" | "crossing" => {
                Label::Yes
            }
            "no" | "0" | "false" | "license" | "restricted" | "prohibited" | "emergency"
            | "use_sidepath" | "dismount" => Label::No,
            "destination" | "customers" | "delivery" | "forestry" | "agricultural"
            | "residents" | "resident" | "customer" | "private" => Label::NoThruTraffic,
            "limited" => Label::Limited,
            _ => Label::Unknown,
        }
    }
}

// Permission tree nodes: ACCESS -> {FOOT -> WHEELCHAIR, VEHICLE -> {BICYCLE, CAR}}
const ACCESS: usize = 0;
const FOOT: usize = 1;
const VEHICLE: usize = 2;
const BICYCLE: usize = 4;
const CAR: usize = 5;
const N_NODES: usize = 6;

fn parent(n: usize) -> Option<usize> {
    match n {
        FOOT | VEHICLE => Some(ACCESS),
        BICYCLE | CAR => Some(VEHICLE),
        3 => Some(FOOT), // wheelchair
        _ => None,
    }
}

fn children(n: usize) -> &'static [usize] {
    match n {
        ACCESS => &[FOOT, VEHICLE],
        VEHICLE => &[BICYCLE, CAR],
        FOOT => &[3],
        _ => &[],
    }
}

type Tree = [Label; N_NODES];

/// Default permission tree for a `highway` value (US labeler).
fn default_tree(highway: &str) -> Tree {
    use Label::*;
    let mut t = [Unknown; N_NODES];
    match highway {
        "motorway" | "motorway_link" => {
            t[ACCESS] = Yes;
            t[BICYCLE] = No;
            t[FOOT] = No;
        }
        "trunk" | "trunk_link" | "primary" | "primary_link" | "secondary" | "secondary_link"
        | "tertiary" | "tertiary_link" | "unclassified" | "residential" | "living_street"
        | "road" | "service" | "track" => t[ACCESS] = Yes,
        "pedestrian" => {
            t[ACCESS] = No;
            t[FOOT] = Yes;
            t[BICYCLE] = Yes; // US addition
        }
        "path" => {
            t[ACCESS] = No;
            t[FOOT] = Yes;
            t[BICYCLE] = Yes;
        }
        "bridleway" | "cycleway" => {
            t[ACCESS] = No;
            t[BICYCLE] = Yes; // US addition for bridleway
            t[FOOT] = Yes; // US addition for both
        }
        "footway" | "steps" | "platform" | "corridor" => {
            t[ACCESS] = No;
            t[FOOT] = Yes;
        }
        _ => t[ACCESS] = Yes, // falls back to highway=road
    }
    t
}

fn platform_tree() -> Tree {
    let mut t = [Label::Unknown; N_NODES];
    t[ACCESS] = Label::No;
    t[FOOT] = Label::Yes;
    t
}

fn walk(tree: &Tree, mut n: usize) -> Label {
    loop {
        if tree[n] != Label::Unknown {
            return tree[n];
        }
        match parent(n) {
            Some(p) => n = p,
            None => return Label::Unknown,
        }
    }
}

fn apply(tree: &mut Tree, n: usize, l: Label) {
    if l != Label::Unknown {
        tree[n] = l;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OneWay {
    No,
    Yes,
    Reverse,
}

impl OneWay {
    fn from_tag(v: &str) -> OneWay {
        match v.trim().to_lowercase().as_str() {
            "yes" | "true" | "1" => OneWay::Yes,
            "-1" | "reverse" => OneWay::Reverse,
            _ => OneWay::No,
        }
    }
}

fn set_oneway(tree: &mut [OneWay; N_NODES], n: usize, v: OneWay) {
    tree[n] = v;
    for &c in children(n) {
        set_oneway(tree, c, v);
    }
}

fn directional_tree(tags: &[(String, String)]) -> [OneWay; N_NODES] {
    let mut t = [OneWay::No; N_NODES];
    if has(tags, "highway", "motorway") || has(tags, "junction", "roundabout") {
        set_oneway(&mut t, ACCESS, OneWay::Yes);
    }
    for (key, node) in [
        ("oneway", ACCESS),
        ("oneway:vehicle", VEHICLE),
        ("oneway:motorcar", CAR),
        ("oneway:bicycle", BICYCLE),
    ] {
        if let Some(v) = tag(tags, key) {
            set_oneway(&mut t, node, OneWay::from_tag(v));
        }
    }
    let mut sidepath = OneWay::No;
    if let Some(v) = tag(tags, "bicycle:forward") {
        if Label::from_tag(v) == Label::No {
            sidepath = OneWay::Reverse;
        }
    }
    if let Some(v) = tag(tags, "bicycle:backward") {
        if Label::from_tag(v) == Label::No {
            sidepath = OneWay::Yes;
        }
    }
    if sidepath != OneWay::No {
        set_oneway(&mut t, BICYCLE, sidepath);
    }
    // Pedestrians ignore oneway; only oneway:foot applies.
    set_oneway(&mut t, FOOT, OneWay::No);
    if let Some(v) = tag(tags, "oneway:foot") {
        set_oneway(&mut t, FOOT, OneWay::from_tag(v));
    }
    t
}

/// Forward and backward permission flags of a way (ALLOWS_* and the unused
/// NO_THRU_TRAFFIC_* markers).
pub fn permissions(tags: &[(String, String)]) -> (u32, u32) {
    let mut tree = match tag(tags, "highway") {
        Some(h) => default_tree(h.trim().to_lowercase().as_str()),
        None => platform_tree(), // routable ways without highway are platforms
    };
    if let Some(v) = tag(tags, "access") {
        apply(&mut tree, ACCESS, Label::from_tag(v));
    }
    if let Some(v) = tag(tags, "foot") {
        apply(&mut tree, FOOT, Label::from_tag(v));
    }
    if let Some(v) = tag(tags, "sidewalk") {
        let v = v.to_lowercase();
        if v == "both" || v == "left" || v == "right" {
            apply(&mut tree, FOOT, Label::Yes);
        }
    }
    if let Some(v) = tag(tags, "vehicle") {
        apply(&mut tree, VEHICLE, Label::from_tag(v));
    }
    if let Some(v) = tag(tags, "bicycle") {
        apply(&mut tree, BICYCLE, Label::from_tag(v));
    }
    if let Some(v) = tag(tags, "cycleway") {
        let l = Label::from_tag(v);
        if l != Label::No {
            apply(&mut tree, BICYCLE, l);
        }
    }
    if let Some(v) = tag(tags, "cycleway:both") {
        apply(&mut tree, BICYCLE, Label::from_tag(v));
    }
    if let Some(v) = tag(tags, "motor_vehicle") {
        apply(&mut tree, CAR, Label::from_tag(v));
    }
    if let Some(v) = tag(tags, "motorcar") {
        apply(&mut tree, CAR, Label::from_tag(v));
    }

    let mut base = 0u32;
    match walk(&tree, FOOT) {
        Label::Yes => base |= ALLOWS_PEDESTRIAN,
        Label::NoThruTraffic => base |= NO_THRU_TRAFFIC_PEDESTRIAN,
        _ => {}
    }
    match walk(&tree, BICYCLE) {
        Label::Yes => base |= ALLOWS_BIKE,
        Label::NoThruTraffic => base |= NO_THRU_TRAFFIC_BIKE,
        _ => {}
    }
    match walk(&tree, CAR) {
        Label::Yes => base |= ALLOWS_CAR,
        Label::NoThruTraffic => base |= NO_THRU_TRAFFIC_CAR,
        _ => {}
    }
    let (mut fwd, mut bwd) = (base, base);

    // cycleway:right/left = yes-like add bike in that direction
    if tag(tags, "cycleway:right").map(Label::from_tag) == Some(Label::Yes) {
        fwd |= ALLOWS_BIKE;
    }
    if tag(tags, "cycleway:left").map(Label::from_tag) == Some(Label::Yes) {
        bwd |= ALLOWS_BIKE;
    }

    let dir = directional_tree(tags);
    for (node, flag) in [
        (CAR, ALLOWS_CAR),
        (BICYCLE, ALLOWS_BIKE),
        (FOOT, ALLOWS_PEDESTRIAN),
    ] {
        match dir[node] {
            OneWay::Yes => bwd &= !flag,
            OneWay::Reverse => fwd &= !flag,
            OneWay::No => {}
        }
    }
    // Contraflow cycling. Note R5 picks the edge to fix by the CAR direction.
    let opposite_target = if dir[CAR] == OneWay::Reverse {
        &mut fwd
    } else {
        &mut bwd
    };
    // R5 checks cycleway, then cycleway:right, then cycleway:left; any one
    // starting with "opposite" adds the bike permission.
    let starts_opposite = |k: &str| tag(tags, k).is_some_and(|v| v.starts_with("opposite"));
    if starts_opposite("cycleway")
        || starts_opposite("cycleway:right")
        || starts_opposite("cycleway:left")
    {
        *opposite_target |= ALLOWS_BIKE;
    }
    (fwd, bwd)
}

/// LINKABLE on both directions unless a tunnel, covered way or motorway.
pub fn linkable(tags: &[(String, String)]) -> bool {
    !(has(tags, "tunnel", "yes") || has(tags, "covered", "yes") || has(tags, "highway", "motorway"))
}

// ---------------------------------------------------------------------------
// Car speeds

/// R5 default car speeds (mph, converted to m/s as `f32`) by highway class.
fn default_speed_ms(highway: &str) -> f32 {
    let mph: f64 = match highway {
        "motorway" => 65.0,
        "motorway_link" | "trunk_link" | "secondary" => 35.0,
        "trunk" => 55.0,
        "primary" => 45.0,
        "primary_link" | "secondary_link" | "tertiary" | "tertiary_link" => 25.0,
        "living_street" => 5.0,
        _ => 25.0, // defaultSpeed
    };
    (mph * 0.44704) as f32
}

/// `maxspeed`-style value to m/s (R5 `SpeedLabeler.getMetersSecondFromSpeed`):
/// at least two characters of digits/dots, optional space and unit.
fn speed_tag_ms(v: &str) -> Option<f32> {
    let num_len = v
        .bytes()
        .take_while(|b| b.is_ascii_digit() || *b == b'.')
        .count();
    if num_len < 2 || !v.as_bytes()[0].is_ascii_digit() {
        return None;
    }
    let (num, rest) = v.split_at(num_len);
    let unit = rest.strip_prefix(' ').unwrap_or(rest);
    if rest.starts_with(' ') && unit.is_empty() {
        return None; // a lone trailing space does not match
    }
    let factor: f32 = match unit {
        "" | "kmh" | "km/h" | "kmph" | "kph" => 0.277778,
        "mph" => 0.446944,
        "knots" => 0.514444,
        _ => return None,
    };
    let x = parse_java_double(num)? as f32;
    Some(factor * x)
}

/// `Double.parseDouble` on a string of digits and dots.
fn parse_java_double(s: &str) -> Option<f64> {
    if s.matches('.').count() > 1 {
        return None;
    }
    s.parse::<f64>().ok()
}

/// Car speed in m/s for the forward (`back = false`) or backward edge.
pub fn car_speed_ms(tags: &[(String, String)], back: bool) -> f32 {
    let mut speed = tag(tags, "maxspeed:motorcar").and_then(speed_tag_ms);
    if speed.is_none() && !back {
        speed = tag(tags, "maxspeed:forward").and_then(speed_tag_ms);
    }
    if speed.is_none() && back {
        speed = tag(tags, "maxspeed:reverse").and_then(speed_tag_ms);
    }
    if speed.is_none() {
        if let Some(lanes) = tag(tags, "maxspeed:lanes") {
            for lane in lanes.split('|') {
                if let Some(s) = speed_tag_ms(lane) {
                    if speed.map_or(true, |cur| s > cur) {
                        speed = Some(s);
                    }
                }
            }
        }
    }
    if speed.is_none() {
        speed = tag(tags, "maxspeed").and_then(speed_tag_ms);
    }
    match speed {
        Some(s) if s >= 0.0001 => s,
        _ => match tag(tags, "highway") {
            Some(h) => default_speed_ms(h.trim().to_lowercase().as_str()),
            None => default_speed_ms(""),
        },
    }
}

/// Speed in m/s to the stored centimetres per second (`(short) Math.round`).
///
/// Clamped to `1..=i16::MAX` cm/s. R5 stores `(short) Math.round(...)`, which
/// wraps for absurd `maxspeed` tags (over ~1180 km/h) and can give 0 for tiny
/// ones; such values would make edge costs negative or divide by zero. Every
/// real speed is inside the range, so results there are unchanged.
pub fn speed_to_cms(speed_ms: f32) -> i16 {
    let x = (speed_ms * 100.0f32 + 0.5f32).floor();
    if x.is_nan() {
        return 1;
    }
    (x as i32).clamp(1, i32::from(i16::MAX)) as i16
}

/// Stored centimetres per second back to m/s (`(float)(short / 100.0)`).
#[inline]
pub fn cms_to_ms(cms: i16) -> f32 {
    (f64::from(cms) / 100.0) as f32
}

// ---------------------------------------------------------------------------
// Level of traffic stress

/// `maxspeed` in km/h for the LTS rules; NaN when it does not parse.
fn lts_speed_kmh(v: &str) -> f64 {
    let num_len = v
        .bytes()
        .take_while(|b| b.is_ascii_digit() || *b == b'.')
        .count();
    if num_len < 1 || !v.as_bytes()[0].is_ascii_digit() {
        return f64::NAN;
    }
    let (num, rest) = v.split_at(num_len);
    let unit = rest.strip_prefix(' ').unwrap_or(rest);
    let factor = match unit {
        "" | "km/h" | "kmh" | "kph" => 1.0,
        "mph" => 1.609,
        "knots" => 1.852,
        _ => return f64::NAN,
    };
    match parse_java_double(num) {
        Some(x) => x * factor,
        None => f64::NAN,
    }
}

/// Java `Integer.parseInt`: optional sign, decimal digits, fits in i32.
fn parse_java_int(s: &str) -> Option<i32> {
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<i32>().ok()
}

fn lts_flag(lts: i32) -> u32 {
    match lts {
        1 => BIKE_LTS_1,
        2 => BIKE_LTS_2,
        3 => BIKE_LTS_3,
        _ => BIKE_LTS_4,
    }
}

/// Add the initial LTS flags to an edge pair's flags (R5 `label`).
pub fn label_lts(tags: &[(String, String)], fwd: &mut u32, bwd: &mut u32) {
    if let Some(v) = tag(tags, "lts") {
        if let Ok(x) = v.trim().parse::<f64>() {
            let f = lts_flag(x as i32) | BIKE_LTS_EXPLICIT;
            *fwd |= f;
            *bwd |= f;
            return;
        }
    }
    if (*fwd | *bwd) & ALLOWS_CAR == 0 {
        *fwd |= BIKE_LTS_1;
        *bwd |= BIKE_LTS_1;
        return;
    }
    if has(tags, "highway", "service") {
        return;
    }
    if has(tags, "highway", "residential") || has(tags, "highway", "living_street") {
        *fwd |= BIKE_LTS_1;
        *bwd |= BIKE_LTS_1;
        return;
    }
    let mut fwd_lane = false;
    let mut bwd_lane = false;
    if has(tags, "cycleway", "lane") {
        fwd_lane = true;
        bwd_lane = true;
    }
    if has(tags, "cycleway:left", "lane")
        || has(tags, "cycleway", "opposite")
        || has(tags, "cycleway:right", "opposite")
    {
        bwd_lane = true;
    }
    if has(tags, "cycleway:left", "opposite") || has(tags, "cycleway:right", "lane") {
        fwd_lane = true;
    }
    let max_speed = tag(tags, "maxspeed").map_or(f64::NAN, lts_speed_kmh);
    let lanes = tag(tags, "lanes")
        .and_then(parse_java_int)
        .unwrap_or(i32::MAX);
    let limit = 25.0 * 1.61;
    let mut default = BIKE_LTS_3;
    if lanes <= 3 && max_speed <= limit {
        default = BIKE_LTS_2;
    }
    if lanes == i32::MAX && max_speed <= limit {
        default = BIKE_LTS_2;
    }
    if has(tags, "highway", "unclassified")
        || has(tags, "highway", "tertiary")
        || has(tags, "highway", "tertiary_link")
    {
        if lanes <= 3 && max_speed.is_nan() {
            default = BIKE_LTS_2;
        }
        *fwd |= if fwd_lane { BIKE_LTS_2 } else { default };
        *bwd |= if bwd_lane { BIKE_LTS_2 } else { default };
    } else {
        if fwd_lane {
            *fwd |= default;
        }
        if bwd_lane {
            *bwd |= default;
        }
    }
    for f in [fwd, bwd] {
        if *f & LTS_MASK == 0 {
            *f |= BIKE_LTS_4;
        }
    }
}

/// The LTS level of a flag set (1-4), or 0 when unlabelled.
#[inline]
pub fn lts_of(flags: u32) -> i32 {
    // R5 tests 2, 3, 4 in turn; a set may in principle hold several.
    let mut lts = 0;
    if flags & BIKE_LTS_1 != 0 {
        lts = 1;
    }
    if flags & BIKE_LTS_2 != 0 {
        lts = lts.max(2);
    }
    if flags & BIKE_LTS_3 != 0 {
        lts = lts.max(3);
    }
    if flags & BIKE_LTS_4 != 0 {
        lts = lts.max(4);
    }
    lts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn default_car_speeds_match_r5() {
        // stored cm/s from the R5 spec table
        for (h, cms) in [
            ("motorway", 2906),
            ("motorway_link", 1565),
            ("trunk", 2459),
            ("primary", 2012),
            ("secondary", 1565),
            ("tertiary", 1118),
            ("living_street", 224),
            ("residential", 1118),
            ("service", 1118),
        ] {
            let s = car_speed_ms(&t(&[("highway", h)]), false);
            assert_eq!(speed_to_cms(s), cms, "{h}");
        }
    }

    #[test]
    fn maxspeed_parsing() {
        let s = |v: &str| car_speed_ms(&t(&[("highway", "residential"), ("maxspeed", v)]), false);
        assert_eq!(speed_to_cms(s("60")), 1667);
        // absurd values are clamped instead of wrapping or reaching zero
        assert_eq!(speed_to_cms(s("99999")), i16::MAX);
        assert_eq!(speed_to_cms(s("0.01")), 1);
        assert_eq!(speed_to_cms(s("30 mph")), 1341);
        assert_eq!(speed_to_cms(s("30mph")), 1341);
        // single digit, "none" and multi-values fall back to the default
        for bad in ["5", "none", "50;60", "BR:urban", "50 "] {
            assert_eq!(speed_to_cms(s(bad)), 1118, "{bad}");
        }
        // backward edge reads maxspeed:reverse, not maxspeed:backward
        let tags = t(&[
            ("highway", "primary"),
            ("maxspeed:backward", "30"),
            ("maxspeed:forward", "80"),
        ]);
        assert_eq!(speed_to_cms(car_speed_ms(&tags, true)), 2012);
        assert_eq!(speed_to_cms(car_speed_ms(&tags, false)), 2222);
    }

    #[test]
    fn permissions_and_oneway() {
        let (f, b) = permissions(&t(&[("highway", "residential"), ("oneway", "yes")]));
        assert_eq!(f & ALL_ALLOWS, ALLOWS_PEDESTRIAN | ALLOWS_BIKE | ALLOWS_CAR);
        assert_eq!(b & ALL_ALLOWS, ALLOWS_PEDESTRIAN); // bikes inherit oneway
        let (_, b) = permissions(&t(&[
            ("highway", "residential"),
            ("oneway", "yes"),
            ("cycleway", "opposite_lane"),
        ]));
        assert_eq!(b & ALL_ALLOWS, ALLOWS_PEDESTRIAN | ALLOWS_BIKE);
        let (f, b) = permissions(&t(&[("highway", "motorway")]));
        assert_eq!((f & ALL_ALLOWS, b & ALL_ALLOWS), (ALLOWS_CAR, 0));
        let (f, _) = permissions(&t(&[("highway", "footway")]));
        assert_eq!(f & ALL_ALLOWS, ALLOWS_PEDESTRIAN);
        let (f, _) = permissions(&t(&[("highway", "cycleway")]));
        assert_eq!(f & ALL_ALLOWS, ALLOWS_PEDESTRIAN | ALLOWS_BIKE);
        // private access blocks every mode (no-thru-traffic is never routed)
        let (f, _) = permissions(&t(&[("highway", "service"), ("access", "private")]));
        assert_eq!(f & ALL_ALLOWS, 0);
        // unknown highway values fall back to road: everything allowed
        let (f, _) = permissions(&t(&[("highway", "busway")]));
        assert_eq!(f & ALL_ALLOWS, ALLOWS_PEDESTRIAN | ALLOWS_BIKE | ALLOWS_CAR);
    }

    #[test]
    fn lts_rules() {
        let lts = |pairs: &[(&str, &str)]| {
            let tags = t(pairs);
            let (mut f, mut b) = permissions(&tags);
            label_lts(&tags, &mut f, &mut b);
            (lts_of(f), lts_of(b))
        };
        assert_eq!(lts(&[("highway", "residential")]), (1, 1));
        assert_eq!(lts(&[("highway", "footway")]), (1, 1));
        assert_eq!(lts(&[("highway", "service")]), (0, 0));
        assert_eq!(lts(&[("highway", "primary")]), (4, 4));
        assert_eq!(lts(&[("highway", "primary"), ("cycleway", "lane")]), (3, 3));
        assert_eq!(
            lts(&[
                ("highway", "primary"),
                ("cycleway", "lane"),
                ("maxspeed", "40")
            ]),
            (2, 2)
        );
        assert_eq!(lts(&[("highway", "tertiary"), ("lanes", "2")]), (2, 2));
        assert_eq!(lts(&[("highway", "tertiary")]), (3, 3));
    }
}
