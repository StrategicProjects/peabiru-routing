//! r5r's scenarios (`new_carspeeds`, `carspeed_scale`, `new_lts`) from the
//! list the R code builds (`scenario_spec()` in `R/scenario.R`): the
//! columns of the data frames as text, as r5r hands them to R5 (Java
//! parses them here), and the geometries of the `sf` frames flattened.

use extendr_api::prelude::*;
use extendr_api::{Error, Result};
use peabiru_core::unstable::{java_parse_float, java_parse_int, java_parse_long};
use peabiru_core::{CarSpeeds, LtsChange, LtsLine, Scenario, SpeedPolygon};

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(msg.to_string())
}

fn strings(spec: &Robj, name: &str) -> Result<Vec<String>> {
    let v: Strings = spec
        .dollar(name)
        .map_err(err)?
        .try_into()
        .map_err(|_| err(format!("scenario${name} must be a character vector")))?;
    Ok(v.iter().map(|s| <str>::to_owned(s)).collect())
}

fn doubles(spec: &Robj, name: &str) -> Result<Vec<f64>> {
    spec.dollar(name)
        .map_err(err)?
        .as_real_vector()
        .ok_or_else(|| err(format!("scenario${name} must be a double vector")))
}

fn integers(spec: &Robj, name: &str) -> Result<Vec<i32>> {
    spec.dollar(name)
        .map_err(err)?
        .as_integer_vector()
        .ok_or_else(|| err(format!("scenario${name} must be an integer vector")))
}

fn string(spec: &Robj, name: &str) -> Result<String> {
    spec.dollar(name)
        .map_err(err)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| err(format!("scenario${name} must be a string")))
}

/// The ids (`Long.parseLong`) of a data frame's `osm_id` column, as r5r's
/// `RMapBuilder` reads them; the error names the argument as r5r's Java
/// exception does.
fn ids(arg: &str, text: &[String]) -> Result<Vec<i64>> {
    text.iter()
        .map(|s| {
            java_parse_long(s).ok_or_else(|| {
                err(format!(
                    "'{arg}': osm_id '{s}' is not an integer as R writes it (r5r fails too: \
                     java.lang.NumberFormatException: For input string: \"{s}\")"
                ))
            })
        })
        .collect()
}

/// The scenario of a routing call; `NULL` changes nothing.
pub(crate) fn scenario_of(spec: &Robj) -> Result<Scenario> {
    let mut s = Scenario::default();
    if spec.is_null() {
        return Ok(s);
    }
    s.car_speed_scale = doubles(spec, "carspeed_scale")?
        .first()
        .copied()
        .ok_or_else(|| err("scenario$carspeed_scale is empty"))? as f32;
    match string(spec, "car")?.as_str() {
        "none" => {}
        kind @ ("scale" | "km/h") => {
            let ids = ids("new_carspeeds", &strings(spec, "car_ids")?)?;
            let values = strings(spec, "car_values")?
                .iter()
                .map(|v| {
                    java_parse_float(v).ok_or_else(|| {
                        err(format!(
                            "'new_carspeeds': max_speed '{v}' is not a number as Java reads it \
                             (r5r fails too: java.lang.NumberFormatException: For input \
                             string: \"{v}\")"
                        ))
                    })
                })
                .collect::<Result<Vec<f32>>>()?;
            let pairs = ids.into_iter().zip(values).collect();
            s.car_speeds = if kind == "scale" {
                CarSpeeds::ScaleByWay(pairs)
            } else {
                CarSpeeds::KmhByWay(pairs)
            };
        }
        "polygon" => {
            let (x, y) = (doubles(spec, "poly_x")?, doubles(spec, "poly_y")?);
            let ring_len = integers(spec, "ring_len")?;
            let ring_polygon = integers(spec, "ring_polygon")?;
            let polygon_feature = integers(spec, "polygon_feature")?;
            let scale = doubles(spec, "scale")?;
            let priority = doubles(spec, "priority")?;
            let mut features: Vec<Vec<peabiru_core::Polygon>> = vec![Vec::new(); scale.len()];
            let mut polys: Vec<peabiru_core::Polygon> = vec![Vec::new(); polygon_feature.len()];
            let mut at = 0usize;
            for (&n, &p) in ring_len.iter().zip(&ring_polygon) {
                let n = n as usize;
                let ring = (at..at + n).map(|i| [x[i], y[i]]).collect();
                polys[p as usize].push(ring);
                at += n;
            }
            for (poly, &f) in polys.into_iter().zip(&polygon_feature) {
                features[f as usize].push(poly);
            }
            s.car_speeds = CarSpeeds::ByPolygon(
                features
                    .into_iter()
                    .zip(scale.iter().zip(&priority))
                    .map(|(p, (&sc, &pr))| SpeedPolygon::new(p, sc, pr))
                    .collect(),
            );
        }
        other => return Err(err(format!("unknown scenario$car '{other}'"))),
    }
    match string(spec, "lts")?.as_str() {
        "none" => {}
        "way" => {
            let ids = ids("new_lts", &strings(spec, "lts_ids")?)?;
            let values = strings(spec, "lts_values")?
                .iter()
                .map(|v| {
                    java_parse_int(v).ok_or_else(|| {
                        err(format!(
                            "'new_lts': lts '{v}' is not an integer (r5r fails too: \
                             java.lang.NumberFormatException: For input string: \"{v}\")"
                        ))
                    })
                })
                .collect::<Result<Vec<i32>>>()?;
            s.lts = LtsChange::ByWay(ids.into_iter().zip(values).collect());
        }
        "line" => {
            let (x, y) = (doubles(spec, "line_x")?, doubles(spec, "line_y")?);
            let len = integers(spec, "line_len")?;
            let lts = integers(spec, "line_lts")?;
            let mut at = 0usize;
            let mut lines = Vec::with_capacity(len.len());
            for (&n, &l) in len.iter().zip(&lts) {
                let n = n as usize;
                lines.push(LtsLine::new(
                    (at..at + n).map(|i| [x[i], y[i]]).collect(),
                    l,
                ));
                at += n;
            }
            s.lts = LtsChange::ByLine(lines);
        }
        other => return Err(err(format!("unknown scenario$lts '{other}'"))),
    }
    Ok(s)
}
