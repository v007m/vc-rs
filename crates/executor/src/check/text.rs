//! Text matchers: `contains`, `startsWith`, `endsWith`, `regex`.

use regex::Regex;
use serde_json::Value;
use vc_model::MatcherArg;

use super::{Judge, MatcherRegistry, compare};

/// `registry` with the text matchers.
pub fn register(registry: MatcherRegistry) -> MatcherRegistry {
    registry
        .with("contains", contains)
        .with("startsWith", |judge, actual, arg| affix(judge, actual, (arg, |text, part| text.starts_with(part))))
        .with("endsWith", |judge, actual, arg| affix(judge, actual, (arg, |text, part| text.ends_with(part))))
        .with("regex", regex)
}

/// A string has the text as a substring; a list has the value as an element.
fn contains(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let wanted = judge.value(arg)?;
    Ok(match actual {
        Some(Value::String(text)) => text.contains(&vc_template::to_text(&wanted)),
        Some(Value::Array(items)) => items.iter().any(|item| compare::same(item, &wanted)),
        _ => false,
    })
}

/// Whether a text has a part where a matcher wants it.
type Has = fn(&str, &str) -> bool;

fn affix(judge: &Judge<'_>, actual: Option<&Value>, (arg, has): (&MatcherArg, Has)) -> Result<bool, String> {
    let part = vc_template::to_text(&judge.value(arg)?);
    Ok(actual.and_then(Value::as_str).is_some_and(|text| has(text, &part)))
}

fn regex(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let pattern = Regex::new(&judge.text(arg)?).map_err(|error| error.to_string())?;
    Ok(actual.and_then(Value::as_str).is_some_and(|text| pattern.is_match(text)))
}
