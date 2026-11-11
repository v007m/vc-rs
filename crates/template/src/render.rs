//! Renders templates and template values at run time (LLD "Vars, templates and CEL at run
//! time"). The values of the roots come from a [`Resolve`], so rendering stays pure.

use serde_json::{Map, Value};
use vc_model::{CelProgram, Part, PipeCall, Reference, Template, TemplateValue};

use crate::merge;
use crate::pipe_fns::{Chance, PipeArgs};
use crate::select;
use crate::{PipeRegistry, RenderError};

/// Gives the value of each reference and CEL expression a template holds.
pub trait Resolve {
    /// The value of `reference` before its pipes: its root followed along its path.
    ///
    /// # Errors
    ///
    /// A missing [`RenderError`] when the value is not there, or an invalid one when the root
    /// cannot be read here.
    fn resolve(&self, reference: &Reference) -> Result<Value, RenderError>;

    /// The value of a CEL expression.
    ///
    /// # Errors
    ///
    /// An invalid [`RenderError`] when evaluation fails.
    fn evaluate(&self, program: &CelProgram) -> Result<Value, RenderError>;

    /// The random source for `random` and `shuffle`.
    fn chance(&self) -> &dyn Chance;
}

/// Renders against one [`Resolve`] with one set of pipes.
#[derive(Clone, Copy)]
pub struct Renderer<'a> {
    pipes: &'a PipeRegistry,
    resolve: &'a dyn Resolve,
}

impl<'a> Renderer<'a> {
    /// A renderer applying `pipes` to the values `resolve` gives.
    #[must_use]
    pub const fn new(pipes: &'a PipeRegistry, resolve: &'a dyn Resolve) -> Self {
        Self { pipes, resolve }
    }

    /// A template's value: a lone `${…}` keeps its value's type; anything else is text.
    ///
    /// # Errors
    ///
    /// The first reference that cannot be rendered.
    pub fn template(&self, template: &Template) -> Result<Value, RenderError> {
        template
            .single_ref()
            .map_or_else(|| self.text(template).map(Value::String), |reference| self.reference(reference))
    }

    /// A template as text: strings as they are, anything else as compact JSON.
    ///
    /// # Errors
    ///
    /// The first reference that cannot be rendered.
    pub fn text(&self, template: &Template) -> Result<String, RenderError> {
        template.parts.iter().try_fold(String::new(), |mut text, part| {
            match part {
                Part::Literal(literal) => text.push_str(literal),
                Part::Ref(reference) => text.push_str(&to_text(&self.reference(reference)?)),
            }
            Ok(text)
        })
    }

    /// A template value: lists and objects element by element, CEL evaluated, and `$extend`
    /// merged.
    ///
    /// # Errors
    ///
    /// The first part that cannot be rendered.
    pub fn value(&self, value: &TemplateValue) -> Result<Value, RenderError> {
        Ok(match value {
            TemplateValue::Null => Value::Null,
            TemplateValue::Bool(flag) => Value::Bool(*flag),
            TemplateValue::Number(number) => Value::Number(number.clone()),
            TemplateValue::String(template) => self.template(template)?,
            TemplateValue::List(items) => {
                Value::Array(items.iter().map(|item| self.value(item)).collect::<Result<_, _>>()?)
            }
            TemplateValue::Map(entries) => Value::Object(self.object(entries.iter())?),
            TemplateValue::Cel(program) => self.resolve.evaluate(program)?,
            TemplateValue::Extend { base, changes, remove } => {
                merge::extend(self.template(base)?, self.value(changes)?, remove)?
            }
        })
    }

    fn object<'v>(
        &self,
        entries: impl IntoIterator<Item = (&'v String, &'v TemplateValue)>,
    ) -> Result<Map<String, Value>, RenderError> {
        entries.into_iter().map(|(key, value)| Ok((key.clone(), self.value(value)?))).collect()
    }

    /// One reference with its pipes; a missing value reaches a first pipe that takes it as
    /// `null`.
    fn reference(&self, reference: &Reference) -> Result<Value, RenderError> {
        let value = match self.resolve.resolve(reference) {
            Ok(value) => value,
            Err(error) if error.missing && self.takes_missing(reference.pipes.first()) => Value::Null,
            Err(error) => return Err(error),
        };
        reference.pipes.iter().try_fold(value, |value, call| self.pipe(&value, call))
    }

    fn takes_missing(&self, call: Option<&PipeCall>) -> bool {
        call.and_then(|call| self.pipes.get(&call.name)).is_some_and(|def| def.takes_missing)
    }

    fn pipe(&self, value: &Value, call: &PipeCall) -> Result<Value, RenderError> {
        let name = &call.name;
        let def = self.pipes.get(name).ok_or_else(|| RenderError::invalid(format!("unknown pipe `{name}`")))?;
        let args = PipeArgs { args: &call.args, chance: self.resolve.chance() };
        (def.apply)(value, &args).map_err(|reason| {
            RenderError::invalid(format!("pipe `{name}` cannot take {}: {reason}", select::preview(value)))
        })
    }
}

/// A value as it appears inside text: a string as it is, anything else as compact JSON.
#[must_use]
pub fn to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
