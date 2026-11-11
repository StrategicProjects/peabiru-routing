//! Transit fares as r5r 2.5.0 computes them: its fare structure (the
//! object of `setup_fare_structure()`, `read_fare_structure()` and
//! `write_fare_structure()`) and its `RuleBasedInRoutingFareCalculator`,
//! which R5's multi-criteria router (McRAPTOR) calls for every state.
//!
//! A [`FareStructure`] holds the values as r5r's R object holds them
//! (doubles, `Inf` included). r5r hands the object to Java as JSON
//! (`jsonlite::toJSON`, four decimal places), where Jackson reads the
//! fares into `float`s and the counts into `int`s, and the calculator works
//! in integer cents (`Math.round(fare * 100.0f)`); [`FareStructure`] goes
//! through the same conversions before routing, so that the same object
//! gives the same fares as in r5r.
//!
//! The calculator, the structure and its builder are translations to Rust,
//! by the peabiru authors, of r5r's Java classes `FareStructure`,
//! `FareStructureBuilder` and `RuleBasedInRoutingFareCalculator`
//! (Copyright (c) 2020 r5r contributors, MIT licence); see
//! `LICENSE-THIRD-PARTY`.

pub(crate) mod zipio;

pub use zipio::{read_fare_structure, write_fare_structure};

use crate::jcompat::{string_hash, JavaHashMap};
use crate::transit::raptor::route_type_mode;
use crate::{RouteError, TransitLayer};

/// One fare type of a [`FareStructure`] (r5r's `fares_per_type` row).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct FarePerType {
    /// The name of the type (r5r's `type`); `None` is R's `NA`.
    pub fare_type: Option<String>,
    /// Rides of this type after one of the same type are free and do not
    /// use up a discounted transfer.
    pub unlimited_transfers: bool,
    /// A discounted transfer between two rides of this type may be on the
    /// same route.
    pub allow_same_route_transfer: bool,
    /// The routes of this type cost their own `route_fare`; otherwise they
    /// cost the type's `fare`.
    pub use_route_fare: bool,
    /// The fare of the type.
    pub fare: f64,
}

/// A discounted transfer between two fare types (r5r's
/// `fares_per_transfer` row): both rides together cost `fare`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct FarePerTransfer {
    /// The fare type of the first ride (`None` is R's `NA`).
    pub first_leg: Option<String>,
    /// The fare type of the second ride.
    pub second_leg: Option<String>,
    /// The fare of the two rides together.
    pub fare: f64,
}

/// The fare type and fare of one GTFS route (r5r's `fares_per_route` row).
/// A route is matched by `agency_id` and `route_id`; the other names are
/// for the reader.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[allow(missing_docs)]
pub struct FarePerRoute {
    pub agency_id: Option<String>,
    pub agency_name: Option<String>,
    pub route_id: Option<String>,
    pub route_short_name: Option<String>,
    pub route_long_name: Option<String>,
    pub mode: Option<String>,
    /// The route's own fare, used when its type has `use_route_fare`.
    pub route_fare: f64,
    /// The route's fare type, one of the `fares_per_type` types.
    pub fare_type: Option<String>,
}

/// r5r's fare structure: how much each ride costs and which transfers are
/// discounted. The values are those of r5r's R object (`setup_fare_structure()`,
/// `read_fare_structure()`): counts and minutes as doubles, `Inf` for no
/// limit. Start from [`setup_fare_structure`] or [`read_fare_structure`]
/// and change the fields.
///
/// The rules, as r5r's `RuleBasedInRoutingFareCalculator` applies them:
/// the first ride costs its route's fare; each later ride costs the
/// transfer fare between the two types minus the fare of the previous ride
/// when a discounted transfer applies (a row of `fares_per_transfer` for
/// the two types, the boardings at most `transfer_time_allowance` minutes
/// apart, not the same route unless the type allows it, and fewer than
/// `max_discounted_transfers` discounts used so far), and its full fare
/// otherwise; a ride of the same type as the previous one is free when the
/// type has unlimited transfers. The total is capped at `fare_cap`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct FareStructure {
    /// Most transfers with a discount (r5r default 1; `Inf` becomes -1 in
    /// Java, where no transfer is ever discounted).
    pub max_discounted_transfers: f64,
    /// Minutes between the boardings of a discounted transfer (r5r default
    /// 120; `Inf` becomes -1 in Java, where no transfer is ever discounted).
    pub transfer_time_allowance: f64,
    /// Highest total fare of a trip (`Inf`, the default, or 0: no cap).
    pub fare_cap: f64,
    #[allow(missing_docs)]
    pub fares_per_type: Vec<FarePerType>,
    #[allow(missing_docs)]
    pub fares_per_transfer: Vec<FarePerTransfer>,
    #[allow(missing_docs)]
    pub fares_per_route: Vec<FarePerRoute>,
    /// r5r's `debug_settings$output_file` (r5r 2.5.0 writes no debug file,
    /// whatever it says, and neither does peabiru).
    pub debug_output_file: String,
    /// r5r's `debug_settings$trip_info`.
    pub debug_trip_info: String,
}

impl Default for FareStructure {
    /// An empty structure with r5r's global defaults.
    fn default() -> Self {
        FareStructure {
            max_discounted_transfers: 1.0,
            transfer_time_allowance: 120.0,
            fare_cap: f64::INFINITY,
            fares_per_type: Vec::new(),
            fares_per_transfer: Vec::new(),
            fares_per_route: Vec::new(),
            debug_output_file: String::new(),
            debug_trip_info: "MODE".into(),
        }
    }
}

impl Default for FarePerType {
    fn default() -> Self {
        FarePerType {
            fare_type: None,
            unlimited_transfers: false,
            allow_same_route_transfer: false,
            use_route_fare: false,
            fare: 0.0,
        }
    }
}

impl Default for FarePerTransfer {
    fn default() -> Self {
        FarePerTransfer {
            first_leg: None,
            second_leg: None,
            fare: 0.0,
        }
    }
}

impl Default for FarePerRoute {
    fn default() -> Self {
        FarePerRoute {
            agency_id: None,
            agency_name: None,
            route_id: None,
            route_short_name: None,
            route_long_name: None,
            mode: None,
            route_fare: 0.0,
            fare_type: None,
        }
    }
}

/// The value of a Java `float` as R reads it back from Jackson's JSON
/// (`Float.toString`, then a double): `4.8f` gives `4.8`.
pub(crate) fn float_to_r(f: f32) -> f64 {
    // Rust prints the shortest decimal that reads back as the same float,
    // as Java's `Float.toString` does (JDK 19 and later)
    format!("{f}").parse().unwrap_or(f64::from(f))
}

/// The text `jsonlite::toJSON(x, digits = 4)` writes for a number
/// (`num_to_char`: `modp_dtoa2` with four decimals for magnitudes between
/// 1e-5 and 2^31 - 1, `%.*g` otherwise).
pub(crate) fn jsonlite_number(x: f64) -> String {
    const PREC: i32 = 4;
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Inf".into() } else { "-Inf".into() };
    }
    if x.abs() < 2_147_483_647.0 && x.abs() > 1e-5 {
        return modp_dtoa2(x, PREC);
    }
    let decimals = (15f64.min(1f64.max(x.log10()) + f64::from(PREC))).ceil() as usize;
    c_format_g(x, decimals.max(1))
}

/// `modp_dtoa2` of stringencoders, as jsonlite uses it.
fn modp_dtoa2(value: f64, prec: i32) -> String {
    const POW10: [f64; 10] = [1., 10., 100., 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9];
    let neg = value < 0.0;
    let value = value.abs();
    let mut whole = value as i32;
    let tmp = (value - f64::from(whole)) * POW10[prec as usize];
    let mut frac = tmp as u32;
    let diff = tmp - f64::from(frac);
    if diff > 0.5 {
        frac += 1;
        if f64::from(frac) >= POW10[prec as usize] {
            frac = 0;
            whole += 1;
        }
    } else if diff == 0.5 && frac & 1 == 1 {
        // a tie goes to the even last digit (jsonlite 2.0's modp_dtoa2)
        frac += 1;
        if f64::from(frac) >= POW10[prec as usize] {
            frac = 0;
            whole += 1;
        }
    }
    let mut s = String::new();
    if neg {
        s.push('-');
    }
    s.push_str(&whole.to_string());
    if frac != 0 {
        let mut count = prec;
        while frac % 10 == 0 {
            count -= 1;
            frac /= 10;
        }
        let digits = frac.to_string();
        s.push('.');
        for _ in digits.len() as i32..count {
            s.push('0');
        }
        s.push_str(&digits);
    }
    s
}

/// C's `%.*g` with `p` significant digits (only its value matters here,
/// which `parse` reads back).
fn c_format_g(x: f64, p: usize) -> String {
    format!("{:.*e}", p - 1, x)
}

/// Jackson reading a JSON number into a Java `float`: the double the text
/// stands for, rounded to a float (Jackson 2.10 `getFloatValue`).
fn jackson_float(text: &str) -> f32 {
    text.parse::<f64>().map_or(f32::NAN, |d| d as f32)
}

/// Jackson reading a JSON number into a Java `int`: an integer literal
/// that must fit, or a decimal truncated toward zero
/// (`ACCEPT_FLOAT_AS_INT`) when within range.
fn jackson_int(text: &str, name: &str) -> Result<i32, String> {
    let out_of_range = || format!("{name} ({text}) is out of the range of a Java int");
    if text.contains(['.', 'e', 'E']) {
        let d: f64 = text.parse().map_err(|_| out_of_range())?;
        if d < f64::from(i32::MIN) || d > f64::from(i32::MAX) {
            return Err(out_of_range());
        }
        Ok(d as i32)
    } else {
        text.parse::<i32>().map_err(|_| out_of_range())
    }
}

/// Java `Math.round(float)`.
fn java_round(f: f32) -> i32 {
    // floor(f + 0.5) is exact in f64 for any float; `as` saturates, NaN -> 0
    (f64::from(f) + 0.5).floor() as i32
}

/// Integer cents of a fare: `Math.round(fare * 100.0f)`.
fn cents(f: f32) -> i32 {
    java_round(f * 100.0f32)
}

/// A number of the structure as Java reads it from r5r's JSON.
fn as_float(x: f64) -> f32 {
    jackson_float(&jsonlite_number(x))
}

/// `TransitLayer.getTransitModes(route_type).toString()`.
fn mode_name(route_type: i32) -> Result<&'static str, String> {
    match route_type_mode(route_type) {
        Some(Some(m)) => Ok(m.name()),
        Some(None) => Ok("AIR"),
        None => Err(format!("unsupported GTFS route_type {route_type}")),
    }
}

/// r5r's `setup_fare_structure()`: one fare type per mode (`by =
/// "MODE"`), per agency id (`"AGENCY_ID"`, or `"AGENCY"`) or name
/// (`"AGENCY_NAME"`), or a single `GENERIC` type (anything else); every
/// route, type and transfer between two types costs `base_fare`, read as a
/// Java `float`. The routes are in the network's route order, the types
/// and transfers in the order of r5r's Java `HashSet`s.
pub fn setup_fare_structure(
    layer: &TransitLayer,
    base_fare: f64,
    by: &str,
) -> Result<FareStructure, RouteError> {
    // rJava::.jfloat(base_fare), and Jackson writing the float back
    let fare_f = base_fare as f32;
    if !fare_f.is_finite() {
        return Err(RouteError::Invalid(format!(
            "base_fare must be finite, got {base_fare}"
        )));
    }
    let fare = float_to_r(fare_f);
    let mut types: JavaHashMap<Option<String>> = JavaHashMap::new();
    let mut routes = Vec::with_capacity(layer.routes.len());
    for r in &layer.routes {
        let mode = mode_name(r.route_type).map_err(RouteError::Invalid)?;
        let t = match by {
            "MODE" => Some(mode.to_string()),
            "AGENCY" | "AGENCY_ID" => r.agency_id.clone(),
            "AGENCY_NAME" => r.agency_name.clone(),
            _ => Some("GENERIC".to_string()),
        };
        let h = string_hash(t.as_deref());
        types.insert(t.clone(), h).map_err(RouteError::Invalid)?;
        routes.push(FarePerRoute {
            agency_id: r.agency_id.clone(),
            agency_name: r.agency_name.clone(),
            route_id: Some(r.route_id.clone()),
            route_short_name: r.short_name.clone(),
            route_long_name: r.long_name.clone(),
            mode: Some(mode.to_string()),
            route_fare: fare,
            fare_type: t,
        });
    }
    let names: Vec<Option<String>> = types.order().map(|i| types.key(i).clone()).collect();
    // transfers: a HashSet of "a&b" strings (null is "null"), split on "&"
    let mut transfers: JavaHashMap<String> = JavaHashMap::new();
    for a in &names {
        for b in &names {
            let s = format!(
                "{}&{}",
                a.as_deref().unwrap_or("null"),
                b.as_deref().unwrap_or("null")
            );
            let h = string_hash(Some(&s));
            transfers.insert(s, h).map_err(RouteError::Invalid)?;
        }
    }
    let mut fares_per_transfer = Vec::new();
    for i in transfers.order() {
        let legs = java_split_amp(transfers.key(i));
        if legs.len() < 2 {
            return Err(RouteError::Invalid(format!(
                "r5r cannot build this fare structure: the transfer {:?} does not split into \
                 two fare types (Java ArrayIndexOutOfBoundsException)",
                transfers.key(i)
            )));
        }
        fares_per_transfer.push(FarePerTransfer {
            first_leg: Some(legs[0].clone()),
            second_leg: Some(legs[1].clone()),
            fare,
        });
    }
    Ok(FareStructure {
        fares_per_type: names
            .into_iter()
            .map(|t| FarePerType {
                fare_type: t,
                fare,
                ..FarePerType::default()
            })
            .collect(),
        fares_per_transfer,
        fares_per_route: routes,
        ..FareStructure::default()
    })
}

/// Java's `s.split("&")`: the pieces, trailing empty ones dropped.
fn java_split_amp(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s.split('&').map(str::to_string).collect();
    while v.last().is_some_and(String::is_empty) {
        v.pop();
    }
    v
}

impl FareStructure {
    /// The checks of r5r's `assert_fare_structure()` on the values: counts,
    /// minutes and cap not negative (nor `NA`), fare types present and
    /// unique, fares finite and not negative, every transfer and route
    /// naming one of the types, and no missing value in the route table.
    pub fn validate(&self) -> Result<(), RouteError> {
        self.check().map_err(RouteError::Invalid)
    }

    /// [`FareStructure::validate`], then the checks r5r makes as soon as it
    /// is given the structure for a network (`setFareCalculator`, even for
    /// a street-only trip): its numbers as Java reads them, and a row of
    /// `fares_per_route` for the `agency_id` and `route_id` of every
    /// route the network's trip patterns run on.
    pub fn validate_for(&self, layer: &TransitLayer) -> Result<(), RouteError> {
        FareCalculator::new(self, layer).map(|_| ())
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        let number = |name: &str, x: f64| {
            if x.is_nan() || x < 0.0 {
                Err(format!(
                    "fare_structure${name} must be a number >= 0, got {x}"
                ))
            } else {
                Ok(())
            }
        };
        number("max_discounted_transfers", self.max_discounted_transfers)?;
        number("transfer_time_allowance", self.transfer_time_allowance)?;
        number("fare_cap", self.fare_cap)?;
        let fare = |table: &str, k: usize, x: f64| {
            if !x.is_finite() || x < 0.0 {
                Err(format!(
                    "fare_structure${table} row {}: fares must be finite and >= 0, got {x}",
                    k + 1
                ))
            } else {
                Ok(())
            }
        };
        let mut seen = std::collections::HashSet::new();
        for (k, t) in self.fares_per_type.iter().enumerate() {
            let Some(name) = &t.fare_type else {
                return Err(format!(
                    "fare_structure$fares_per_type row {}: type is missing",
                    k + 1
                ));
            };
            if !seen.insert(name.as_str()) {
                return Err(format!(
                    "fare_structure$fares_per_type: type {name:?} is repeated"
                ));
            }
            fare("fares_per_type", k, t.fare)?;
        }
        let known = |table: &str, col: &str, k: usize, v: &Option<String>| match v {
            None => Err(format!(
                "fare_structure${table} row {}: {col} is missing",
                k + 1
            )),
            Some(s) if !seen.contains(s.as_str()) => Err(format!(
                "fare_structure${table} row {}: {col} {s:?} is not a type of fares_per_type",
                k + 1
            )),
            _ => Ok(()),
        };
        for (k, t) in self.fares_per_transfer.iter().enumerate() {
            known("fares_per_transfer", "first_leg", k, &t.first_leg)?;
            known("fares_per_transfer", "second_leg", k, &t.second_leg)?;
            fare("fares_per_transfer", k, t.fare)?;
        }
        for (k, r) in self.fares_per_route.iter().enumerate() {
            for (col, v) in [
                ("agency_id", &r.agency_id),
                ("agency_name", &r.agency_name),
                ("route_id", &r.route_id),
                ("route_short_name", &r.route_short_name),
                ("route_long_name", &r.route_long_name),
                ("mode", &r.mode),
            ] {
                if v.is_none() {
                    return Err(format!(
                        "fare_structure$fares_per_route row {}: {col} is missing",
                        k + 1
                    ));
                }
            }
            fare("fares_per_route", k, r.route_fare)?;
            known("fares_per_route", "fare_type", k, &r.fare_type)?;
        }
        Ok(())
    }
}

/// The type table of the calculator.
#[derive(Debug, Clone)]
struct TypeRule {
    unlimited: bool,
    same_route: bool,
}

/// A discounted transfer, in cents, with the full fares of both types.
#[derive(Debug, Clone, Copy)]
struct TransferRule {
    fare: i32,
    first_full: i32,
    second_full: i32,
}

/// r5r's `RuleBasedInRoutingFareCalculator` set up for a network: per
/// pattern its route's fare type, fare in cents and `route_id`.
#[derive(Debug, Clone)]
pub(crate) struct FareCalculator {
    max_discounted: i32,
    allowance_s: i32,
    cap_positive: bool,
    cap: i32,
    types: Vec<TypeRule>,
    /// `[first][second]`.
    transfers: Vec<Vec<Option<TransferRule>>>,
    /// Per pattern: (type index, cents, index of its `FarePerRoute`).
    patterns: Vec<(u32, i32, u32)>,
    /// `route_id` of each `FarePerRoute` (the same-route check compares
    /// these).
    route_ids: Vec<Option<String>>,
}

/// What a state has paid and the transfer allowance it holds (R5's
/// `FareBounds` with r5r's `R5RTransferAllowance`). R5 compares
/// allowances by value, number and expiration only: r5r's overload that
/// also compares the fare type is never called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FareBounds {
    pub(crate) paid: i32,
    pub(crate) value: i32,
    pub(crate) number: i32,
    pub(crate) expiration: i32,
}

impl FareCalculator {
    /// `RuleBasedInRoutingFareCalculator(transitLayer, json)` with
    /// `loadFareInformation()`: the structure as Java reads it, every
    /// pattern's route looked up by `agency_id:route_id`.
    pub(crate) fn new(s: &FareStructure, layer: &TransitLayer) -> Result<Self, RouteError> {
        s.check().map_err(RouteError::Invalid)?;
        let inv = RouteError::Invalid;
        // set_fare_structure(): Inf becomes -1, then JSON and Jackson
        let java_int = |x: f64, name: &str| -> Result<i32, RouteError> {
            let x = if x == f64::INFINITY { -1.0 } else { x };
            jackson_int(&jsonlite_number(x), name).map_err(inv)
        };
        let max_discounted = java_int(s.max_discounted_transfers, "max_discounted_transfers")?;
        let allowance_min = java_int(s.transfer_time_allowance, "transfer_time_allowance")?;
        let cap_f = as_float(if s.fare_cap == f64::INFINITY {
            -1.0
        } else {
            s.fare_cap
        });
        // indexTransportType: HashMap put, a later type with the same name
        // wins (validation rules repeats out)
        let mut type_index = std::collections::HashMap::new();
        let mut types = Vec::with_capacity(s.fares_per_type.len());
        let mut type_cents = Vec::with_capacity(s.fares_per_type.len());
        for (i, t) in s.fares_per_type.iter().enumerate() {
            type_index.insert(t.fare_type.clone(), i as u32);
            types.push(TypeRule {
                unlimited: t.unlimited_transfers,
                same_route: t.allow_same_route_transfer,
            });
            type_cents.push(cents(as_float(t.fare)));
        }
        let lookup = |t: &Option<String>| -> Result<u32, RouteError> {
            type_index.get(t).copied().ok_or_else(|| {
                RouteError::Invalid(format!(
                    "fare type {:?} is not in fares_per_type (r5r fails with a Java \
                     NullPointerException)",
                    t.as_deref().unwrap_or("NA")
                ))
            })
        };
        // indexRouteInfo: agency_id:route_id -> the last FarePerRoute
        let mut by_route = std::collections::HashMap::new();
        for (i, r) in s.fares_per_route.iter().enumerate() {
            by_route.insert(
                format!(
                    "{}:{}",
                    r.agency_id.as_deref().unwrap_or(""),
                    r.route_id.as_deref().unwrap_or("")
                ),
                i as u32,
            );
        }
        let mut route_rule: Vec<Option<(u32, i32)>> = vec![None; s.fares_per_route.len()];
        let mut patterns = Vec::with_capacity(layer.patterns.len());
        for pi in 0..layer.patterns.len() {
            let ri = &layer.routes[layer.pattern_route[pi] as usize];
            let key = format!(
                "{}:{}",
                ri.agency_id.as_deref().unwrap_or("null"),
                ri.route_id
            );
            let Some(&k) = by_route.get(&key) else {
                return Err(RouteError::Invalid(format!(
                    "fare_structure$fares_per_route has no row for agency_id {:?} and \
                     route_id {:?} of the network (r5r fails with a Java NullPointerException)",
                    ri.agency_id.as_deref().unwrap_or("NA"),
                    ri.route_id
                )));
            };
            let rule = match route_rule[k as usize] {
                Some(r) => r,
                None => {
                    let r = &s.fares_per_route[k as usize];
                    let ti = lookup(&r.fare_type)?;
                    let t = &s.fares_per_type[ti as usize];
                    let c = if t.use_route_fare {
                        cents(as_float(r.route_fare))
                    } else {
                        type_cents[ti as usize]
                    };
                    route_rule[k as usize] = Some((ti, c));
                    (ti, c)
                }
            };
            patterns.push((rule.0, rule.1, k));
        }
        let n = types.len();
        let mut transfers = vec![vec![None; n]; n];
        for t in &s.fares_per_transfer {
            let (a, b) = (lookup(&t.first_leg)?, lookup(&t.second_leg)?);
            transfers[a as usize][b as usize] = Some(TransferRule {
                fare: cents(as_float(t.fare)),
                first_full: type_cents[a as usize],
                second_full: type_cents[b as usize],
            });
        }
        Ok(FareCalculator {
            max_discounted,
            allowance_s: allowance_min.wrapping_mul(60),
            cap_positive: cap_f > 0.0,
            cap: cents(cap_f),
            types,
            transfers,
            patterns,
            route_ids: s
                .fares_per_route
                .iter()
                .map(|r| r.route_id.clone())
                .collect(),
        })
    }

    fn full(&self, pattern: i32) -> i32 {
        self.patterns[pattern as usize].1
    }

    fn type_of(&self, pattern: i32) -> u32 {
        self.patterns[pattern as usize].0
    }

    /// `isTransferAllowed`.
    fn transfer_allowed(&self, a: i32, b: i32) -> bool {
        let (ta, tb) = (self.type_of(a), self.type_of(b));
        if ta != tb {
            return true;
        }
        if self.types[ta as usize].same_route {
            return true;
        }
        let ra = &self.route_ids[self.patterns[a as usize].2 as usize];
        let rb = &self.route_ids[self.patterns[b as usize].2 as usize];
        ra != rb
    }

    /// `calculateFare` for the rides of a state, `(pattern, board time)`
    /// in travel order.
    pub(crate) fn fare(&self, rides: &[(i32, i32)]) -> FareBounds {
        let empty = |paid| FareBounds {
            paid,
            value: 0,
            number: 0,
            expiration: 0,
        };
        let Some(&(first, first_board)) = rides.first() else {
            return empty(0);
        };
        let mut paid = self.full(first);
        let mut prev = first;
        let mut discounts = 0i32;
        let mut prev_board = first_board;
        let mut current = first;
        let mut current_board = -1;
        let mut last_type = self.type_of(first);
        for &(pattern, board) in &rides[1..] {
            current = pattern;
            current_board = board;
            let (ta, tb) = (self.type_of(prev), self.type_of(current));
            last_type = tb;
            if ta == tb && self.types[ta as usize].unlimited {
                prev = current;
                continue;
            }
            if discounts >= self.max_discounted {
                paid = paid.wrapping_add(self.full(current));
            } else {
                // getIntegrationFare
                let gap = current_board.wrapping_sub(prev_board);
                match self.transfers[ta as usize][tb as usize] {
                    Some(t) if self.transfer_allowed(prev, current) && gap <= self.allowance_s => {
                        paid = paid.wrapping_add(t.fare.wrapping_sub(self.full(prev)));
                        discounts += 1;
                    }
                    _ => paid = paid.wrapping_add(self.full(current)),
                }
            }
            prev = current;
            prev_board = current_board;
        }
        if self.cap_positive {
            paid = paid.min(self.cap);
        }
        let remaining = self.max_discounted.wrapping_sub(discounts);
        if remaining <= 0 {
            return empty(paid);
        }
        let full = self.full(current);
        let mut max_allowance = 0;
        for t in self.transfers[last_type as usize].iter().flatten() {
            let allowance = t
                .first_full
                .wrapping_add(t.second_full)
                .wrapping_sub(t.fare);
            max_allowance = max_allowance.max(allowance);
        }
        if self.cap_positive && paid > self.cap {
            max_allowance = max_allowance.max(full);
        }
        FareBounds {
            paid,
            value: max_allowance,
            number: remaining,
            expiration: current_board.wrapping_add(self.allowance_s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonlite_numbers() {
        for (x, s) in [
            (4.8, "4.8"),
            (8.37, "8.37"),
            (1.0 / 3.0, "0.3333"),
            (2.0 / 3.0, "0.6667"),
            (5.0, "5"),
            (120.0, "120"),
            (-1.0, "-1"),
            (0.00005, "0"),
            (38.48095, "38.481"),
            (6.47865, "6.4786"),
        ] {
            assert_eq!(jsonlite_number(x), s, "{x}");
        }
        assert_eq!(jsonlite_number(0.0).parse::<f64>().unwrap(), 0.0);
        assert_eq!(jsonlite_number(1e-6).parse::<f64>().unwrap(), 1e-6);
        assert_eq!(jackson_int("1.9", "x"), Ok(1));
        assert!(jackson_int("1e10", "x").is_err());
        assert_eq!(cents(as_float(4.8)), 480);
        assert_eq!(cents(as_float(8.37)), 837);
        assert_eq!(float_to_r(4.8f32), 4.8);
    }

    /// Cases written by R (`x` as `%.17g`, then jsonlite's or fwrite's
    /// text) in `$PEABIRU_FARE_CASES/{jsonlite,fwrite}_cases.txt`.
    #[test]
    #[ignore = "needs PEABIRU_FARE_CASES"]
    fn number_formats_match_r() {
        let dir = std::env::var("PEABIRU_FARE_CASES").expect("PEABIRU_FARE_CASES");
        let read = |f: &str| std::fs::read_to_string(format!("{dir}/{f}")).expect("cases");
        let mut bad = 0;
        for line in read("jsonlite_cases.txt").lines() {
            let (x, want) = line.split_once(' ').expect("pair");
            let x: f64 = x.parse().unwrap();
            let got = jsonlite_number(x);
            if got.parse::<f64>().unwrap() != want.parse::<f64>().unwrap() {
                bad += 1;
                eprintln!("jsonlite {x:e}: {got} != {want}");
            }
        }
        for line in read("fwrite_cases.txt").lines() {
            let (x, want) = line.split_once(' ').expect("pair");
            let x: f64 = x.parse().unwrap();
            // data.table's own digit generation can differ from a correct
            // rounding in the 15th digit (and from one platform to
            // another); fares have far fewer digits
            if format!("{x:.13e}").parse::<f64>().unwrap() != x {
                continue;
            }
            let mut got = String::new();
            super::zipio::fwrite_number(x, &mut got);
            if got != want {
                bad += 1;
                eprintln!("fwrite {x}: {got} != {want}");
            }
        }
        assert_eq!(bad, 0);
    }

    #[test]
    fn java_hash_set_order_of_r5r_types() {
        // r5r 2.5.0 setup_fare_structure(by = "MODE") on Porto Alegre:
        // types BUS, RAIL; transfers BUS&BUS, RAIL&BUS, BUS&RAIL, RAIL&RAIL
        let mut types: JavaHashMap<Option<String>> = JavaHashMap::new();
        for t in ["RAIL", "BUS"] {
            types.insert(Some(t.into()), string_hash(Some(t))).unwrap();
        }
        let names: Vec<_> = types
            .order()
            .map(|i| types.key(i).clone().unwrap())
            .collect();
        assert_eq!(names, ["BUS", "RAIL"]);
        let mut tr: JavaHashMap<String> = JavaHashMap::new();
        for a in &names {
            for b in &names {
                let s = format!("{a}&{b}");
                let h = string_hash(Some(&s));
                tr.insert(s, h).unwrap();
            }
        }
        let order: Vec<_> = tr.order().map(|i| tr.key(i).clone()).collect();
        assert_eq!(order, ["BUS&BUS", "RAIL&BUS", "BUS&RAIL", "RAIL&RAIL"]);
    }
}
