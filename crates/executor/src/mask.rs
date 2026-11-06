//! What the redactor masks with: the secret values it has been told and header and field
//! names (the built-in masking, synchronous), then the user's `redact.rules` (D171), each an
//! async [`MaskRule`] made through the [`MaskRules`] registry (D185).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use aho_corasick::{AhoCorasick, MatchKind};
use async_trait::async_trait;
use regex::{NoExpand, Regex};
use serde_json::{Value, json};
use vc_model::{CelProgram, Config, Namespace, RedactRule};
use vc_template::{Activation, CelCompiler, Expr};

use crate::MASKED;

/// Values shorter than this are not masked inside text: masking `ab` everywhere would hide
/// more than it protects.
const SHORTEST: usize = 4;

/// A user's way to find what must not be shown.
///
/// It runs after the built-in masking, so it never sees a secret value, and gets each piece
/// of output whole, so a rule that calls out (a hook) does so once per piece.
#[async_trait]
pub trait MaskRule: Send + Sync {
    /// `text` with what the rule matches masked.
    async fn text(&self, text: String) -> String;

    /// `value` with what the rule matches masked: fields whole, or inside its strings.
    async fn value(&self, value: Value) -> Value;
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

    fn masked(&self, text: &str) -> String {
        let Some(regex) = &self.0 else { return MASKED.to_owned() };
        regex.replace_all(text, NoExpand(MASKED)).into_owned()
    }
}

#[async_trait]
impl MaskRule for RegexRule {
    async fn text(&self, text: String) -> String {
        self.masked(&text)
    }

    async fn value(&self, value: Value) -> Value {
        walk(value, &|_, _| false, &|text| self.masked(text))
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

    fn hides(&self, name: &str, value: &Value) -> bool {
        let Some(expr) = &self.0 else { return true };
        let activation = Activation::new().with(Namespace::Item, json!({ "name": name, "value": value }));
        !matches!(expr.eval(&activation), Ok(Value::Bool(false)))
    }
}

#[async_trait]
impl MaskRule for CelRule {
    async fn text(&self, text: String) -> String {
        text
    }

    async fn value(&self, value: Value) -> Value {
        walk(value, &|name, value| self.hides(name, value), &str::to_owned)
    }
}

/// The rule for an entry no maker makes: it masks everything (fail closed).
struct MaskAll;

#[async_trait]
impl MaskRule for MaskAll {
    async fn text(&self, _: String) -> String {
        MASKED.to_owned()
    }

    async fn value(&self, _: Value) -> Value {
        json!(MASKED)
    }
}

/// Whether a JSON field (its name and value) is masked whole.
type Hides<'h> = dyn Fn(&str, &Value) -> bool + 'h;

/// `value` with each object field `hides` holds for masked whole, and every other string
/// passed through `text`.
fn walk(value: Value, hides: &Hides<'_>, text: &dyn Fn(&str) -> String) -> Value {
    match value {
        Value::String(shown) => Value::String(text(&shown)),
        Value::Array(items) => Value::Array(items.into_iter().map(|item| walk(item, hides, text)).collect()),
        Value::Object(fields) => {
            let field = |(name, value): (String, Value)| {
                let value = if hides(&name, &value) { json!(MASKED) } else { walk(value, hides, text) };
                (name, value)
            };
            Value::Object(fields.into_iter().map(field).collect())
        }
        other => other,
    }
}

/// The kind of a `redact.rules` entry, as [`MaskRules`] keys its makers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum RedactRuleKind {
    /// `regex:`.
    Regex,
    /// `cel:`.
    Cel,
    /// `function:`, a hook (M4, [`MaskRules::with_hooks`]).
    Function,
}

impl RedactRuleKind {
    /// The kind of `rule`.
    #[must_use]
    pub const fn of(rule: &RedactRule) -> Self {
        match rule {
            RedactRule::Regex(_) => Self::Regex,
            RedactRule::Cel(_) => Self::Cel,
            RedactRule::Function(_) => Self::Function,
        }
    }
}

/// Makes the [`MaskRule`] a `redact.rules` entry describes; `None` for an entry of another
/// kind.
pub type MaskRuleMaker = Arc<dyn Fn(&RedactRule) -> Option<Arc<dyn MaskRule>> + Send + Sync>;

/// The mask rule makers by kind (a registry filled at start-up). `regex` and `cel` are built
/// in; [`MaskRules::with_hooks`] adds the `function:` maker.
#[derive(Clone, Default)]
pub struct MaskRules {
    by_kind: BTreeMap<RedactRuleKind, MaskRuleMaker>,
}

impl fmt::Debug for MaskRules {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.by_kind.keys()).finish()
    }
}

impl MaskRules {
    /// The `regex` and `cel` makers.
    #[must_use]
    pub fn standard() -> Self {
        let regex: MaskRuleMaker = Arc::new(|rule| match rule {
            RedactRule::Regex(pattern) => Some(Arc::new(RegexRule::new(pattern))),
            RedactRule::Cel(_) | RedactRule::Function(_) => None,
        });
        let cel: MaskRuleMaker = Arc::new(|rule| match rule {
            RedactRule::Cel(program) => Some(Arc::new(CelRule::new(program))),
            RedactRule::Regex(_) | RedactRule::Function(_) => None,
        });
        Self::default().with(RedactRuleKind::Regex, regex).with(RedactRuleKind::Cel, cel)
    }

    /// The registry with `maker` making the rules of `kind`.
    #[must_use]
    pub fn with(mut self, kind: RedactRuleKind, maker: MaskRuleMaker) -> Self {
        self.by_kind.insert(kind, maker);
        self
    }

    /// The rule `rule` describes; one that no maker makes masks everything.
    #[must_use]
    pub fn rule(&self, rule: &RedactRule) -> Arc<dyn MaskRule> {
        let made = self.by_kind.get(&RedactRuleKind::of(rule)).and_then(|maker| maker(rule));
        made.unwrap_or_else(|| Arc::new(MaskAll))
    }

    /// The rules of the config's `redact.rules`, in order.
    #[must_use]
    pub fn for_config(&self, config: &Config) -> Vec<Arc<dyn MaskRule>> {
        config.redact_rules.iter().map(|rule| self.rule(rule)).collect()
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

    /// `value` with every hidden field masked whole and every hidden value masked inside its
    /// strings, and no rule applied.
    #[must_use]
    pub fn value(&self, value: Value) -> Value {
        walk(value, &|name, _| self.fields.contains(&name.to_ascii_lowercase()), &|text| self.without_values(text))
    }
}

/// The masks and rules in force for one piece of output.
pub struct Masking<'m> {
    /// The names and values to hide.
    pub masks: &'m Masks,
    /// The rules, in order, applied after the masks.
    pub rules: &'m [Arc<dyn MaskRule>],
}

impl Masking<'_> {
    /// `text` with every secret value masked, then every rule applied.
    pub async fn text(&self, text: &str) -> String {
        let mut text = self.masks.without_values(text);
        for rule in self.rules {
            text = rule.text(text).await;
        }
        text
    }

    /// `value` with every secret value and hidden field masked, then every rule applied.
    pub async fn value(&self, value: Value) -> Value {
        let mut value = self.masks.value(value);
        for rule in self.rules {
            value = rule.value(value).await;
        }
        value
    }
}

#[cfg(test)]
#[path = "mask_tests.rs"]
mod tests;
