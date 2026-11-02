use std::collections::HashMap;

use serde_json::Value;
use vc_diagnostics::{Diagnostic, Fix, TextEdit};
use vc_model::PipeCall;

use crate::codes;

/// The type of a value as far as the compiler can tell before the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueType {
    /// Not known until run time; fits everywhere.
    Any,
    /// A string, number or boolean.
    Scalar,
    /// A string.
    Text,
    /// A number.
    Number,
    /// `true` or `false`.
    Bool,
    /// A list.
    List,
    /// An object.
    Object,
}

impl ValueType {
    /// Whether a value of type `actual` may be passed where `self` is expected.
    #[must_use]
    pub fn accepts(self, actual: Self) -> bool {
        let scalar = matches!(actual, Self::Text | Self::Number | Self::Bool);
        self == actual || self == Self::Any || actual == Self::Any || (self == Self::Scalar && scalar)
    }
}

/// The kind of literal a pipe argument must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArgType {
    /// Any literal.
    Any,
    /// A whole number of zero or more.
    Count,
}

impl ArgType {
    fn accepts(self, value: &Value) -> bool {
        self == Self::Any || value.as_u64().is_some()
    }
}

/// What a pipe takes and gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipeSignature {
    /// One entry per argument, in order.
    pub args: &'static [ArgType],
    /// The value the pipe applies to.
    pub input: ValueType,
    /// The value it gives.
    pub output: ValueType,
}

impl PipeSignature {
    /// A pipe from `input` to `output` taking `args`.
    #[must_use]
    pub const fn new(input: ValueType, output: ValueType, args: &'static [ArgType]) -> Self {
        Self { args, input, output }
    }
}

/// One registered pipe. Run-time application is added here in M2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipeDef {
    /// What it takes and gives.
    pub signature: PipeSignature,
}

/// The pipes by name: the built-ins, plus any a plugin registers under its prefix.
#[derive(Clone, Debug, Default)]
pub struct PipeRegistry {
    pipes: HashMap<String, PipeDef>,
}

impl PipeRegistry {
    /// A registry holding the built-in pipes.
    #[must_use]
    pub fn builtin() -> Self {
        let mut registry = Self::default();
        for (name, signature) in crate::builtin_pipes::BUILTIN {
            registry.register(*name, *signature);
        }
        registry
    }

    /// Adds or replaces the pipe `name`.
    pub fn register(&mut self, name: impl Into<String>, signature: PipeSignature) {
        self.pipes.insert(name.into(), PipeDef { signature });
    }

    /// The pipe `name`, if registered.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&PipeDef> {
        self.pipes.get(name)
    }

    /// Checks a chain of pipes applied to a value of type `input`, and gives the result type.
    ///
    /// # Errors
    ///
    /// The first unknown pipe (with did-you-mean), wrong argument count or type, or pipe that
    /// cannot take the type the previous step gives.
    pub fn check_chain(&self, pipes: &[PipeCall], input: ValueType) -> Result<ValueType, Box<Diagnostic>> {
        pipes.iter().try_fold(input, |current, call| {
            let signature = self.check(call)?;
            if signature.input.accepts(current) {
                return Ok(signature.output);
            }
            let message = format!("pipe `{}` takes {:?}, not {current:?}", call.name, signature.input);
            Err(Box::new(Diagnostic::new(codes::PIPE_TYPE, call.span, message)))
        })
    }

    /// Checks one call: the pipe exists and its arguments fit its signature.
    ///
    /// # Errors
    ///
    /// An unknown pipe (with did-you-mean) or the wrong argument count or type.
    pub fn check(&self, call: &PipeCall) -> Result<PipeSignature, Box<Diagnostic>> {
        let Some(def) = self.get(&call.name) else { return Err(Box::new(self.unknown(call))) };
        let params = def.signature.args;
        if params.len() != call.args.len() {
            let message = format!("pipe `{}` takes {} argument(s), not {}", call.name, params.len(), call.args.len());
            return Err(Box::new(Diagnostic::new(codes::PIPE_CALL, call.span, message)));
        }
        let bad = params.iter().zip(&call.args).enumerate().find(|(_, (param, arg))| !param.accepts(arg));
        bad.map_or(Ok(def.signature), |(index, (param, _))| {
            let message = format!("argument {} of pipe `{}` must be {param:?}", index + 1, call.name);
            Err(Box::new(Diagnostic::new(codes::PIPE_CALL, call.span, message)))
        })
    }

    fn unknown(&self, call: &PipeCall) -> Diagnostic {
        let diagnostic = Diagnostic::new(codes::PIPE_CALL, call.name_span, format!("unknown pipe `{}`", call.name));
        let closest = self
            .pipes
            .keys()
            .map(|name| (strsim::damerau_levenshtein(name, &call.name), name))
            .filter(|(distance, _)| *distance <= 2)
            .min();
        let Some((_, name)) = closest else { return diagnostic };
        let diagnostic = diagnostic.help(format!("did you mean `{name}`?"));
        if call.name_span.byte_range().len() != call.name.len() {
            return diagnostic;
        }
        diagnostic.fix(Fix::suggested(format!("Replace with {name}"), vec![TextEdit::replace(call.name_span, name)]))
    }
}
