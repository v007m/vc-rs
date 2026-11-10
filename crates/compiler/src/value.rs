//! Lowers a parsed value to a [`TemplateValue`]: text becomes a template, numbers and flags
//! keep their type, a map whose only key is `$cel` becomes a checked CEL value, and a map with
//! an `$extend` key extends that base with its other keys, then drops its `$remove` paths.

use indexmap::IndexMap;
use serde_json::Number;
use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Path, TemplateValue};
use vc_parser::{Entry, Scalar, ScalarKind, Value};

use crate::link::body_path;
use crate::parts;
use crate::parts::{At, Cx};

/// The key of a computed value: `{ $cel: var.price * 2 }`.
const CEL_KEY: &str = "$cel";

/// The key of the value a map extends: `{ $extend: ${original.body}, city: Pune }`.
const EXTEND_KEY: &str = "$extend";

/// The key of the paths an extended map drops: `$remove: [id, shipping.geo]`.
const REMOVE_KEY: &str = "$remove";

/// The value at `at`, or `None` when part of it does not compile (the reasons are recorded).
pub fn lower(cx: &mut Cx<'_>, at: At<'_>) -> Option<TemplateValue> {
    match &at.node.value {
        Value::Scalar(scalar) => scalar_value(cx, at, scalar),
        Value::Seq(items) => {
            let lowered: Vec<_> = items.iter().map(|item| lower(cx, at.child(item))).collect();
            lowered.into_iter().collect::<Option<Vec<_>>>().map(TemplateValue::List)
        }
        Value::Map(entries) => map(cx, at, entries),
    }
}

fn map(cx: &mut Cx<'_>, at: At<'_>, entries: &[Entry]) -> Option<TemplateValue> {
    if let [only] = entries
        && only.key == CEL_KEY
    {
        return parts::cel(cx, at.child(&only.value)).map(TemplateValue::Cel);
    }
    if let Some(base) = entries.iter().find(|entry| entry.key == EXTEND_KEY) {
        return extend(cx, at, (base, entries));
    }
    plain_map(cx, at, entries.iter())
}

/// `$extend`'s base, the other keys as the changes merged into it, and the `$remove` paths.
fn extend(cx: &mut Cx<'_>, at: At<'_>, (base, entries): (&Entry, &[Entry])) -> Option<TemplateValue> {
    let base = parts::template(cx, at.child(&base.value));
    let changes = entries.iter().filter(|entry| entry.key != EXTEND_KEY && entry.key != REMOVE_KEY);
    let changes = plain_map(cx, at, changes);
    let remove = entries
        .iter()
        .find(|entry| entry.key == REMOVE_KEY)
        .map_or_else(|| Some(Vec::new()), |entry| removed(cx, entry));
    Some(TemplateValue::Extend { base: base?, changes: Box::new(changes?), remove: remove? })
}

/// The paths a `$remove` list names; `None`, with a V2302, when it is not a list of texts.
fn removed(cx: &mut Cx<'_>, entry: &Entry) -> Option<Vec<Path>> {
    let paths = match &entry.value.value {
        Value::Seq(items) => items.iter().map(|item| item.as_scalar().map(|scalar| body_path(&scalar.text))).collect(),
        Value::Scalar(_) | Value::Map(_) => None,
    };
    if paths.is_none() {
        let message = format!("expected a list of paths, found {}", entry.value.kind_name());
        cx.push(Diagnostic::new(Code::V2302, entry.value.span, message).label("expected a list of paths"));
    }
    paths
}

fn plain_map<'e>(cx: &mut Cx<'_>, at: At<'_>, entries: impl Iterator<Item = &'e Entry>) -> Option<TemplateValue> {
    let lowered: Vec<_> = entries.map(|entry| (&entry.key, lower(cx, at.child(&entry.value)))).collect();
    let mut values = IndexMap::with_capacity(lowered.len());
    for (key, value) in lowered {
        values.insert(key.clone(), value?);
    }
    Some(TemplateValue::Map(values))
}

fn scalar_value(cx: &mut Cx<'_>, at: At<'_>, scalar: &Scalar) -> Option<TemplateValue> {
    let typed = match scalar.kind {
        ScalarKind::Null => Some(TemplateValue::Null),
        ScalarKind::Bool => Some(TemplateValue::Bool(scalar.text.eq_ignore_ascii_case("true"))),
        ScalarKind::Int => integer(&scalar.text).map(TemplateValue::Number),
        ScalarKind::Float => scalar.text.parse().ok().and_then(Number::from_f64).map(TemplateValue::Number),
        ScalarKind::String => None,
    };
    typed.or_else(|| parts::template(cx, at).map(TemplateValue::String))
}

/// A YAML 1.2 integer: decimal, `0x` hex or `0o` octal; `None` past the 64-bit range.
fn integer(text: &str) -> Option<Number> {
    let radix =
        [("0x", 16), ("0o", 8)].into_iter().find_map(|(prefix, radix)| Some((text.strip_prefix(prefix)?, radix)));
    match radix {
        Some((digits, radix)) => u64::from_str_radix(digits, radix).ok().map(Number::from),
        None => text.parse::<i64>().map(Number::from).ok().or_else(|| text.parse::<u64>().ok().map(Number::from)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Number;

    use super::integer;

    #[test]
    fn reads_yaml_integers() {
        assert_eq!(integer("42"), Some(Number::from(42)));
        assert_eq!(integer("-7"), Some(Number::from(-7)));
        assert_eq!(integer("0x1F"), Some(Number::from(31)));
        assert_eq!(integer("0o17"), Some(Number::from(15)));
        assert_eq!(integer("18446744073709551615"), Some(Number::from(u64::MAX)));
        assert_eq!(integer("99999999999999999999"), None);
    }
}
