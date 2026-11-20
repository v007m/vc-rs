//! The matcher registry and the judge that applies matchers (syntax reference "Checks and
//! matchers").

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde_json::{Map, Value, json};
use vc_model::{Matcher, MatcherArg, MatcherCall, StatusPattern, TemplateValue};
use vc_template::RenderError;

use super::{compare, compose, errors, messages, shape, text};
use crate::Render;

/// Renders the templates in a check's expected values: the call's [`Render`] in a run.
pub trait Arguments {
    /// `value`, rendered.
    ///
    /// # Errors
    ///
    /// A [`RenderError`] when a reference in it cannot be rendered.
    fn render(&self, value: &TemplateValue) -> Result<Value, RenderError>;

    /// What `body` breaks the JSON Schema named `key` with (D201); none when it matches.
    ///
    /// # Errors
    ///
    /// A message when no schema was loaded for `key` or it is not a valid JSON Schema.
    fn schema(&self, key: &str, _body: &Value) -> Result<Vec<String>, String> {
        Err(format!("no schema is loaded for `{key}`"))
    }
}

impl Arguments for Render<'_> {
    fn render(&self, value: &TemplateValue) -> Result<Value, RenderError> {
        self.value(value)
    }

    fn schema(&self, key: &str, body: &Value) -> Result<Vec<String>, String> {
        self.schemas().check(key, body)
    }
}

/// One matcher: whether `actual` (`None` when the path is absent) passes with `arg`.
///
/// It fails with a message when `arg` cannot be used, such as a template that does not render.
pub type MatcherFn = fn(&Judge<'_>, Option<&Value>, &MatcherArg) -> Result<bool, String>;

/// A matcher that says, when it fails, what to report as the actual value.
pub type ReportingFn = fn(&Judge<'_>, Option<&Value>, &MatcherArg) -> Result<Judged, String>;

/// Whether a matcher held and, when it did not, what it reports as the actual value (`None`
/// to report the value it was given).
#[derive(Debug, PartialEq, Eq)]
pub struct Judged {
    pub holds: bool,
    pub detail: Option<Value>,
}

impl Judged {
    /// A failure that reports `detail` as the actual value.
    pub const fn failed(detail: Value) -> Self {
        Self { holds: false, detail: Some(detail) }
    }
}

impl From<bool> for Judged {
    fn from(holds: bool) -> Self {
        Self { holds, detail: None }
    }
}

/// A registered matcher: plain, or one that reports its own actual value.
#[derive(Clone, Copy)]
enum Entry {
    Plain(MatcherFn),
    Reporting(ReportingFn),
}

/// The matchers by name (a registry filled at start-up).
#[derive(Clone, Default)]
pub struct MatcherRegistry {
    by_name: HashMap<&'static str, Entry>,
    /// The matchers a written form lowers to, which a report shows as their argument.
    internal: HashSet<&'static str>,
}

impl fmt::Debug for MatcherRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut names: Vec<_> = self.by_name.keys().collect();
        names.sort_unstable();
        f.debug_set().entries(names).finish()
    }
}

impl MatcherRegistry {
    /// Every matcher the syntax reference lists.
    #[must_use]
    pub fn standard() -> Self {
        let registry = Self::default();
        let registry = compose::register(text::register(shape::register(compare::register(registry))));
        messages::register(errors::register(registry))
    }

    /// The registry with `matcher` under `name`.
    #[must_use]
    pub fn with(mut self, name: &'static str, matcher: MatcherFn) -> Self {
        self.by_name.insert(name, Entry::Plain(matcher));
        self
    }

    /// The registry with `matcher` under `name`, as a matcher users cannot write: the compiler
    /// lowers a written form to it, and a report shows its argument, as written, not its name.
    #[must_use]
    pub fn internal(mut self, name: &'static str, matcher: MatcherFn) -> Self {
        self.internal.insert(name);
        self.with(name, matcher)
    }

    /// The registry with `matcher` under `name`, internal as [`Self::internal`], reporting its
    /// own actual value when it fails.
    #[must_use]
    pub(crate) fn internal_reporting(mut self, name: &'static str, matcher: ReportingFn) -> Self {
        self.internal.insert(name);
        self.by_name.insert(name, Entry::Reporting(matcher));
        self
    }

    fn get(&self, name: &str) -> Option<Entry> {
        self.by_name.get(name).copied()
    }
}

/// Applies matchers: finds each by name and renders its argument.
#[derive(Clone, Copy)]
pub struct Judge<'j> {
    registry: &'j MatcherRegistry,
    args: &'j dyn Arguments,
}

impl<'j> Judge<'j> {
    /// A judge using `registry`'s matchers, rendering arguments with `args`.
    #[must_use]
    pub const fn new(registry: &'j MatcherRegistry, args: &'j dyn Arguments) -> Self {
        Self { registry, args }
    }

    /// Whether `actual` passes `matcher`; several matcher calls must all pass.
    ///
    /// # Errors
    ///
    /// A message when a matcher is unknown or its argument cannot be used.
    pub fn matches(&self, matcher: &Matcher, actual: Option<&Value>) -> Result<bool, String> {
        match matcher {
            Matcher::Equals(value) => {
                let expected = self.render(value)?;
                Ok(actual.is_some_and(|actual| compare::same(actual, &expected)))
            }
            Matcher::All(_) => self.judged(matcher, actual).map(|judged| judged.holds),
            Matcher::Status(patterns) => Ok(status_in(patterns, actual)),
            Matcher::StatusOf(value) => Ok(status_in(&statuses(&self.render(value)?)?, actual)),
        }
    }

    /// Whether `actual` passes `matcher`, with what the first call that fails reports.
    ///
    /// # Errors
    ///
    /// As [`Self::matches`].
    pub(crate) fn judged(&self, matcher: &Matcher, actual: Option<&Value>) -> Result<Judged, String> {
        let Matcher::All(calls) = matcher else { return self.matches(matcher, actual).map(Judged::from) };
        for call in calls {
            let judged = self.call(call, actual)?;
            if !judged.holds {
                return Ok(judged);
            }
        }
        Ok(Judged::from(true))
    }

    fn call(&self, call: &MatcherCall, actual: Option<&Value>) -> Result<Judged, String> {
        let entry = self.registry.get(&call.name).ok_or_else(|| format!("no matcher is named `{}`", call.name))?;
        match entry {
            Entry::Plain(matcher) => matcher(self, actual, &call.arg).map(Judged::from),
            Entry::Reporting(matcher) => matcher(self, actual, &call.arg),
        }
    }

    /// What `body` breaks the JSON Schema named `key` with (D201); none when it matches.
    ///
    /// # Errors
    ///
    /// A message when no schema was loaded for `key` or it is not a valid JSON Schema.
    pub fn schema(&self, key: &str, body: &Value) -> Result<Vec<String>, String> {
        self.args.schema(key, body)
    }

    /// `value`, rendered.
    ///
    /// # Errors
    ///
    /// The render error's message.
    pub fn render(&self, value: &TemplateValue) -> Result<Value, String> {
        self.args.render(value).map_err(|error| error.message)
    }

    /// A matcher argument as a value: templates rendered, durations in milliseconds, sizes in
    /// bytes, a range as `[low, high]` and nested matchers as they read.
    ///
    /// # Errors
    ///
    /// A message when a template in it does not render.
    pub fn value(&self, arg: &MatcherArg) -> Result<Value, String> {
        Ok(match arg {
            MatcherArg::Value(value) => self.render(value)?,
            MatcherArg::Duration(duration) => json!(duration.get().as_secs_f64() * 1000.0),
            MatcherArg::Size(size) => json!(size.bytes()),
            MatcherArg::Range(range) => {
                let [low, high] = range.as_ref();
                json!([self.value(low)?, self.value(high)?])
            }
            MatcherArg::Matcher(matcher) => self.expected(matcher)?,
            MatcherArg::Matchers(matchers) => {
                Value::Array(matchers.iter().map(|matcher| self.expected(matcher)).collect::<Result<_, _>>()?)
            }
            MatcherArg::Fields(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(name, matcher)| Ok((name.clone(), self.expected(matcher)?)))
                    .collect::<Result<_, String>>()?,
            ),
            MatcherArg::Paths(paths) => Value::Object(
                paths
                    .iter()
                    .map(|(path, matcher)| Ok((vc_template::dotted(&path.segments), self.expected(matcher)?)))
                    .collect::<Result<_, String>>()?,
            ),
        })
    }

    /// What `matcher` expects, for a report: the value, the matchers with their arguments, or
    /// the status codes and classes.
    ///
    /// # Errors
    ///
    /// A message when a template in it does not render.
    pub fn expected(&self, matcher: &Matcher) -> Result<Value, String> {
        Ok(match matcher {
            Matcher::Equals(value) => self.render(value)?,
            Matcher::All(calls) => match calls.as_slice() {
                [call] if self.registry.internal.contains(call.name.as_str()) => self.value(&call.arg)?,
                _ => {
                    let mut described = Map::new();
                    for call in calls {
                        described.insert(call.name.clone(), self.value(&call.arg)?);
                    }
                    Value::Object(described)
                }
            },
            Matcher::Status(patterns) => Value::Array(patterns.iter().map(|pattern| status(*pattern)).collect()),
            Matcher::StatusOf(value) => {
                let rendered = self.render(value)?;
                statuses(&rendered)
                    .map_or(rendered, |patterns| Value::Array(patterns.into_iter().map(status).collect()))
            }
        })
    }

    /// `arg` as text, for matchers that take a word or a pattern.
    ///
    /// # Errors
    ///
    /// A message when it does not render or is not text.
    pub fn text(&self, arg: &MatcherArg) -> Result<String, String> {
        match self.value(arg)? {
            Value::String(text) => Ok(text),
            other => Err(format!("expected text, got {other}")),
        }
    }
}

/// Whether `actual` is a status one of `patterns` matches.
fn status_in(patterns: &[StatusPattern], actual: Option<&Value>) -> bool {
    let status = actual.and_then(Value::as_u64).and_then(|status| u16::try_from(status).ok());
    status.is_some_and(|status| patterns.iter().any(|pattern| pattern.matches(status)))
}

/// A templated status's rendered value, read as a written status is: a code as a number or as
/// text (`201`, `"201"`), a class (`"2xx"`), or a list of them.
fn statuses(rendered: &Value) -> Result<Vec<StatusPattern>, String> {
    let items = match rendered {
        Value::Array(items) if items.is_empty() => return Err(not_a_status(rendered)),
        Value::Array(items) => items.as_slice(),
        one => std::slice::from_ref(one),
    };
    let pattern = |item: &Value| match item {
        Value::Number(number) => StatusPattern::parse(&number.to_string()),
        Value::String(text) => StatusPattern::parse(text),
        _ => None,
    };
    items.iter().map(|item| pattern(item).ok_or_else(|| not_a_status(rendered))).collect()
}

fn not_a_status(rendered: &Value) -> String {
    format!("the status {rendered} is not a code from 100 to 599, a class from 1xx to 5xx or a list of them")
}

/// A status pattern as written: `201` or `"2xx"`.
fn status(pattern: StatusPattern) -> Value {
    match pattern {
        StatusPattern::Code(code) => json!(code),
        StatusPattern::Class(class) => json!(format!("{class}xx")),
    }
}
