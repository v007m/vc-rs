//! The matcher registry and the judge that applies matchers (syntax reference "Checks and
//! matchers").

use std::collections::HashMap;
use std::fmt;

use serde_json::{Map, Value, json};
use vc_model::{Matcher, MatcherArg, MatcherCall, StatusPattern, TemplateValue};
use vc_template::RenderError;

use super::{compare, compose, shape, text};
use crate::Render;

/// Renders the templates in a check's expected values: the call's [`Render`] in a run.
pub trait Arguments {
    /// `value`, rendered.
    ///
    /// # Errors
    ///
    /// A [`RenderError`] when a reference in it cannot be rendered.
    fn render(&self, value: &TemplateValue) -> Result<Value, RenderError>;
}

impl Arguments for Render<'_> {
    fn render(&self, value: &TemplateValue) -> Result<Value, RenderError> {
        self.value(value)
    }
}

/// One matcher: whether `actual` (`None` when the path is absent) passes with `arg`.
///
/// It fails with a message when `arg` cannot be used, such as a template that does not render.
pub type MatcherFn = fn(&Judge<'_>, Option<&Value>, &MatcherArg) -> Result<bool, String>;

/// The matchers by name (a registry filled at start-up).
#[derive(Clone, Default)]
pub struct MatcherRegistry {
    by_name: HashMap<&'static str, MatcherFn>,
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
        compose::register(text::register(shape::register(compare::register(registry))))
    }

    /// The registry with `matcher` under `name`.
    #[must_use]
    pub fn with(mut self, name: &'static str, matcher: MatcherFn) -> Self {
        self.by_name.insert(name, matcher);
        self
    }

    fn get(&self, name: &str) -> Option<MatcherFn> {
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
            Matcher::All(calls) => {
                for call in calls {
                    if !self.call(call, actual)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Matcher::Status(patterns) => {
                let status = actual.and_then(Value::as_u64).and_then(|status| u16::try_from(status).ok());
                Ok(status.is_some_and(|status| patterns.iter().any(|pattern| pattern.matches(status))))
            }
        }
    }

    fn call(&self, call: &MatcherCall, actual: Option<&Value>) -> Result<bool, String> {
        let matcher = self.registry.get(&call.name).ok_or_else(|| format!("no matcher is named `{}`", call.name))?;
        matcher(self, actual, &call.arg)
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
            Matcher::All(calls) => {
                let mut described = Map::new();
                for call in calls {
                    described.insert(call.name.clone(), self.value(&call.arg)?);
                }
                Value::Object(described)
            }
            Matcher::Status(patterns) => Value::Array(patterns.iter().map(|pattern| status(*pattern)).collect()),
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

/// A status pattern as written: `201` or `"2xx"`.
fn status(pattern: StatusPattern) -> Value {
    match pattern {
        StatusPattern::Code(code) => json!(code),
        StatusPattern::Class(class) => json!(format!("{class}xx")),
    }
}
