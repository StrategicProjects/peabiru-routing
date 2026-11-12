//! Scenarios (`new_carspeeds`, `carspeed_scale`, `new_lts`) from the
//! dictionary `peabiru._scenario.scenario_spec()` builds: the data frame
//! columns as text, written as R5 receives them (Java parses them
//! here), and the geometries flattened.

use peabiru_core::unstable::{java_parse_float, java_parse_int, java_parse_long};
use peabiru_core::{CarSpeeds, LtsChange, LtsLine, Polygon, Scenario, SpeedPolygon};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

fn item<'py, T>(d: &Bound<'py, PyDict>, key: &str) -> PyResult<T>
where
    T: for<'a> FromPyObject<'a, 'py>,
{
    let v = d
        .get_item(key)?
        .ok_or_else(|| PyTypeError::new_err(format!("scenario['{key}'] is missing")))?;
    v.extract().map_err(Into::into)
}

fn ids(arg: &str, text: &[String]) -> PyResult<Vec<i64>> {
    text.iter()
        .map(|s| {
            java_parse_long(s).ok_or_else(|| {
                PyValueError::new_err(format!(
                    "'{arg}': osm_id '{s}' is not an integer as R writes it"
                ))
            })
        })
        .collect()
}

/// The scenario of a routing call; `None` changes nothing.
pub(crate) fn scenario_of(spec: Option<&Bound<'_, PyDict>>) -> PyResult<Scenario> {
    let mut s = Scenario::default();
    let Some(d) = spec else {
        return Ok(s);
    };
    s.car_speed_scale = item::<f64>(d, "carspeed_scale")? as f32;
    let car: String = item(d, "car")?;
    match car.as_str() {
        "none" => {}
        "scale" | "km/h" => {
            let ids = ids("new_carspeeds", &item::<Vec<String>>(d, "car_ids")?)?;
            let values = item::<Vec<String>>(d, "car_values")?
                .iter()
                .map(|v| {
                    java_parse_float(v).ok_or_else(|| {
                        PyValueError::new_err(format!(
                            "'new_carspeeds': max_speed '{v}' is not a number as Java reads it"
                        ))
                    })
                })
                .collect::<PyResult<Vec<f32>>>()?;
            let pairs = ids.into_iter().zip(values).collect();
            s.car_speeds = if car == "scale" {
                CarSpeeds::ScaleByWay(pairs)
            } else {
                CarSpeeds::KmhByWay(pairs)
            };
        }
        "polygon" => {
            // R5 receives the polygons as GeoJSON written by GDAL
            let gdal = |v: Vec<f64>| -> Vec<f64> {
                v.into_iter()
                    .map(peabiru_core::unstable::gdal_geojson_coordinate)
                    .collect()
            };
            let x: Vec<f64> = gdal(item(d, "poly_x")?);
            let y: Vec<f64> = gdal(item(d, "poly_y")?);
            let ring_len: Vec<usize> = item(d, "ring_len")?;
            let ring_polygon: Vec<usize> = item(d, "ring_polygon")?;
            let polygon_feature: Vec<usize> = item(d, "polygon_feature")?;
            let scale: Vec<f64> = item(d, "scale")?;
            let priority: Vec<f64> = item(d, "priority")?;
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
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown scenario car '{other}'"
            )))
        }
    }
    let lts: String = item(d, "lts")?;
    match lts.as_str() {
        "none" => {}
        "way" => {
            let ids = ids("new_lts", &item::<Vec<String>>(d, "lts_ids")?)?;
            let values = item::<Vec<String>>(d, "lts_values")?
                .iter()
                .map(|v| {
                    java_parse_int(v).ok_or_else(|| {
                        PyValueError::new_err(format!("'new_lts': lts '{v}' is not an integer"))
                    })
                })
                .collect::<PyResult<Vec<i32>>>()?;
            s.lts = LtsChange::ByWay(ids.into_iter().zip(values).collect());
        }
        "line" => {
            let x: Vec<f64> = item(d, "line_x")?;
            let y: Vec<f64> = item(d, "line_y")?;
            let len: Vec<usize> = item(d, "line_len")?;
            let lts: Vec<i32> = item(d, "line_lts")?;
            let mut at = 0;
            let mut lines = Vec::with_capacity(len.len());
            for (&n, &l) in len.iter().zip(&lts) {
                lines.push(LtsLine::new(
                    (at..at + n).map(|i| [x[i], y[i]]).collect(),
                    l,
                ));
                at += n;
            }
            s.lts = LtsChange::ByLine(lines);
        }
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown scenario lts '{other}'"
            )))
        }
    }
    Ok(s)
}
