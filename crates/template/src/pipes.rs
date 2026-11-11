use std::collections::HashMap;
use std::fmt;

use serde_json::Value;
use vc_diagnostics::{Diagnostic, Fix, TextEdit};
use vc_model::PipeCall;

use crate::pipe_fns::PipeFn;
use crate::{TemplateError, codes};

/// The type of a value as far as the compiler can tell before the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ValueType {
    /// Not known until run time; fits everywhere.
    Any,
    /// A string, number or boolean.
    Scalar,
    /// A string or a number.
    TextOrNumber,
    /// A string or a boolean.
    TextOrBool,
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
        let within = match self {
            Self::Scalar => matches!(actual, Self::Text | Self::Number | Self::Bool),
            Self::TextOrNumber => matches!(actual, Self::Text | Self::Number),
            Self::TextOrBool => matches!(actual, Self::Text | Self::Bool),
            _ => false,
        };
        self == actual || self == Self::Any || actual == Self::Any || within
    }

    /// The type in the syntax reference's words: `a list`, `text`.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Any => "any value",
            Self::Scalar => "text, a number or a boolean",
            Self::TextOrNumber => "text or a number",
            Self::TextOrBool => "text or a boolean",
            Self::Text => "text",
            Self::Number => "a number",
            Self::Bool => "a boolean",
            Self::List => "a list",
            Self::Object => "an object",
        }
    }
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.describe())
    }
}

/// The kind of literal a pipe argument must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
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

    /// The argument kind in the syntax reference's words: `a whole number ≥ 0`.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Any => "any literal",
            Self::Count => "a whole number ≥ 0",
        }
    }
}

impl fmt::Display for ArgType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.describe())
    }
}

/// What a pipe takes and gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
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

/// One registered pipe: what it takes and gives, and what it does at run time.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct PipeDef {
    /// What it takes and gives.
    pub signature: PipeSignature,
    /// What it does.
    pub apply: PipeFn,
    /// Whether it also runs on a value that is missing, given as `null` (`default`).
    pub takes_missing: bool,
}

impl PipeDef {
    /// A pipe with `signature` that runs `apply` on values that are present.
    #[must_use]
    pub const fn new(signature: PipeSignature, apply: PipeFn) -> Self {
        Self { signature, apply, takes_missing: false }
    }

    /// The same pipe, also run on a missing value, given as `null`.
    #[must_use]
    pub const fn taking_missing(mut self) -> Self {
        self.takes_missing = true;
        self
    }
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
        let pipes = crate::builtin_pipes::BUILTIN.iter();
        Self { pipes: pipes.map(|(name, def)| ((*name).to_owned(), *def)).collect() }
    }

    /// Adds a plugin pipe, named under its plugin's prefix: `acme.mask`.
    ///
    /// # Errors
    ///
    /// [`TemplateError::UnprefixedPipe`] for a name without a `prefix.`, and
    /// [`TemplateError::DuplicatePipe`] for a name already registered.
    pub fn register(&mut self, name: &str, def: PipeDef) -> Result<(), TemplateError> {
        let words: Vec<&str> = name.split('.').collect();
        if words.len() < 2 || words.contains(&"") {
            return Err(TemplateError::UnprefixedPipe(name.to_owned()));
        }
        if self.pipes.contains_key(name) {
            return Err(TemplateError::DuplicatePipe(name.to_owned()));
        }
        self.pipes.insert(name.to_owned(), def);
        Ok(())
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
            let message = format!("pipe `{}` takes {}, not {current}", call.name, signature.input);
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
            let message = format!("argument {} of pipe `{}` must be {param}", index + 1, call.name);
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
