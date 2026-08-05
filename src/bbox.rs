//! Rust port of the `_process_bbox` helper from `server.py`.
//!
//! Normalizes a Hyper3D Rodin bounding-box condition to the ratio form the
//! API expects: `[int(float(i) / max(original) * 100) for i in original]`,
//! with integer passthrough and rejection of non-positive values.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Number, Value};

/// Normalized `bbox_condition` (Length/Width/Height ratio, 0..100 each) as
/// returned / sent to Blender.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct BboxCondition {
    /// The normalized Length/Width/Height ratios, each in the range 0..=100.
    pub values: Vec<i64>,
}

impl BboxCondition {
    /// Convert a raw JSON array of numbers into a normalized ratio.
    ///
    /// Ported byte-for-byte from `_process_bbox`:
    ///  - `None`          -> `None`
    ///  - all `int`       -> passed through unchanged
    ///  - any value <= 0  -> `ValueError("Incorrect number range: bbox must be bigger than zero!")`
    ///  - otherwise       -> `[int(float(i) / max * 100) for i in original]`
    pub fn from_json(original: Option<&Value>) -> Result<Option<BboxCondition>, String> {
        let original = match original {
            Some(v) if !v.is_null() => v,
            _ => return Ok(None),
        };

        let arr = original
            .as_array()
            .ok_or_else(|| "bbox_condition must be an array of numbers".to_string())?;

        // Mimic Python's int-vs-float discrimination: a JSON number is "int"
        // when its serialized form has no fraction / exponent. sehe we compare
        // the underlying JSON Number (lossless) rather than an f64.
        let all_ints = arr.iter().all(|v| is_json_int(v));

        if all_ints {
            let values: Result<Vec<i64>, _> = arr
                .iter()
                .map(|v| v.as_i64().ok_or_else(|| "bbox_condition integer out of range".to_string()))
                .collect();
            return values.map(|values| Some(BboxCondition { values }));
        }

        // Floats: reject any non-positive value first.
        if let Some(bad) = arr.iter().find(|v| {
            as_f64(v).is_some_and(|n| n <= 0.0)
        }) {
            let val = as_f64(bad).unwrap_or(0.0);
            return Err(format!(
                "Incorrect number range: bbox must be bigger than zero! (got {val})"
            ));
        }

        let nums: Vec<f64> = arr.iter().map(|v| as_f64(v).unwrap_or(0.0)).collect();
        let max = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let values = nums
            .iter()
            .map(|n| (n / max * 100.0) as i64)
            .collect();
        Ok(Some(BboxCondition { values }))
    }

    /// Convert to a bare JSON array value.
    pub fn to_json(&self) -> Value {
        Value::Array(
            self.values
                .iter()
                .map(|&v| Value::Number(Number::from(v)))
                .collect(),
        )
    }
}

/// A lossy f64 view, used only for the float branch.
fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// True when the JSON value is a number whose shortest representation is an
/// integer (no fraction, no exponent) — the Rust equivalent of Python's
/// `isinstance(i, int)`.
fn is_json_int(v: &Value) -> bool {
    match v {
        Value::Number(n) => {
            n.as_i64().is_some()
                || n.as_u64().is_some()
        }
        _ => false,
    }
}

/// Fallback wrapper so callers that prefer a map can store the normalized list.
pub fn bbox_to_map(cond: Option<&BboxCondition>) -> Option<Value> {
    cond.map(BboxCondition::to_json)
}

/// Convenience: normalize directly from an optional JSON array to an optional
/// bare array, matching the shape used in tool params.
pub fn process_bbox(original: Option<&Value>) -> Result<Option<Value>, String> {
    BboxCondition::from_json(original).map(|c| c.map(|c| c.to_json()))
}

#[allow(dead_code)]
fn _unused_types(_m: &BTreeMap<String, Value>) {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run(original: Value) -> Result<Option<Value>, String> {
        process_bbox(Some(&original))
    }

    #[test]
    fn none_passes_through() {
        assert_eq!(process_bbox(None).unwrap(), None);
        assert_eq!(process_bbox(Some(&Value::Null)).unwrap(), None);
    }

    #[test]
    fn integer_array_passes_through_unchanged() {
        let out = run(json!([1, 2, 3])).unwrap().unwrap();
        assert_eq!(out, json!([1, 2, 3]));
    }

    #[test]
    fn integer_passthrough_keeps_nonpositive_values() {
        // Python's `all(isinstance(i, int))` branch returns the list as-is
        // without the >0 range check, which only applies to the float branch.
        let out = run(json!([0, -5, 10])).unwrap().unwrap();
        assert_eq!(out, json!([0, -5, 10]));
    }

    #[test]
    fn float_array_is_normalized_to_ratios() {
        let out = run(json!([1.0, 2.0, 3.0])).unwrap().unwrap();
        assert_eq!(out, json!([33, 66, 100]));
    }

    #[test]
    fn float_normalization_scales_by_max() {
        let out = run(json!([0.5, 1.0])).unwrap().unwrap();
        assert_eq!(out, json!([50, 100]));
    }

    #[test]
    fn mixed_int_float_uses_float_branch() {
        // 2.0 serializes as a float, so `all(isinstance(i, int))` is false.
        let out = run(json!([1, 2.0])).unwrap().unwrap();
        assert_eq!(out, json!([50, 100]));
    }

    #[test]
    fn nonpositive_float_is_rejected() {
        let err = run(json!([1.0, 0.0])).unwrap_err();
        assert!(
            err.contains("Incorrect number range: bbox must be bigger than zero!"),
            "unexpected error: {err}"
        );
        let err = run(json!([-1.0, 2.0])).unwrap_err();
        assert!(
            err.contains("Incorrect number range: bbox must be bigger than zero!"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn non_array_is_rejected() {
        let err = run(json!({"x": 1})).unwrap_err();
        assert!(err.contains("must be an array"), "unexpected error: {err}");
    }

    #[test]
    fn bbox_condition_roundtrip() {
        let cond = BboxCondition::from_json(Some(&json!([1.0, 2.0, 4.0])))
            .unwrap()
            .unwrap();
        assert_eq!(cond.values, vec![25, 50, 100]);
        assert_eq!(cond.to_json(), json!([25, 50, 100]));
        assert_eq!(bbox_to_map(Some(&cond)), Some(json!([25, 50, 100])));
    }
}