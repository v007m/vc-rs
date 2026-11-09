//! A v1 `expect` (and `repeat-until`) as a v2 `expect`: status, headers, timing, the response
//! schema, and the common shapes of v1's JavaScript asserts as `body.match` (D200). An assert of
//! another shape is a TODO.

use serde_json::{Map, Value, json};

use super::source::Source;
use super::template::Templates;
use super::{Converter, duration};

/// Comparison operators: v1's JavaScript, and the v2 matcher (`None` for equality).
const COMPARISONS: [(&str, Option<&str>); 8] = [
    ("!==", Some("ne")),
    ("===", None),
    ("!=", Some("ne")),
    ("==", None),
    (">=", Some("gte")),
    ("<=", Some("lte")),
    (">", Some("gt")),
    ("<", Some("lt")),
];

/// v1's `timing` key for a wait before the call, which v2 has no form for.
const WAIT: &str = "wait";

/// Timing limits: v1's operator and the v2 matcher, longest operator first.
const LIMITS: [(&str, &str); 4] = [("<=", "lte"), (">=", "gte"), ("<", "lt"), (">", "gt")];

/// `{response.list}.every(x => …)` and `.filter(x => …).length >= 1`: the call, what ends it,
/// and the v2 matcher.
const ARROWS: [(&str, &str, &str); 3] =
    [(".every(", ")", "every"), (".filter(", ").length >= 1", "some"), (".filter(", ").length > 0", "some")];

/// A v1 expect as v2, with the TODOs for what it could not keep.
pub struct Expected {
    /// The v2 expect, `None` when nothing was kept.
    pub expect: Option<Value>,
    /// What has no v2 form, with its v1 text.
    pub todos: Vec<String>,
}

impl Converter<'_> {
    /// The v2 form of the v1 expect `expect`; `at` names it in notes.
    pub(super) fn expected(&mut self, expect: &Value, at: &str) -> Expected {
        let mut out = Map::new();
        let mut todos = Vec::new();
        out.extend(expect.get("status").map(|status| ("status".to_owned(), status.clone())));
        let headers = expect.get("headers").map(|headers| self.templates.value(headers, at));
        out.extend(headers.map(|headers| ("headers".to_owned(), headers)));
        let body = self.response(expect.get("response"), at, &mut todos);
        if !body.is_empty() {
            out.insert("body".to_owned(), Value::Object(body));
        }
        if expect.get("timing").and_then(|timing| timing.get(WAIT)).is_some() {
            self.templates.note(at, "timing.wait has no v2 form; it is dropped".to_owned());
        }
        let timing = timing(expect.get("timing"), &mut todos);
        if !timing.is_empty() {
            out.insert("timing".to_owned(), Value::Object(timing));
        }
        Expected { expect: (!out.is_empty()).then_some(Value::Object(out)), todos }
    }

    /// `body.schema` from a `schema` reference, `body.match` from the asserts of known shapes.
    fn response(&mut self, response: Option<&Value>, at: &str, todos: &mut Vec<String>) -> Map<String, Value> {
        let mut body = Map::new();
        let mut matched = Map::new();
        for (label, assert) in response.and_then(Value::as_object).into_iter().flatten() {
            let script = assert.as_str().unwrap_or_default();
            if label == "schema" {
                match Source::referred(&self.source.schemas, script) {
                    Some(schema) => drop(body.insert("schema".to_owned(), schema.clone())),
                    None => todos.push(format!("expect.response.schema: {script} is not in the schemas folder")),
                }
                continue;
            }
            match assertion(script, &mut self.templates, at).filter(|(key, _)| !matched.contains_key(key)) {
                Some((key, check)) => drop(matched.insert(key, check)),
                None => todos.push(format!("expect.response \"{label}\": {script}")),
            }
        }
        if !matched.is_empty() {
            body.insert("match".to_owned(), Value::Object(matched));
        }
        body
    }
}

/// Timing limits such as `total: "<700"` as `total: {lt: 700ms}`; another shape is a TODO.
/// `wait` (v1's wait before the call) is left out; [`Converter::expected`] notes it.
fn timing(timing: Option<&Value>, todos: &mut Vec<String>) -> Map<String, Value> {
    let mut out = Map::new();
    let limits = timing.and_then(Value::as_object).into_iter().flatten().filter(|(name, _)| *name != WAIT);
    for (name, limit) in limits {
        let text = limit.as_str().unwrap_or_default().trim();
        let found = LIMITS.iter().find_map(|(op, matcher)| {
            let millis = text.strip_prefix(op)?.trim().parse::<u64>().ok()?;
            Some(json!({ *matcher: duration(millis) }))
        });
        match found {
            Some(found) => drop(out.insert(name.clone(), found)),
            None => todos.push(format!("expect.timing.{name}: {limit}")),
        }
    }
    out
}

/// The `body.match` key and matcher for a v1 assert of a known shape.
fn assertion(script: &str, templates: &mut Templates, at: &str) -> Option<(String, Value)> {
    arrow(script, templates, at).or_else(|| comparison(script, templates, at))
}

/// `{response.p} === 'x'`, `{response.p.length} <= {n}`, `'{response.p}' !== '{v}'`, ….
fn comparison(script: &str, templates: &mut Templates, at: &str) -> Option<(String, Value)> {
    let (left, matcher, right) = split(script)?;
    let path = subject(left)?;
    let value = compared(matcher, literal(right, templates, at)?);
    Some(match path.strip_suffix("length").filter(|rest| rest.is_empty() || rest.ends_with('.')) {
        Some(rest) => (key(rest.trim_end_matches('.')), json!({"length": value})),
        None => (key(path), value),
    })
}

/// `{response.list}.every(x => x === v)`, `.every(x => x.length === n)` and
/// `.filter(x => x).length >= 1`.
fn arrow(script: &str, templates: &mut Templates, at: &str) -> Option<(String, Value)> {
    ARROWS.iter().find_map(|(call, end, matcher)| {
        let (left, rest) = script.split_once(call)?;
        let (param, body) = rest.trim_end().strip_suffix(end)?.split_once("=>")?;
        let param = param.trim().trim_start_matches('(').trim_end_matches(')').trim();
        let element = element((param, body.trim()), templates, at)?;
        Some((key(subject(left)?), json!({ *matcher: element })))
    })
}

/// What an arrow function says of each element `param`: itself (true), its value, its length.
fn element((param, body): (&str, &str), templates: &mut Templates, at: &str) -> Option<Value> {
    if body == param {
        return Some(Value::Bool(true));
    }
    let (left, matcher, right) = split(body)?;
    let value = compared(matcher, literal(right, templates, at)?);
    match left.trim().strip_prefix(param)? {
        "" => Some(value),
        ".length" => Some(json!({"length": value})),
        _ => None,
    }
}

/// `left`, the matcher of the operator between, and `right`; equality has no matcher.
type Comparison<'s> = (&'s str, Option<&'static str>, &'s str);

/// `left op right` at the first comparison operator that is in `script`.
fn split(script: &str) -> Option<Comparison<'_>> {
    COMPARISONS.iter().find_map(|(op, matcher)| script.split_once(op).map(|(left, right)| (left, *matcher, right)))
}

/// `value` under its matcher; equality is the value itself.
fn compared(matcher: Option<&str>, value: Value) -> Value {
    match matcher {
        Some(matcher) => json!({ matcher: value }),
        None => value,
    }
}

/// The path after `response` in `{response.p}` or `'{response.p}'`; empty for the whole body.
fn subject(text: &str) -> Option<&str> {
    let text = text.trim().trim_matches(['\'', '"']);
    let path = text.strip_prefix("{response")?.strip_suffix('}')?;
    if path.is_empty() { Some("") } else { path.strip_prefix('.') }
}

/// A JavaScript literal or a quoted v1 text as a v2 value.
fn literal(text: &str, templates: &mut Templates, at: &str) -> Option<Value> {
    let text = text.trim();
    let quoted = ['\'', '"'].iter().find_map(|quote| text.strip_prefix(*quote)?.strip_suffix(*quote));
    if let Some(inner) = quoted.or_else(|| text.starts_with('{').then_some(text)) {
        return Some(Value::String(templates.text(inner, at)));
    }
    serde_json::from_str::<Value>(text).ok().filter(|value| !value.is_object() && !value.is_array())
}

/// A response path as a `body.match` key: `all` as `[*]`, an index as `[n]`, the body as `$`.
pub(super) fn key(path: &str) -> String {
    let key = path.split('.').filter(|word| !word.is_empty()).fold(String::new(), |key, word| {
        if word == "all" {
            key + "[*]"
        } else if word.bytes().all(|byte| byte.is_ascii_digit()) {
            format!("{key}[{word}]")
        } else if key.is_empty() {
            word.to_owned()
        } else {
            format!("{key}.{word}")
        }
    });
    if key.is_empty() { "$".to_owned() } else { key }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{assertion, key};
    use crate::v1::template::Templates;

    fn shape(script: &str) -> Option<(String, serde_json::Value)> {
        assertion(script, &mut Templates::default(), "a")
    }

    #[test]
    fn comparisons_become_matchers() {
        assert_eq!(shape("{response.length} > 0"), Some(("$".to_owned(), json!({"length": {"gt": 0}}))));
        assert_eq!(shape("{response.id.length} === 32"), Some(("id".to_owned(), json!({"length": 32}))));
        assert_eq!(shape("'{response.name}' === '{userName}'"), Some(("name".to_owned(), json!("${var.userName}"))));
        assert_eq!(shape("{response.isAdmin} === false"), Some(("isAdmin".to_owned(), json!(false))));
        assert_eq!(
            shape("'{response.name}' !== '{oldName}'"),
            Some(("name".to_owned(), json!({"ne": "${var.oldName}"})))
        );
        let limit = shape("{response.items.length} <= {pageSize}");
        assert_eq!(limit, Some(("items".to_owned(), json!({"length": {"lte": "${var.pageSize}"}}))));
    }

    #[test]
    fn every_and_filter_become_every_and_some() {
        let every = shape("{response.all.id}.every(id => id.length === 32)");
        assert_eq!(every, Some(("[*].id".to_owned(), json!({"every": {"length": 32}}))));
        let some = shape("{response.all.isAdmin}.filter(isAdmin => isAdmin).length >= 1");
        assert_eq!(some, Some(("[*].isAdmin".to_owned(), json!({"some": true}))));
        let category = shape("{response.items.all.category}.every(c => c === '{categoryCode}')");
        assert_eq!(category, Some(("items[*].category".to_owned(), json!({"every": "${var.categoryCode}"}))));
    }

    #[test]
    fn other_scripts_have_no_shape() {
        assert_eq!(shape("JSON.parse(x).ok"), None);
        assert_eq!(shape("{response.a} === Date.now()"), None);
        assert_eq!(shape("{response.all.a}.map(a => a * 2).length > 1"), None);
        assert_eq!(key("items.0.id"), "items[0].id");
    }
}
