//! Small shared helpers used by the tool implementations.

use serde_json::Value;

/// Return the string at `key` if it is a JSON string, otherwise "".
pub fn get_str(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Return the integer at `key` if it is a JSON number, otherwise 0.
pub fn get_i64(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0)
}

/// Return the boolean at `key`, else a default.
pub fn get_bool(v: &Value, key: &str, default: bool) -> bool {
    v.get(key)
        .and_then(|x| x.as_bool())
        .unwrap_or(default)
}

/// Python-style truthiness for a JSON value, matching `if value:` semantics.
pub fn is_truthy(v: Option<&Value>) -> bool {
    match v {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Null) => false,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// Pretty-print a JSON value the way Python's `json.dumps(value, indent=2)`
/// does. Falls back to a compact serialization on failure.
pub fn to_json_pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

/// Join an array of JSON strings with ", ", treating non-string / absent
/// entries as empty strings, mirroring Python `", ".join(list)`.
pub fn join_strings_array(v: &Value, key: &str) -> String {
    let items = v.get(key).and_then(|x| x.as_array());
    match items {
        Some(arr) => arr
            .iter()
            .map(|x| x.as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        None => String::new(),
    }
}