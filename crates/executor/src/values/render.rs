//! Renders one action's templates against its scope and bound roots (LLD "Vars, templates and
//! CEL at run time").

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Map, Value, json};
use vc_interfaces::Random;
use vc_model::{CelProgram, Examples, Namespace, Program, Reference, Segment, Template, TemplateValue};
use vc_template::{Activation, CelCompiler, Chance, RenderError, Renderer, Resolve, root_name, select};

use super::run::RunValues;
use super::scope::{Found, Scope};
use crate::{ActionRequest, Frame, Saved};

/// How many vars a definition may pass through before the lookup gives up on a loop.
const MAX_DEPTH: usize = 16;

/// One action's renderer: its scope, the roots bound so far (`run`, `iteration`, `example`, step
/// ids, then `request` and `response`) and the fake-data memo.
pub struct Render<'a> {
    values: &'a RunValues,
    program: &'a Program,
    scope: Scope,
    roots: HashMap<Namespace, Arc<Value>>,
    memo: Saved,
    chance: RandomChance<'a>,
}

impl<'a> Render<'a> {
    /// The renderer for `request`'s action. Vars defined as one `${fake.…}` that the frame has not
    /// generated yet are generated now, in name order, so each is drawn once per instance.
    ///
    /// # Errors
    ///
    /// When the instance's example row does not render.
    pub fn new(request: &'a ActionRequest) -> Result<Self, RenderError> {
        let cx = &*request.cx;
        let mut render = Self {
            values: &cx.values,
            program: &cx.program,
            scope: Scope::of(request),
            roots: HashMap::new(),
            memo: Saved::clone(&request.frame.fakes),
            chance: RandomChance(cx.values.inputs().random.as_ref()),
        };
        render.bind(Namespace::Run, json!({ "profiles": render.values.profiles() }));
        for (id, response) in request.frame.steps.iter() {
            render.roots.insert(Namespace::Step(*id), Arc::clone(response));
        }
        render.memoize();
        render.bind_instance(&request.frame)?;
        Ok(render)
    }

    /// Binds `ns` to `value` for the renders that follow.
    pub fn bind(&mut self, ns: Namespace, value: Value) {
        self.roots.insert(ns, Arc::new(value));
    }

    /// Renders a template value.
    ///
    /// # Errors
    ///
    /// When a reference is missing or cannot be read, a pipe fails or a `$cel` value fails.
    pub fn value(&self, value: &TemplateValue) -> Result<Value, RenderError> {
        Renderer::new(self.values.pipes(), &Lookup { render: self, within: None }).value(value)
    }

    /// Renders a template, keeping the type of a lone reference.
    ///
    /// # Errors
    ///
    /// As [`Self::value`].
    pub fn template(&self, template: &Template) -> Result<Value, RenderError> {
        Renderer::new(self.values.pipes(), &Lookup { render: self, within: None }).template(template)
    }

    /// Renders a template to text.
    ///
    /// # Errors
    ///
    /// As [`Self::value`].
    pub fn text(&self, template: &Template) -> Result<String, RenderError> {
        Renderer::new(self.values.pipes(), &Lookup { render: self, within: None }).text(template)
    }

    /// The program the action belongs to.
    pub const fn program(&self) -> &'a Program {
        self.program
    }

    /// The run's values.
    pub const fn values(&self) -> &'a RunValues {
        self.values
    }

    /// The fake-data memo, for the frame to keep.
    pub fn into_memo(self) -> Saved {
        self.memo
    }

    fn memoize(&mut self) {
        for name in self.scope.defined_names() {
            if self.memo.contains_key(&name) {
                continue;
            }
            let Some((level, Found::Defined(def))) = self.scope.find(self.program, &name, 0) else { continue };
            if !is_fake(def) {
                continue;
            }
            if let Ok(value) = self.defined(def, &Within { name: &name, level, depth: 1 }) {
                self.memo.insert(name, value);
            }
        }
    }

    /// Binds `iteration` and the instance's `example` row.
    fn bind_instance(&mut self, frame: &Frame) -> Result<(), RenderError> {
        let Some(instance) = &frame.instance else { return Ok(()) };
        let Some(test) = self.program.tests.get(&instance.test) else { return Ok(()) };
        let index = u64::from(instance.repeat_index) + 1;
        self.bind(Namespace::Iteration, json!({ "index": index, "count": test.repeat }));
        let Some(row) = instance.example_row else { return Ok(()) };
        let row = match &test.examples {
            Some(Examples::Rows(rows)) => rows.get(usize::try_from(row).unwrap_or(usize::MAX)),
            _ => return Err(RenderError::invalid("examples from a file or a template arrive in a later milestone")),
        };
        let row = row.ok_or_else(|| RenderError::invalid("the example row is not in the test"))?;
        let value = self.value(row)?;
        self.bind(Namespace::Example, value);
        Ok(())
    }

    /// The value of `var.<name>`, looking from the closest level unless `within` is the
    /// definition of the same name, which continues past its own level.
    fn var(&self, name: &str, within: Option<&Within<'_>>) -> Result<Value, RenderError> {
        let (from, depth) = within.map_or((0, 0), |within| (within.after(name), within.depth));
        if depth > MAX_DEPTH {
            let message =
                format!("`var.{name}` goes through more than {MAX_DEPTH} vars; do some vars refer to each other?");
            return Err(RenderError::invalid(message));
        }
        let found = self.scope.find(self.program, name, from);
        let (level, found) = found.ok_or_else(|| RenderError::missing(format!("`var.{name}` is not set")))?;
        match found {
            Found::Value(value) => Ok(value),
            Found::Defined(def) => self.memo.get(name).filter(|_| is_fake(def)).map_or_else(
                || self.defined(def, &Within { name, level, depth: depth + 1 }),
                |value| Ok(value.clone()),
            ),
        }
    }

    fn defined(&self, def: &TemplateValue, within: &Within<'_>) -> Result<Value, RenderError> {
        Renderer::new(self.values.pipes(), &Lookup { render: self, within: Some(within) }).value(def)
    }

    fn label(&self, ns: Namespace) -> String {
        match ns {
            Namespace::Step(id) => self.program.str(id).unwrap_or("step").to_owned(),
            other => root_name(other).unwrap_or("root").to_owned(),
        }
    }
}

/// A var defined as exactly one `${fake.…}` is generated once per instance (D131).
fn is_fake(def: &TemplateValue) -> bool {
    let TemplateValue::String(template) = def else { return false };
    template.single_ref().is_some_and(|reference| reference.ns == Namespace::Fake)
}

/// The var definition being rendered.
struct Within<'n> {
    name: &'n str,
    level: usize,
    depth: usize,
}

impl Within<'_> {
    /// Where a lookup of `name` starts: past this definition's level for its own name.
    fn after(&self, name: &str) -> usize {
        if self.name == name { self.level + 1 } else { 0 }
    }
}

/// The run's random source as the pipes' [`Chance`].
struct RandomChance<'a>(&'a dyn Random);

impl Chance for RandomChance<'_> {
    fn below(&self, bound: usize) -> usize {
        let draw = self.0.next_u64().checked_rem(u64::try_from(bound).unwrap_or(u64::MAX));
        draw.and_then(|draw| usize::try_from(draw).ok()).unwrap_or(0)
    }
}

/// Answers the renderer's lookups for one template.
struct Lookup<'r> {
    render: &'r Render<'r>,
    within: Option<&'r Within<'r>>,
}

impl Resolve for Lookup<'_> {
    fn resolve(&self, reference: &Reference) -> Result<Value, RenderError> {
        match reference.ns {
            Namespace::Var => {
                let (name, rest) = named("var", reference)?;
                select(&format!("var.{name}"), &self.render.var(name, self.within)?, rest)
            }
            Namespace::Env => {
                let (name, rest) = named("env", reference)?;
                select(&format!("env.{name}"), &self.env(name)?, rest)
            }
            Namespace::Fake => self.fake(reference),
            ns => {
                let label = self.render.label(ns);
                let value = self.render.roots.get(&ns);
                let value = value.ok_or_else(|| RenderError::invalid(format!("`{label}` has no value here")))?;
                select(&label, value, &reference.path.segments)
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
        &self.render.chance
    }
}

impl Lookup<'_> {
    fn env(&self, name: &str) -> Result<Value, RenderError> {
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
        inputs.fakes.generate(&name, inputs.random.as_ref(), &reference.args).map_err(RenderError::invalid)
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
