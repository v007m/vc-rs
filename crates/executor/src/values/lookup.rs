//! Answers a template's references for [`Render`]: vars through the scope, `env` with declared
//! secrets refused, `fake` generators, and the bound roots.

use std::sync::Arc;

use serde_json::{Map, Value};
use vc_diagnostics::Code;
use vc_model::{CelProgram, Namespace, Reference, Segment};
use vc_template::{Activation, CelCompiler, Chance, FakeCx, RenderError, Resolve};

use super::render::{Render, Within};

/// Answers the renderer's lookups for one template.
pub(super) struct Lookup<'r> {
    pub(super) render: &'r Render<'r>,
    pub(super) within: Option<&'r Within<'r>>,
}

impl Resolve for Lookup<'_> {
    fn resolve(&self, reference: &Reference) -> Result<Value, RenderError> {
        match reference.ns {
            Namespace::Var => {
                let (name, rest) = named("var", reference)?;
                vc_template::select(&format!("var.{name}"), &self.render.var(name, self.within)?, rest)
            }
            Namespace::Env => {
                let (name, rest) = named("env", reference)?;
                vc_template::select(&format!("env.{name}"), &self.env(name)?, rest)
            }
            Namespace::Fake => self.fake(reference),
            ns => {
                let label = self.render.label(ns);
                let value = self.render.roots.get(&ns);
                let value = value.ok_or_else(|| RenderError::invalid(format!("`{label}` has no value here")))?;
                let selected = vc_template::select(&label, value, &reference.path.segments);
                selected.map_err(|error| row_column(error, (ns, value), reference))
            }
        }
    }

    fn evaluate(&self, program: &CelProgram) -> Result<Value, RenderError> {
        let expr = CelCompiler::new().prepare(program).map_err(|error| RenderError::invalid(error.to_string()))?;
        let roots =
            self.render.roots.iter().fold(Activation::new(), |roots, (ns, value)| roots.with(*ns, Arc::clone(value)));
        let activation = roots.with(Namespace::Var, self.vars(program)?).with(Namespace::Env, self.envs(program));
        expr.eval(&activation).map_err(|error| RenderError::invalid(error.to_string()))
    }

    fn chance(&self) -> &dyn Chance {
        self.render.chance()
    }
}

impl Lookup<'_> {
    fn env(&self, name: &str) -> Result<Value, RenderError> {
        if self.render.program.config.secrets.declares(name) {
            let message = format!("`env.{name}` is a declared secret; use `${{secret.{name}}}` so it is masked");
            return Err(RenderError::invalid(message));
        }
        let value = self.render.values.inputs().environment.var(name);
        value.map(Value::String).ok_or_else(|| RenderError::missing(format!("`env.{name}` is not set")))
    }

    fn fake(&self, reference: &Reference) -> Result<Value, RenderError> {
        let keys = reference.path.segments.iter().filter_map(|segment| match segment {
            Segment::Key(key) => Some(key.as_str()),
            Segment::Index(_) | Segment::All => None,
        });
        let name = keys.collect::<Vec<_>>().join(".");
        let inputs = self.render.values.inputs();
        let cx = FakeCx::new(self.chance(), inputs.started_ms);
        let program = self.render.program;
        let named = program.generators.values().find(|def| program.str(def.id) == Some(name.as_str()));
        let (name, args) = named.map_or((name.as_str(), reference.args.as_slice()), |def| (&def.fake, &def.args));
        inputs.fakes.generate(name, args, &cx).map_err(RenderError::invalid)
    }

    /// The `var` root of a `$cel` value: each var it reads that is set.
    fn vars(&self, program: &CelProgram) -> Result<Value, RenderError> {
        let mut vars = Map::new();
        for name in read_names(program, Namespace::Var) {
            match self.render.var(name, self.within) {
                Ok(value) => drop(vars.insert(name.to_owned(), value)),
                Err(error) if error.missing => {}
                Err(error) => return Err(error),
            }
        }
        Ok(Value::Object(vars))
    }

    /// The `env` root of a `$cel` value: each variable it reads that is set.
    fn envs(&self, program: &CelProgram) -> Value {
        let found =
            read_names(program, Namespace::Env).filter_map(|name| Some((name.to_owned(), self.env(name).ok()?)));
        Value::Object(found.collect())
    }
}

/// The names a `$cel` value reads under `ns`.
fn read_names(program: &CelProgram, ns: Namespace) -> impl Iterator<Item = &str> {
    program.refs.iter().filter(move |reference| reference.ns == ns).filter_map(|reference| {
        match reference.path.segments.first() {
            Some(Segment::Key(name)) => Some(name.as_str()),
            _ => None,
        }
    })
}

/// A name and the path after it.
type Named<'r> = (&'r str, &'r [Segment]);

/// The name after `root` and the path after it.
fn named<'r>(root: &str, reference: &'r Reference) -> Result<Named<'r>, RenderError> {
    match reference.path.segments.split_first() {
        Some((Segment::Key(name), rest)) => Ok((name.as_str(), rest)),
        _ => Err(RenderError::invalid(format!("`{root}` needs a name after it, such as `{root}.name`"))),
    }
}

/// V5112 when an `example` or `item` row is a map without the column `reference` reads (D173).
fn row_column(error: RenderError, (ns, row): (Namespace, &Value), reference: &Reference) -> RenderError {
    let Some(Segment::Key(column)) = reference.path.segments.first() else { return error };
    let rows = matches!(ns, Namespace::Example | Namespace::Item);
    match row {
        Value::Object(row) if rows && !row.contains_key(column.as_str()) => error.coded(Code::V5112),
        _ => error,
    }
}
