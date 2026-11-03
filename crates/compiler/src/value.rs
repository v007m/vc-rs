//! Lowers a parsed value to a [`TemplateValue`]: text becomes a template, numbers and flags
//! keep their type, and a map whose only key is `$cel` becomes a checked CEL value.

use indexmap::IndexMap;
use serde_json::Number;
use vc_model::TemplateValue;
use vc_parser::{Entry, Scalar, ScalarKind, Value};

use crate::parts::{At, Cx, cel, template};

/// The key of a computed value: `{ $cel: var.price * 2 }`.
const CEL_KEY: &str = "$cel";

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
        return cel(cx, at.child(&only.value)).map(TemplateValue::Cel);
    }
    let lowered: Vec<_> = entries.iter().map(|entry| (&entry.key, lower(cx, at.child(&entry.value)))).collect();
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
    typed.or_else(|| template(cx, at).map(TemplateValue::String))
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
