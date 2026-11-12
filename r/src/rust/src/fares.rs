//! Fare structures (`setup_fare_structure()`, `read_fare_structure()`,
//! `write_fare_structure()`) and `pareto_frontier()`: r5r's R object
//! converted to the core's `FareStructure` and back.

use super::{err, interruptible, network, points, threads, Outcome};
use extendr_api::prelude::*;
use extendr_api::Result;
use peabiru_core::unstable::{fwrite_csv, zip_deflated, FwriteColumn};
use peabiru_core::{
    pareto_frontier, read_fare_structure, setup_fare_structure, FarePerRoute, FarePerTransfer,
    FarePerType, FareStructure, TransitParams,
};
use std::sync::Arc;

fn field(x: &Robj, name: &str) -> Result<Robj> {
    x.dollar(name)
        .map_err(|_| err(format!("fare_structure${name} is missing")))
}

fn number(x: &Robj, what: &str) -> Result<f64> {
    if let Some(v) = x.as_integer_slice() {
        if v.len() == 1 {
            return Ok(if v[0] == i32::MIN {
                f64::NAN
            } else {
                f64::from(v[0])
            });
        }
    }
    if let Some(v) = x.as_real_slice() {
        if v.len() == 1 {
            return Ok(v[0]);
        }
    }
    Err(err(format!("{what} must be a single number")))
}

fn numbers(x: &Robj, what: &str) -> Result<Vec<f64>> {
    if let Some(v) = x.as_integer_slice() {
        return Ok(v
            .iter()
            .map(|&i| {
                if i == i32::MIN {
                    f64::NAN
                } else {
                    f64::from(i)
                }
            })
            .collect());
    }
    if let Some(v) = x.as_real_slice() {
        return Ok(v.to_vec());
    }
    if x.is_null() {
        return Ok(Vec::new());
    }
    Err(err(format!("{what} must be numeric")))
}

fn texts(x: &Robj, what: &str) -> Result<Vec<Option<String>>> {
    if x.is_null() {
        return Ok(Vec::new());
    }
    let s: Strings = x
        .clone()
        .try_into()
        .map_err(|_| err(format!("{what} must be a character vector")))?;
    Ok(s.iter()
        .map(|r| (!r.is_na()).then(|| r.to_string()))
        .collect())
}

fn logicals(x: &Robj, what: &str) -> Result<Vec<bool>> {
    if x.is_null() {
        return Ok(Vec::new());
    }
    let l: Logicals = x
        .clone()
        .try_into()
        .map_err(|_| err(format!("{what} must be a logical vector")))?;
    l.iter()
        .map(|b| {
            if b.is_na() {
                Err(err(format!("{what} must not hold NA")))
            } else {
                Ok(b.is_true())
            }
        })
        .collect()
}

fn one_text(x: &Robj, what: &str) -> Result<String> {
    match texts(x, what)?.as_slice() {
        [Some(s)] => Ok(s.clone()),
        _ => Err(err(format!("{what} must be a single string"))),
    }
}

/// r5r's fare structure list as the core's structure (the R code has
/// checked its element names).
pub(crate) fn structure_from_r(x: &Robj) -> Result<FareStructure> {
    let mut s = FareStructure::default();
    s.max_discounted_transfers = number(
        &field(x, "max_discounted_transfers")?,
        "fare_structure$max_discounted_transfers",
    )?;
    s.transfer_time_allowance = number(
        &field(x, "transfer_time_allowance")?,
        "fare_structure$transfer_time_allowance",
    )?;
    s.fare_cap = number(&field(x, "fare_cap")?, "fare_structure$fare_cap")?;
    let t = field(x, "fares_per_type")?;
    let col = |df: &Robj, n: &str| df.dollar(n).unwrap_or_else(|_| ().into());
    let names = texts(&col(&t, "type"), "fares_per_type$type")?;
    let unl = logicals(
        &col(&t, "unlimited_transfers"),
        "fares_per_type$unlimited_transfers",
    )?;
    let same = logicals(
        &col(&t, "allow_same_route_transfer"),
        "fares_per_type$allow_same_route_transfer",
    )?;
    let use_route = logicals(&col(&t, "use_route_fare"), "fares_per_type$use_route_fare")?;
    let fare = numbers(&col(&t, "fare"), "fares_per_type$fare")?;
    let n = names.len();
    if [unl.len(), same.len(), use_route.len(), fare.len()]
        .iter()
        .any(|&l| l != n)
    {
        return Err(err("the columns of fares_per_type differ in length"));
    }
    for k in 0..n {
        let mut ft = FarePerType::default();
        ft.fare_type = names[k].clone();
        ft.unlimited_transfers = unl[k];
        ft.allow_same_route_transfer = same[k];
        ft.use_route_fare = use_route[k];
        ft.fare = fare[k];
        s.fares_per_type.push(ft);
    }
    let tr = field(x, "fares_per_transfer")?;
    let a = texts(&col(&tr, "first_leg"), "fares_per_transfer$first_leg")?;
    let b = texts(&col(&tr, "second_leg"), "fares_per_transfer$second_leg")?;
    let f = numbers(&col(&tr, "fare"), "fares_per_transfer$fare")?;
    if b.len() != a.len() || f.len() != a.len() {
        return Err(err("the columns of fares_per_transfer differ in length"));
    }
    for k in 0..a.len() {
        let mut t = FarePerTransfer::default();
        t.first_leg = a[k].clone();
        t.second_leg = b[k].clone();
        t.fare = f[k];
        s.fares_per_transfer.push(t);
    }
    let r = field(x, "fares_per_route")?;
    let cols = [
        "agency_id",
        "agency_name",
        "route_id",
        "route_short_name",
        "route_long_name",
        "mode",
        "fare_type",
    ]
    .map(|c| texts(&col(&r, c), &format!("fares_per_route${c}")));
    let [agency_id, agency_name, route_id, short, long, mode, fare_type] = cols;
    let (agency_id, agency_name, route_id, short, long, mode, fare_type) = (
        agency_id?,
        agency_name?,
        route_id?,
        short?,
        long?,
        mode?,
        fare_type?,
    );
    let route_fare = numbers(&col(&r, "route_fare"), "fares_per_route$route_fare")?;
    let n = route_fare.len();
    if [
        agency_id.len(),
        agency_name.len(),
        route_id.len(),
        short.len(),
        long.len(),
        mode.len(),
        fare_type.len(),
    ]
    .iter()
    .any(|&l| l != n)
    {
        return Err(err("the columns of fares_per_route differ in length"));
    }
    for k in 0..n {
        let mut fr = FarePerRoute::default();
        fr.agency_id = agency_id[k].clone();
        fr.agency_name = agency_name[k].clone();
        fr.route_id = route_id[k].clone();
        fr.route_short_name = short[k].clone();
        fr.route_long_name = long[k].clone();
        fr.mode = mode[k].clone();
        fr.route_fare = route_fare[k];
        fr.fare_type = fare_type[k].clone();
        s.fares_per_route.push(fr);
    }
    let d = field(x, "debug_settings")?;
    s.debug_output_file = one_text(&col(&d, "output_file"), "debug_settings$output_file")?;
    s.debug_trip_info = one_text(&col(&d, "trip_info"), "debug_settings$trip_info")?;
    Ok(s)
}

/// An R integer of a count: `NA` for NaN.
fn count(x: f64) -> Robj {
    if x.is_nan() {
        Robj::from(Option::<i32>::None)
    } else {
        Robj::from(x as i32)
    }
}

/// The core's structure as r5r's list, the tables as lists of columns
/// (the R code makes them data frames).
fn structure_to_r(s: &FareStructure) -> Result<Robj> {
    let t = &s.fares_per_type;
    let types = List::from_names_and_values(
        [
            "type",
            "unlimited_transfers",
            "allow_same_route_transfer",
            "use_route_fare",
            "fare",
        ],
        [
            Robj::from(t.iter().map(|x| x.fare_type.clone()).collect::<Vec<_>>()),
            Robj::from(t.iter().map(|x| x.unlimited_transfers).collect::<Vec<_>>()),
            Robj::from(
                t.iter()
                    .map(|x| x.allow_same_route_transfer)
                    .collect::<Vec<_>>(),
            ),
            Robj::from(t.iter().map(|x| x.use_route_fare).collect::<Vec<_>>()),
            Robj::from(t.iter().map(|x| x.fare).collect::<Vec<_>>()),
        ],
    )?;
    let tr = &s.fares_per_transfer;
    let transfers = list!(
        first_leg = tr.iter().map(|x| x.first_leg.clone()).collect::<Vec<_>>(),
        second_leg = tr.iter().map(|x| x.second_leg.clone()).collect::<Vec<_>>(),
        fare = tr.iter().map(|x| x.fare).collect::<Vec<_>>()
    );
    let r = &s.fares_per_route;
    let routes = list!(
        agency_id = r.iter().map(|x| x.agency_id.clone()).collect::<Vec<_>>(),
        agency_name = r.iter().map(|x| x.agency_name.clone()).collect::<Vec<_>>(),
        route_id = r.iter().map(|x| x.route_id.clone()).collect::<Vec<_>>(),
        route_short_name = r
            .iter()
            .map(|x| x.route_short_name.clone())
            .collect::<Vec<_>>(),
        route_long_name = r
            .iter()
            .map(|x| x.route_long_name.clone())
            .collect::<Vec<_>>(),
        mode = r.iter().map(|x| x.mode.clone()).collect::<Vec<_>>(),
        route_fare = r.iter().map(|x| x.route_fare).collect::<Vec<_>>(),
        fare_type = r.iter().map(|x| x.fare_type.clone()).collect::<Vec<_>>()
    );
    Ok(list!(
        max_discounted_transfers = count(s.max_discounted_transfers),
        transfer_time_allowance = count(s.transfer_time_allowance),
        fare_cap = s.fare_cap,
        fares_per_type = types,
        fares_per_transfer = transfers,
        fares_per_route = routes,
        debug_settings = list!(
            output_file = s.debug_output_file.clone(),
            trip_info = s.debug_trip_info.clone()
        )
    )
    .into())
}

/// `fare_structure` and `max_fare` of a routing call: `NULL` or r5r's
/// list.
pub(crate) fn fare_args(fare_structure: &Robj) -> Result<Option<Arc<FareStructure>>> {
    if fare_structure.is_null() {
        return Ok(None);
    }
    Ok(Some(Arc::new(structure_from_r(fare_structure)?)))
}

/// Check a fare structure as r5r does when it is set, against the
/// network's routes (`NULL` passes).
/// @noRd
#[extendr]
fn rs_check_fare_structure(ptr: Robj, fare_structure: Robj) -> Result<()> {
    let Some(s) = fare_args(&fare_structure)? else {
        return Ok(());
    };
    s.validate().map_err(err)?;
    let n = network(&ptr)?;
    s.validate_for(&n.net).map_err(err)
}

/// r5r's `setup_fare_structure()` as a list (columns of the tables).
/// @noRd
#[extendr]
fn rs_setup_fare_structure(ptr: Robj, base_fare: f64, by: &str) -> Result<Robj> {
    let n = network(&ptr)?;
    let s = setup_fare_structure(&n.net, base_fare, by).map_err(err)?;
    structure_to_r(&s)
}

/// r5r's `read_fare_structure()` of the bytes of the `.zip` file.
/// @noRd
#[extendr]
fn rs_read_fare_structure(bytes: Raw, encoding: &str) -> Result<Robj> {
    let s = read_fare_structure(bytes.as_slice(), encoding).map_err(err)?;
    structure_to_r(&s)
}

/// `data.table::fwrite()` text of a data frame (a list of character,
/// logical, integer or double columns).
/// @noRd
#[extendr]
fn rs_fwrite(df: List) -> Result<String> {
    let mut cols = Vec::with_capacity(df.len());
    for (name, v) in df.iter() {
        let c = if v.is_logical() {
            let l: Logicals = v.try_into().map_err(err)?;
            FwriteColumn::Logical(
                l.iter()
                    .map(|b| (!b.is_na()).then(|| b.is_true()))
                    .collect(),
            )
        } else if let Some(i) = v.as_integer_slice() {
            FwriteColumn::Integer(i.iter().map(|&x| (x != i32::MIN).then_some(x)).collect())
        } else if let Some(r) = v.as_real_slice() {
            FwriteColumn::Number(r.to_vec())
        } else if v.is_string() {
            FwriteColumn::Text(texts(&v, name)?)
        } else {
            return Err(err(format!(
                "column {name} of a fare structure table must be character, logical or numeric"
            )));
        };
        cols.push((name.to_string(), c));
    }
    fwrite_csv(&cols).map_err(err)
}

/// A deflated `.zip` archive of the named texts.
/// @noRd
#[extendr]
fn rs_zip_texts(names: Vec<String>, texts: Vec<String>) -> Raw {
    let files: Vec<(&str, &[u8])> = names
        .iter()
        .zip(&texts)
        .map(|(n, t)| (n.as_str(), t.as_bytes()))
        .collect();
    Raw::from_bytes(&zip_deflated(&files))
}

/// r5r's `pareto_frontier()`: `list(from, to, percentile, travel_time,
/// monetary_cost)`, `from`/`to` 1-based. `transit_modes` empty routes by
/// `mode` alone.
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_pareto_frontier(
    ptr: Robj,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    mode: &str,
    transit_modes: &str,
    egress_mode: &str,
    date: i32,
    departure_time: i32,
    time_window: i32,
    percentiles: &[i32],
    max_walk_time: i32,
    max_bike_time: i32,
    max_car_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_rides: i32,
    has_seed: bool,
    seed: i32,
    n_threads: i32,
    fare_structure: Robj,
    fare_cutoffs: &[f64],
) -> Result<Robj> {
    let n = network(&ptr)?;
    let (o, d) = (points(olat, olon), points(dlat, dlon));
    let fs = fare_args(&fare_structure)?;
    let transit = !transit_modes.is_empty();
    let mut p: TransitParams = super::transit_params(
        if transit { transit_modes } else { "TRANSIT" },
        mode,
        egress_mode,
        if transit { date } else { 20190513 },
        departure_time,
        time_window,
        percentiles,
        max_walk_time,
        max_bike_time,
        max_car_time,
        max_trip_duration,
        walk_speed,
        bike_speed,
        max_lts,
        max_rides,
        1,
        super::seed_of(has_seed, seed),
        n_threads,
    )?;
    p.fare_structure = fs;
    p.n_threads = threads(n_threads);
    if transit {
        super::transit(n)?;
    } else {
        // by `mode` alone
        p.transit_modes.clear();
    }
    let f = match interruptible(&p.cancel, || {
        pareto_frontier(&n.net, &o, &d, &p, fare_cutoffs)
    })? {
        Outcome::Done(v) => v,
        Outcome::Stopped(cond) => return super::stopped(cond),
    };
    let rows = &f.rows;
    Ok(list!(
        from = rows.iter().map(|r| r.origin as i32 + 1).collect::<Vec<_>>(),
        to = rows
            .iter()
            .map(|r| r.destination as i32 + 1)
            .collect::<Vec<_>>(),
        percentile = rows.iter().map(|r| r.percentile).collect::<Vec<_>>(),
        travel_time = rows.iter().map(|r| r.travel_time).collect::<Vec<_>>(),
        monetary_cost = rows.iter().map(|r| r.monetary_cost).collect::<Vec<_>>()
    )
    .into())
}

extendr_module! {
    mod fares;
    fn rs_check_fare_structure;
    fn rs_setup_fare_structure;
    fn rs_read_fare_structure;
    fn rs_fwrite;
    fn rs_zip_texts;
    fn rs_pareto_frontier;
}
