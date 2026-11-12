//! Scenarios (`newCarspeeds`, `carspeedScale`, `newLts`) from the
//! object `scenarioSpec()` of `lib/index.js` builds: the columns of the
//! tables as text, parsed here as Java parses them in R5,
//! and the geometries flattened.

use js_sys::{Array, Float64Array, Int32Array, Reflect};
use peabiru_core::unstable::{java_parse_float, java_parse_int, java_parse_long};
use peabiru_core::{CarSpeeds, LtsChange, LtsLine, Polygon, Scenario, SpeedPolygon};
use wasm_bindgen::{JsCast, JsError, JsValue};

fn err(msg: impl std::fmt::Display) -> JsError {
    JsError::new(&msg.to_string())
}

fn get(spec: &JsValue, key: &str) -> Result<JsValue, JsError> {
    Reflect::get(spec, &JsValue::from_str(key)).map_err(|_| err(format!("scenario.{key}")))
}

fn string(spec: &JsValue, key: &str) -> Result<String, JsError> {
    get(spec, key)?
        .as_string()
        .ok_or_else(|| err(format!("scenario.{key} must be a string")))
}

fn strings(spec: &JsValue, key: &str) -> Result<Vec<String>, JsError> {
    let a: Array = get(spec, key)?
        .dyn_into()
        .map_err(|_| err(format!("scenario.{key} must be an array")))?;
    a.iter()
        .map(|v| {
            v.as_string()
                .ok_or_else(|| err(format!("scenario.{key} must hold strings")))
        })
        .collect()
}

fn doubles(spec: &JsValue, key: &str) -> Result<Vec<f64>, JsError> {
    let a: Float64Array = get(spec, key)?
        .dyn_into()
        .map_err(|_| err(format!("scenario.{key} must be a Float64Array")))?;
    Ok(a.to_vec())
}

fn sizes(spec: &JsValue, key: &str) -> Result<Vec<usize>, JsError> {
    let a: Int32Array = get(spec, key)?
        .dyn_into()
        .map_err(|_| err(format!("scenario.{key} must be an Int32Array")))?;
    Ok(a.to_vec().into_iter().map(|v| v.max(0) as usize).collect())
}

fn ids(arg: &str, text: &[String]) -> Result<Vec<i64>, JsError> {
    text.iter()
        .map(|s| {
            java_parse_long(s)
                .ok_or_else(|| err(format!("'{arg}': osm_id '{s}' is not an integer")))
        })
        .collect()
}

/// The scenario of a routing call; `null` or `undefined` changes nothing.
pub(crate) fn scenario_of(spec: &JsValue) -> Result<Scenario, JsError> {
    let mut s = Scenario::default();
    if spec.is_null() || spec.is_undefined() {
        return Ok(s);
    }
    s.car_speed_scale = get(spec, "carspeed_scale")?
        .as_f64()
        .ok_or_else(|| err("scenario.carspeed_scale must be a number"))?
        as f32;
    let car = string(spec, "car")?;
    match car.as_str() {
        "none" => {}
        "scale" | "km/h" => {
            let ids = ids("newCarspeeds", &strings(spec, "car_ids")?)?;
            let values = strings(spec, "car_values")?
                .iter()
                .map(|v| {
                    java_parse_float(v).ok_or_else(|| {
                        err(format!(
                            "'newCarspeeds': max_speed '{v}' is not a number as Java reads it"
                        ))
                    })
                })
                .collect::<Result<Vec<f32>, JsError>>()?;
            let pairs = ids.into_iter().zip(values).collect();
            s.car_speeds = if car == "scale" {
                CarSpeeds::ScaleByWay(pairs)
            } else {
                CarSpeeds::KmhByWay(pairs)
            };
        }
        "polygon" => {
            // R5 reads the polygons from GeoJSON as GDAL writes it
            let gdal = |v: Vec<f64>| -> Vec<f64> {
                v.into_iter()
                    .map(peabiru_core::unstable::gdal_geojson_coordinate)
                    .collect()
            };
            let (x, y) = (
                gdal(doubles(spec, "poly_x")?),
                gdal(doubles(spec, "poly_y")?),
            );
            let ring_len = sizes(spec, "ring_len")?;
            let ring_polygon = sizes(spec, "ring_polygon")?;
            let polygon_feature = sizes(spec, "polygon_feature")?;
            let scale = doubles(spec, "scale")?;
            let priority = doubles(spec, "priority")?;
            let mut polys: Vec<Polygon> = vec![Vec::new(); polygon_feature.len()];
            let mut at = 0;
            for (&n, &p) in ring_len.iter().zip(&ring_polygon) {
                polys[p].push((at..at + n).map(|i| [x[i], y[i]]).collect());
                at += n;
            }
            let mut features: Vec<Vec<Polygon>> = vec![Vec::new(); scale.len()];
            for (poly, &f) in polys.into_iter().zip(&polygon_feature) {
                features[f].push(poly);
            }
            s.car_speeds = CarSpeeds::ByPolygon(
                features
                    .into_iter()
                    .zip(scale.iter().zip(&priority))
                    .map(|(p, (&sc, &pr))| SpeedPolygon::new(p, sc, pr))
                    .collect(),
            );
        }
        other => return Err(err(format!("unknown scenario.car '{other}'"))),
    }
    match string(spec, "lts")?.as_str() {
        "none" => {}
        "way" => {
            let ids = ids("newLts", &strings(spec, "lts_ids")?)?;
            let values = strings(spec, "lts_values")?
                .iter()
                .map(|v| {
                    java_parse_int(v)
                        .ok_or_else(|| err(format!("'newLts': lts '{v}' is not an integer")))
                })
                .collect::<Result<Vec<i32>, JsError>>()?;
            s.lts = LtsChange::ByWay(ids.into_iter().zip(values).collect());
        }
        "line" => {
            let (x, y) = (doubles(spec, "line_x")?, doubles(spec, "line_y")?);
            let len = sizes(spec, "line_len")?;
            let lts: Int32Array = get(spec, "line_lts")?
                .dyn_into()
                .map_err(|_| err("scenario.line_lts must be an Int32Array"))?;
            let mut at = 0;
            let mut lines = Vec::with_capacity(len.len());
            for (&n, l) in len.iter().zip(lts.to_vec()) {
                lines.push(LtsLine::new(
                    (at..at + n).map(|i| [x[i], y[i]]).collect(),
                    l,
                ));
                at += n;
            }
            s.lts = LtsChange::ByLine(lines);
        }
        other => return Err(err(format!("unknown scenario.lts '{other}'"))),
    }
    Ok(s)
}
