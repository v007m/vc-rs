//! Helpers the crate's tests share: thresholds written as literals, and their judge.

use std::time::Duration as StdDuration;

use indexmap::IndexMap;
use serde_json::{Number, Value};
use vc_executor::Arguments;
use vc_model::{Duration, Matcher, MatcherArg, MatcherCall, Pos, SourceFileId, SourceSpan, TemplateValue};
use vc_template::RenderError;

/// Thresholds hold only literal arguments here.
pub struct Literals;

impl Arguments for Literals {
    fn render(&self, value: &TemplateValue) -> Result<Value, RenderError> {
        match value {
            TemplateValue::Number(number) => Ok(Value::Number(number.clone())),
            _ => Err(RenderError::invalid("not a number")),
        }
    }
}

pub fn call(name: &str, arg: MatcherArg) -> Matcher {
    let span = SourceSpan::new(SourceFileId::new(0), Pos::new(1, 1, 0), Pos::new(1, 1, 0));
    Matcher::All(vec![MatcherCall { name: name.to_owned(), arg, span }])
}

pub fn under_ms(millis: u64) -> Matcher {
    call("lt", MatcherArg::Duration(Duration::new(StdDuration::from_millis(millis))))
}

pub fn number(name: &str, value: f64) -> Matcher {
    call(name, MatcherArg::Value(TemplateValue::Number(Number::from_f64(value).unwrap())))
}

pub fn thresholds(entries: &[(&str, Matcher)]) -> IndexMap<String, Matcher> {
    entries.iter().map(|(metric, matcher)| ((*metric).to_owned(), matcher.clone())).collect()
}
