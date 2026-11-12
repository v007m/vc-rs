//! Definitions shown as written, for `vc explain` and `vc config explain`: a value's text,
//! with `${var.x}` replaced by what `x` is set to where the vars are known, its pipes applied.

use serde_json::Value;
use vc_model::{CelProgram, Namespace, Part, Program, Reference, Segment, Template, TemplateValue};
use vc_template::{Chance, PipeRegistry, RenderError, Renderer, Resolve};

use crate::notify::Steady;

/// A var in force: its name, its value and the level that set it.
#[derive(Clone, Debug)]
pub struct Var {
    pub name: String,
    pub value: TemplateValue,
    pub origin: String,
    /// Whether a run saves it from a response, so it is known only then and a reference to it
    /// is shown as written.
    pub saved: bool,
    /// The values it took the place of, latest first, each with the level that set it.
    pub replaced: Vec<(TemplateValue, String)>,
}

/// How deep `${var.x}` is followed before it is shown as written, so a var that names itself
/// ends.
const DEPTH: u8 = 8;

/// Shows values over `vars`; with none, every reference is shown as written.
pub struct Shown<'a> {
    pub program: &'a Program,
    pub vars: &'a [Var],
}

impl Shown<'_> {
    /// `value` as text: a string with its vars filled in, a list or map in brackets.
    pub fn value(&self, value: &TemplateValue) -> String {
        self.value_at(value, 0)
    }

    /// `template` as text, its vars filled in.
    pub fn template(&self, template: &Template) -> String {
        self.template_at(template, 0)
    }

    fn value_at(&self, value: &TemplateValue, depth: u8) -> String {
        match value {
            TemplateValue::Null => "null".to_owned(),
            TemplateValue::Bool(flag) => flag.to_string(),
            TemplateValue::Number(number) => number.to_string(),
            TemplateValue::String(template) => self.template_at(template, depth),
            TemplateValue::List(items) => {
                let items: Vec<String> = items.iter().map(|item| self.value_at(item, depth)).collect();
                format!("[{}]", items.join(", "))
            }
            TemplateValue::Map(map) => {
                let pairs: Vec<String> =
                    map.iter().map(|(key, item)| format!("{key}: {}", self.value_at(item, depth))).collect();
                format!("{{{}}}", pairs.join(", "))
            }
            TemplateValue::Cel(_) => "$cel(…)".to_owned(),
            TemplateValue::Extend { base, .. } => format!("$extend {}", self.template_at(base, depth)),
        }
    }

    fn template_at(&self, template: &Template, depth: u8) -> String {
        let mut text = String::new();
        for part in &template.parts {
            match part {
                Part::Literal(literal) => text.push_str(literal),
                Part::Ref(reference) => match self.var(reference, depth) {
                    Some(value) => text.push_str(&value),
                    None => text.push_str(&self.written(reference)),
                },
            }
        }
        text
    }

    /// What `${var.x}` is set to, its pipes applied, when `x` is known and not too deep.
    fn var(&self, reference: &Reference, depth: u8) -> Option<String> {
        let [Segment::Key(name)] = reference.path.segments.as_slice() else { return None };
        if reference.ns != Namespace::Var || !reference.args.is_empty() || depth >= DEPTH {
            return None;
        }
        let var = self.vars.iter().find(|var| var.name == *name && !var.saved)?;
        let text = self.value_at(&var.value, depth.saturating_add(1));
        if reference.pipes.is_empty() {
            return Some(text);
        }
        piped(reference, text)
    }

    /// `reference` as written: `${response.body.id | int}`.
    fn written(&self, reference: &Reference) -> String {
        let root = match reference.ns {
            Namespace::Step(sym) => self.program.str(sym).unwrap_or("?"),
            other => vc_template::root_name(other).unwrap_or("?"),
        };
        let pipes: String =
            reference.pipes.iter().flat_map(|pipe| [" | ".to_owned(), pipe.name.clone(), args(&pipe.args)]).collect();
        let path = vc_template::path_text(&reference.path.segments);
        format!("${{{root}{path}{}{pipes}}}", args(&reference.args))
    }
}

/// A pipe's or generator's arguments as written, `('x', 2)`; nothing when it has none.
fn args(args: &[Value]) -> String {
    if args.is_empty() {
        return String::new();
    }
    let args: Vec<String> = args
        .iter()
        .map(|arg| match arg {
            Value::String(text) => format!("'{text}'"),
            other => other.to_string(),
        })
        .collect();
    format!("({})", args.join(", "))
}

/// `text` through `reference`'s pipes; none when it still holds a reference or a pipe fails.
/// The value is text, so a list pipe such as `random` fails and is left to the run.
fn piped(reference: &Reference, text: String) -> Option<String> {
    if text.contains("${") {
        return None;
    }
    let known = Known(Value::String(text));
    let pipes = PipeRegistry::builtin();
    let template = Template { parts: vec![Part::Ref(reference.clone())] };
    Renderer::new(&pipes, &known).text(&template).ok()
}

/// A var's known value, before its pipes.
struct Known(Value);

impl Resolve for Known {
    fn resolve(&self, _reference: &Reference) -> Result<Value, RenderError> {
        Ok(self.0.clone())
    }

    fn evaluate(&self, _program: &CelProgram) -> Result<Value, RenderError> {
        Err(RenderError::invalid("evaluated when the run sends it"))
    }

    fn chance(&self) -> &dyn Chance {
        &Steady
    }
}

/// A literal string value, as a setting from outside the files sets a var.
pub fn literal(text: &str) -> TemplateValue {
    TemplateValue::String(Template { parts: vec![Part::Literal(text.to_owned())] })
}

/// Sets `var`, in the place of an earlier one with its name, which it keeps as replaced.
pub fn set(vars: &mut Vec<Var>, mut var: Var) {
    if let Some(known) = vars.iter_mut().find(|known| known.name == var.name) {
        let earlier = std::mem::replace(known, var);
        known.replaced = std::iter::once((earlier.value, earlier.origin)).chain(earlier.replaced).collect();
    } else {
        var.replaced.clear();
        vars.push(var);
    }
}

#[cfg(test)]
#[path = "shown_tests.rs"]
mod tests;
