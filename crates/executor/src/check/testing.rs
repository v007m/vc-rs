//! Helpers the matcher tests share: literal arguments and matcher calls.

use serde_json::{Value, json};
use vc_model::{Matcher, MatcherArg, MatcherCall, Pos, SourceFileId, SourceSpan, TemplateValue};
use vc_template::RenderError;

use super::Arguments;

/// Renders the literal texts these checks hold.
pub struct Texts;

impl Arguments for Texts {
    fn render(&self, value: &TemplateValue) -> Result<Value, RenderError> {
        let TemplateValue::String(text) = value else { return Err(RenderError::missing("not text")) };
        let [vc_model::Part::Literal(text)] = text.parts.as_slice() else { return Err(RenderError::missing("x")) };
        Ok(json!(text))
    }
}

/// A span at the start of the first file.
pub fn span() -> SourceSpan {
    SourceSpan::new(SourceFileId::new(0), Pos::new(1, 1, 0), Pos::new(1, 1, 0))
}

/// The matcher `name` called with `arg`.
pub fn call(name: &str, arg: MatcherArg) -> Matcher {
    Matcher::All(vec![MatcherCall { name: name.to_owned(), arg, span: span() }])
}
