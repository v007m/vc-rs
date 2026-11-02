//! Small pure helpers: JSONPath subset, templating, canonical JSON.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

static SEG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.([A-Za-z_][\w-]*)|\[(\d+)\]").unwrap());
static TPL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{\s*([\w.]+)\s*\}\}").unwrap());
static WHOLE_TPL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\{\{\s*([\w.]+)\s*\}\}$").unwrap());

/// Evaluates the JSONPath subset used by fixtures: $.a.b[0].c
pub fn json_path<'v>(value: &'v Value, path: &str) -> Option<&'v Value> {
    let mut cur = value;
    for caps in SEG_RE.captures_iter(&path[1..]) {
        cur = match (caps.get(1), caps.get(2)) {
            (Some(key), _) => cur.get(key.as_str())?,
            (None, Some(idx)) => cur.get(idx.as_str().parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

pub type Vars = Map<String, Value>;

fn lookup<'v>(vars: &'v Vars, dotted: &str) -> Option<&'v Value> {
    let mut parts = dotted.split('.');
    let mut cur = vars.get(parts.next()?)?;
    for p in parts {
        cur = cur.get(p)?;
    }
    Some(cur)
}

fn to_text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// Renders `{{ placeholders }}`; a string that is exactly one placeholder keeps the value's type.
pub fn render_value(tpl: &Value, vars: &Vars) -> Value {
    match tpl {
        Value::String(s) => {
            if let Some(c) = WHOLE_TPL_RE.captures(s) {
                return lookup(vars, &c[1]).cloned().unwrap_or(Value::Null);
            }
            if !s.contains("{{") {
                return tpl.clone();
            }
            Value::String(TPL_RE.replace_all(s, |c: &regex::Captures| to_text(lookup(vars, &c[1]))).into_owned())
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| render_value(v, vars)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), render_value(v, vars))).collect()),
        other => other.clone(),
    }
}

pub fn render_map(m: &BTreeMap<String, Value>, vars: &Vars) -> Vars {
    m.iter().map(|(k, v)| (k.clone(), render_value(v, vars))).collect()
}

pub fn render_path(tpl: &str, vars: &Vars) -> String {
    if !tpl.contains("{{") {
        return tpl.to_owned();
    }
    TPL_RE.replace_all(tpl, |c: &regex::Captures| percent_encode(&to_text(lookup(vars, &c[1])))).into_owned()
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Canonical JSON: serde_json without `preserve_order` sorts object keys; numbers
/// are normalized to f64 so YAML `1` equals JSON `1.0`.
pub fn canonical(v: &Value) -> String {
    fn norm(v: &Value) -> Value {
        match v {
            Value::Number(n) => n.as_f64().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(Value::Null),
            Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
            Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), norm(v))).collect()),
            other => other.clone(),
        }
    }
    norm(v).to_string()
}
