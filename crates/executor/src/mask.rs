//! What the redactor masks with: the secret values it has been told, header and field names,
//! and the user's `redact.rules` (D171), regex or CEL, plus any rule a hook adds.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use aho_corasick::{AhoCorasick, MatchKind};
use regex::{NoExpand, Regex};
use serde_json::{Value, json};
use vc_model::{CelProgram, Namespace, RedactRule};
use vc_template::{Activation, CelCompiler, Expr};

use crate::MASKED;

/// Values shorter than this are not masked inside text: masking `ab` everywhere would hide
/// more than it protects.
const SHORTEST: usize = 4;

/// One way to find what must not be shown, beyond the secret values themselves. The
/// `redact.rules` of the config are rules, and a hook can add its own.
pub trait MaskRule: Send + Sync {
    /// `text` with what the rule matches masked, or `None` when it matches nothing.
    fn text(&self, text: &str) -> Option<String>;

    /// Whether the JSON field `name` holding `value` is masked whole.
    fn field(&self, name: &str, value: &Value) -> bool;
}

/// A `regex:` rule: every match in any text is masked. A pattern that does not compile
/// masks every text, so a typo never shows what it was meant to hide.
#[derive(Debug)]
pub struct RegexRule(Option<Regex>);

impl RegexRule {
    /// The rule for `pattern`.
    #[must_use]
    pub fn new(pattern: &str) -> Self {
        Self(Regex::new(pattern).ok())
    }
}

impl MaskRule for RegexRule {
    fn text(&self, text: &str) -> Option<String> {
        let Some(regex) = &self.0 else { return Some(MASKED.to_owned()) };
        regex.is_match(text).then(|| regex.replace_all(text, NoExpand(MASKED)).into_owned())
    }

    fn field(&self, _: &str, _: &Value) -> bool {
        false
    }
}

/// A `cel:` rule: a JSON field is masked when the expression is true of
/// `item = {name, value}`. An expression that fails masks the field.
#[derive(Debug)]
pub struct CelRule(Option<Arc<dyn Expr>>);

impl CelRule {
    /// The rule for `program`.
    #[must_use]
    pub fn new(program: &CelProgram) -> Self {
        Self(CelCompiler::new().prepare(program).ok())
    }
}

impl MaskRule for CelRule {
    fn text(&self, _: &str) -> Option<String> {
        None
    }

    fn field(&self, name: &str, value: &Value) -> bool {
        let Some(expr) = &self.0 else { return true };
        let activation = Activation::new().with(Namespace::Item, json!({ "name": name, "value": value }));
        !matches!(expr.eval(&activation), Ok(Value::Bool(false)))
    }
}

/// The rule a `redact.rules` entry describes. A `function` rule masks every text until the
/// redactor can await a hook (D185).
pub fn rule(rule: &RedactRule) -> Arc<dyn MaskRule> {
    match rule {
        RedactRule::Regex(pattern) => Arc::new(RegexRule::new(pattern)),
        RedactRule::Cel(program) => Arc::new(CelRule::new(program)),
        RedactRule::Function(_) => Arc::new(RegexRule(None)),
    }
}

/// The names and values the redactor has been told to hide so far. Replaced whole when one
/// is added, so readers never wait.
#[derive(Clone, Default)]
pub struct Masks {
    headers: BTreeSet<String>,
    fields: BTreeSet<String>,
    values: BTreeSet<String>,
    finder: Option<AhoCorasick>,
}

impl fmt::Debug for Masks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Masks")
            .field("headers", &self.headers)
            .field("fields", &self.fields)
            .field("values", &self.values.len())
            .finish_non_exhaustive()
    }
}

impl Masks {
    /// These masks and the header names `added`, in lower case.
    #[must_use]
    pub fn with_headers(&self, added: &[String]) -> Self {
        let mut masks = self.clone();
        masks.headers.extend(added.iter().map(|name| name.to_ascii_lowercase()));
        masks
    }

    /// These masks and the JSON field names `added`, in lower case.
    #[must_use]
    pub fn with_fields(&self, added: &[String]) -> Self {
        let mut masks = self.clone();
        masks.fields.extend(added.iter().map(|name| name.to_ascii_lowercase()));
        masks
    }

    /// These masks and the values `added` that are long enough to find in text.
    #[must_use]
    pub fn with_values(&self, added: &[String]) -> Self {
        let mut masks = self.clone();
        masks.values.extend(added.iter().filter(|value| value.chars().count() >= SHORTEST).cloned());
        let finder = AhoCorasick::builder().match_kind(MatchKind::LeftmostLongest).build(&masks.values);
        masks.finder = finder.ok();
        masks
    }

    /// Whether the header `name` (lower case) is masked.
    #[must_use]
    pub fn hides_header(&self, name: &str) -> bool {
        self.headers.contains(name)
    }

    /// Whether one of the hidden values is among `bytes`.
    #[must_use]
    pub fn finds_value(&self, bytes: &[u8]) -> bool {
        self.finder.as_ref().is_some_and(|finder| finder.is_match(bytes))
    }

    /// `text` with every hidden value masked, and no rule applied.
    #[must_use]
    pub fn without_values(&self, text: &str) -> String {
        let Some(finder) = &self.finder else { return text.to_owned() };
        let mut masked = String::with_capacity(text.len());
        finder.replace_all_with(text, &mut masked, |_, _, masked| {
            masked.push_str(MASKED);
            true
        });
        masked
    }
}

/// The masks and rules in force for one piece of output.
pub struct Masking<'m> {
    /// The names and values to hide.
    pub masks: &'m Masks,
    /// The rules, in order, applied after the values.
    pub rules: &'m [Arc<dyn MaskRule>],
}

impl Masking<'_> {
    /// `text` with every secret value masked, then every rule applied.
    #[must_use]
    pub fn text(&self, text: &str) -> String {
        let mut text = self.masks.without_values(text);
        for rule in self.rules {
            if let Some(masked) = rule.text(&text) {
                text = masked;
            }
        }
        text
    }

    /// `value` with every string masked as text and every hidden field masked whole.
    #[must_use]
    pub fn value(&self, value: Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.text(&text)),
            Value::Array(items) => Value::Array(items.into_iter().map(|item| self.value(item)).collect()),
            Value::Object(fields) => {
                let field = |(name, value): (String, Value)| {
                    let value = if self.hides(&name, &value) { json!(MASKED) } else { self.value(value) };
                    (name, value)
                };
                Value::Object(fields.into_iter().map(field).collect())
            }
            other => other,
        }
    }

    fn hides(&self, name: &str, value: &Value) -> bool {
        self.masks.fields.contains(&name.to_ascii_lowercase()) || self.rules.iter().any(|rule| rule.field(name, value))
    }
}

#[cfg(test)]
#[path = "mask_tests.rs"]
mod tests;
