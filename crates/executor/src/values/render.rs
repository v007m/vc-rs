//! Renders one action's templates against its scope and bound roots (LLD "Vars, templates and
//! CEL at run time").

use std::cell::OnceCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde_json::{Value, json};
use vc_interfaces::Random;
use vc_model::{Failure, Namespace, Program, Template, TemplateValue};
use vc_template::{Chance, RenderError, Renderer};

use super::lookup::Lookup;
use super::reads::Reads;
use super::run::{Defined, RunValues};
use super::scope::{Found, Scope};
use crate::{ActionRequest, Frame, Redactor, Saved, Schemas};

/// How many vars a definition may pass through before the lookup gives up on a loop.
const MAX_DEPTH: usize = 16;

/// The fake vars drawn by this render, one slot per var not drawn yet, kept in name order.
type Drawn = BTreeMap<String, OnceCell<Value>>;

/// One action's renderer: its scope, the roots bound so far (`run`, `dataset`, `iteration`,
/// `example`, step ids, then `request` and `response`) and the fake-data memo.
pub struct Render<'a> {
    pub(super) values: &'a RunValues,
    pub(super) program: &'a Program,
    redactor: &'a Redactor,
    schemas: &'a Schemas,
    scope: Scope,
    pub(super) roots: HashMap<Namespace, Arc<Value>>,
    memo: Saved,
    drawn: Drawn,
    reads: Reads,
    chance: RandomChance<'a>,
}

impl<'a> Render<'a> {
    /// The renderer for `request`'s action. A var defined as one `${fake.…}` that the frame has not
    /// drawn yet is drawn when first rendered, so each is drawn once per instance and unused ones
    /// draw nothing.
    ///
    /// # Errors
    ///
    /// When the instance's example row does not render.
    pub fn new(request: &'a ActionRequest) -> Result<Self, RenderError> {
        let cx = &*request.cx;
        let (values, program, redactor) = (&cx.values, &cx.program, &cx.redactor);
        let mut render = Self::over(Shared { values, program, redactor, schemas: cx.schemas() }, Scope::of(request));
        render.memo.clone_from(&request.frame.fakes);
        render.reads = Reads::of(&request.frame);
        let (id, started) = (render.values.inputs().run_id, vc_template::rfc3339(render.values.inputs().started_ms));
        let run = json!({ "id": id, "profiles": render.values.profiles(), "startedAt": started });
        render.bind(Namespace::Run, run);
        render.roots.insert(Namespace::Secret, render.values.secrets());
        for (id, response) in request.frame.steps.iter() {
            render.roots.insert(Namespace::Step(*id), Arc::clone(response));
        }
        render.drawn = render.fake_vars();
        render.bind_instance(&request.frame)?;
        render.bind_iteration(&request.frame);
        Ok(render)
    }

    /// A renderer over `scope` that reads `dataset` and no other root: nothing saved, drawn or
    /// bound yet. The example rows render in one before the run starts.
    pub(super) fn over(shared: Shared<'a>, scope: Scope) -> Self {
        let Shared { values, program, redactor, schemas } = shared;
        let roots = HashMap::from([(Namespace::Dataset, values.datasets())]);
        let chance = RandomChance(values.inputs().random.as_ref());
        let (memo, drawn, reads) = (Saved::new(), Drawn::new(), Reads::default());
        Self { values, program, redactor, schemas, scope, roots, memo, drawn, reads, chance }
    }

    /// Runs `inner` with the levels of `around` put around the scope: an auth definition's vars
    /// or its use site's. Each fake var `around` brings is drawn once, apart from a same-named
    /// one of the call's, and those draws are returned with `inner`'s answer; afterwards the
    /// levels, memo and draws are as before.
    pub fn scoped<T>(&mut self, around: Around, inner: impl FnOnce(&Self) -> T) -> (T, Saved) {
        let fakes = self.brought_fakes(&around);
        let aside = self.set_aside(&fakes);
        self.scope.define(around.vars);
        self.scope.give(around.given);
        self.scope.fall_back(around.fallback);
        let out = inner(self);
        self.scope.drop_front(2);
        self.scope.drop_back(1);
        (out, self.restore(&fakes, aside))
    }

    /// The fake vars `around` brings: each of its `vars`, and each of its `fallback` that no
    /// level before it defines.
    fn brought_fakes(&self, around: &Around) -> Vec<String> {
        let fakes = |vars: &Defined| -> Vec<String> {
            vars.iter().filter(|(_, def)| is_fake(def)).map(|(name, _)| name.clone()).collect()
        };
        let hidden = |name: &String| {
            around.vars.contains_key(name)
                || around.given.contains_key(name)
                || self.scope.find(self.program, name, 0).is_some()
        };
        let mut names = fakes(&around.vars);
        names.extend(fakes(&around.fallback).into_iter().filter(|name| !hidden(name)));
        names
    }

    /// The value of `var.<name>` from the closest level.
    pub fn var_value(&self, name: &str) -> Result<Value, RenderError> {
        self.var(name, None)
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

    /// Every var in scope that renders, by name: the `vars` a hook gets.
    /// They are not reads of undeclared saved values (V5108): the hook may not use them.
    pub fn hook_vars(&self) -> Value {
        let rendered = self.scope.names().into_iter().filter_map(|name| {
            let value = self.var(&name, None).ok()?;
            Some((name, value))
        });
        self.reads.quietly(|| Value::Object(rendered.collect()))
    }

    /// A V5108 warning for each saved value this render read that a `function:` or `test:`
    /// action saved without declaring it in `provides` (D186).
    pub fn warnings(&self) -> Vec<Failure> {
        self.reads.warnings(self.program)
    }

    /// `names` as the action sees them, leaving out any that do not render and the fake ones,
    /// which differ every run: what keys a `cache:`d action.
    pub fn vars<'n>(&self, names: impl IntoIterator<Item = &'n String>) -> Saved {
        let plain = |name: &&String| !matches!(self.scope.find(self.program, name, 0), Some((_, Found::Defined(def))) if is_fake(def));
        names.into_iter().filter(plain).filter_map(|name| Some((name.clone(), self.var(name, None).ok()?))).collect()
    }

    /// The program the action belongs to.
    pub const fn program(&self) -> &'a Program {
        self.program
    }

    /// The run's values.
    pub const fn values(&self) -> &'a RunValues {
        self.values
    }

    /// The run's redactor, which masks check results before they leave the action.
    pub const fn redactor(&self) -> &'a Redactor {
        self.redactor
    }

    /// The run's compiled JSON Schemas, for `body.schema` checks (D201).
    pub const fn schemas(&self) -> &'a Schemas {
        self.schemas
    }

    /// The fake-data memo, for the frame to keep: the frame's draws and the ones made here.
    pub fn into_memo(self) -> Saved {
        let mut memo = self.memo;
        memo.extend(self.drawn.into_iter().filter_map(|(name, slot)| Some((name, slot.into_inner()?))));
        memo
    }

    /// An empty slot for each var whose closest definition is one `${fake.…}` the frame has not drawn.
    fn fake_vars(&self) -> Drawn {
        let fresh = self.scope.defined_names().into_iter().filter(|name| !self.memo.contains_key(name));
        let fakes = fresh.filter(
            |name| matches!(self.scope.find(self.program, name, 0), Some((_, Found::Defined(def))) if is_fake(def)),
        );
        fakes.map(|name| (name, OnceCell::new())).collect()
    }

    /// A fake var's value: the frame's draw, this render's, or a new draw kept for the next read.
    fn fake_var(&self, def: &TemplateValue, within: &Within<'_>) -> Result<Value, RenderError> {
        if let Some(value) = self.memo.get(within.name) {
            return Ok(value.clone());
        }
        let Some(slot) = self.drawn.get(within.name) else { return self.defined(def, within) };
        if let Some(value) = slot.get() {
            return Ok(value.clone());
        }
        let value = self.defined(def, within)?;
        Ok(slot.get_or_init(|| value).clone())
    }

    /// Takes out the memo and draws of `names`, giving each a fresh slot.
    fn set_aside(&mut self, names: &[String]) -> Aside {
        let memo = names.iter().filter_map(|name| Some((name.clone(), self.memo.remove(name)?))).collect();
        let drawn =
            names.iter().filter_map(|name| Some((name.clone(), self.drawn.insert(name.clone(), OnceCell::new())?)));
        Aside { memo, drawn: drawn.collect() }
    }

    /// Drops the slots of `names`, puts back what [`Self::set_aside`] took out, and returns
    /// what the dropped slots drew.
    fn restore(&mut self, names: &[String], aside: Aside) -> Saved {
        let drawn = names.iter().filter_map(|name| Some((name.clone(), self.drawn.remove(name)?.into_inner()?)));
        let drawn = drawn.collect();
        self.drawn.extend(aside.drawn);
        self.memo.extend(aside.memo);
        drawn
    }

    /// Binds `iteration` and the instance's `example` row.
    fn bind_instance(&mut self, frame: &Frame) -> Result<(), RenderError> {
        let Some(instance) = &frame.instance else { return Ok(()) };
        let Some(test) = self.program.tests.get(&instance.test) else { return Ok(()) };
        let index = u64::from(instance.repeat_index) + 1;
        self.bind(Namespace::Iteration, json!({ "index": index, "count": test.repeat }));
        let Some(row) = instance.example_row else { return Ok(()) };
        let value = self.example(test, usize::try_from(row).unwrap_or(usize::MAX))?;
        self.bind(Namespace::Example, value);
        Ok(())
    }

    /// Binds the loop turn the frame runs in, if any: `iteration`, and `item` in a `forEach`. A
    /// loop's `iteration` hides a repeated test's.
    fn bind_iteration(&mut self, frame: &Frame) {
        let Some(turn) = &frame.iteration else { return };
        self.bind(Namespace::Iteration, json!({ "index": turn.index, "count": turn.count }));
        if let Some(item) = &turn.item {
            self.bind(Namespace::Item, item.clone());
        }
    }

    /// The value of `var.<name>`, looking from the closest level unless `within` is the
    /// definition of the same name, which continues past its own level.
    pub(super) fn var(&self, name: &str, within: Option<&Within<'_>>) -> Result<Value, RenderError> {
        let (from, depth) = within.map_or((0, 0), |within| (within.after(name), within.depth));
        if depth > MAX_DEPTH {
            let message =
                format!("`var.{name}` goes through more than {MAX_DEPTH} vars; do some vars refer to each other?");
            return Err(RenderError::invalid(message));
        }
        let found = self.scope.find(self.program, name, from);
        let (level, found) = found.ok_or_else(|| RenderError::missing(format!("`var.{name}` is not set")))?;
        if self.scope.is_saved(level) {
            self.reads.note(name);
        }
        let within = Within { name, level, depth: depth + 1 };
        match found {
            Found::Value(value) => Ok(value),
            Found::Defined(def) if is_fake(def) => self.fake_var(def, &within),
            Found::Defined(def) => self.defined(def, &within),
        }
    }

    fn defined(&self, def: &TemplateValue, within: &Within<'_>) -> Result<Value, RenderError> {
        Renderer::new(self.values.pipes(), &Lookup { render: self, within: Some(within) }).value(def)
    }

    /// The run's random source, for the pipes.
    pub(super) fn chance(&self) -> &dyn Chance {
        &self.chance
    }

    pub(super) fn label(&self, ns: Namespace) -> String {
        match ns {
            Namespace::Step(id) => self.program.str(id).unwrap_or("step").to_owned(),
            other => vc_template::root_name(other).unwrap_or("root").to_owned(),
        }
    }
}

/// A var defined as exactly one `${fake.…}` is generated once per instance (D131).
fn is_fake(def: &TemplateValue) -> bool {
    let TemplateValue::String(template) = def else { return false };
    template.single_ref().is_some_and(|reference| reference.ns == Namespace::Fake)
}

/// The levels [`Render::scoped`] puts around the scope: `vars` defined and `given` over them in
/// front of every other level, and `fallback` defined behind them all.
#[derive(Default)]
pub struct Around {
    /// Definitions found first, which may read each other.
    pub vars: Arc<Defined>,
    /// Values found before even `vars`.
    pub given: Arc<Saved>,
    /// Definitions found only when no other level has the name.
    pub fallback: Arc<Defined>,
}

/// The memo and draws a scoped level hides.
struct Aside {
    memo: Saved,
    drawn: Drawn,
}

/// What every render of a run reads besides its scope.
#[derive(Clone, Copy)]
pub(super) struct Shared<'a> {
    pub values: &'a RunValues,
    pub program: &'a Program,
    pub redactor: &'a Redactor,
    pub schemas: &'a Schemas,
}

/// The var definition being rendered.
pub(super) struct Within<'n> {
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
    fn next_u64(&self) -> u64 {
        self.0.next_u64()
    }
}
