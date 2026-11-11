//! The levels a `${var.x}` lookup walks, closest first (syntax reference "Vars and profiles").

use std::sync::Arc;

use serde_json::Value;
use smallvec::SmallVec;
use vc_model::{Action, ActionKind, FileId, OwnerKey, Program, TemplateValue, Test};

use super::run::{Defined, RunValues};
use crate::{ActionRequest, Frame, Saved, SharedSlots};

/// One level of the scope.
#[derive(Debug)]
pub enum Level {
    /// Vars on the call: the action's, a reusing action's, a test's.
    Call(Arc<Defined>),
    /// A cleanup's vars, rendered when its call registered it; they stand in for its own.
    Given(Arc<Saved>),
    /// Values saved earlier in this test instance.
    Saved(Arc<Saved>),
    /// The file's `beforeAll` saves, under each action's id.
    File(Arc<SharedSlots>),
    /// The test file's vars.
    TestFile(Arc<Defined>),
    /// The run's `beforeRun` saves, under each action's id.
    Run(Arc<SharedSlots>),
    /// The api's own vars.
    Api(Arc<Defined>),
    /// The api file's vars.
    ApiFile(Arc<Defined>),
    /// The config's vars.
    Config(Arc<Defined>),
}

/// What a level holds for a name: a value, or a definition still to render.
#[derive(Debug, PartialEq, Eq)]
pub enum Found<'s> {
    /// A value saved or already rendered.
    Value(Value),
    /// A var's definition.
    Defined(&'s TemplateValue),
}

impl Level {
    fn get<'s>(&'s self, program: &Program, name: &str) -> Option<Found<'s>> {
        match self {
            Self::Call(defined)
            | Self::TestFile(defined)
            | Self::Api(defined)
            | Self::ApiFile(defined)
            | Self::Config(defined) => defined.get(name).map(Found::Defined),
            Self::Given(saved) | Self::Saved(saved) => saved.get(name).cloned().map(Found::Value),
            Self::File(slots) | Self::Run(slots) => published(program, slots, name).map(Found::Value),
        }
    }

    fn defined(&self) -> Option<&Defined> {
        match self {
            Self::Call(defined)
            | Self::TestFile(defined)
            | Self::Api(defined)
            | Self::ApiFile(defined)
            | Self::Config(defined) => Some(defined),
            Self::Given(_) | Self::Saved(_) | Self::File(_) | Self::Run(_) => None,
        }
    }

    fn saved(&self) -> Option<&Saved> {
        match self {
            Self::Given(saved) | Self::Saved(saved) => Some(saved),
            Self::Call(_)
            | Self::TestFile(_)
            | Self::Api(_)
            | Self::ApiFile(_)
            | Self::Config(_)
            | Self::File(_)
            | Self::Run(_) => None,
        }
    }
}

/// The saves of the shared action whose id is `name`, as one object.
fn published(program: &Program, slots: &SharedSlots, name: &str) -> Option<Value> {
    let named = |action: &Action| action.id.and_then(|id| program.str(id)) == Some(name);
    slots.passed().find_map(|(id, saved)| {
        let action = program.actions.get(usize::try_from(id.index()).ok()?)?;
        named(action).then(|| Value::Object(Saved::clone(&saved)))
    })
}

/// The levels of one action's lookups, closest first.
#[derive(Debug, Default)]
pub struct Scope {
    levels: SmallVec<[Level; 10]>,
}

impl Scope {
    /// The scope `request`'s action renders in.
    pub fn of(request: &ActionRequest) -> Self {
        let (cx, frame) = (&request.cx, &request.frame);
        let values = &cx.values;
        let action = cx.action(request.action);
        let mut scope = Self::default();
        let own = || values.action(request.action).map(Level::Call);
        scope.push(frame.given.as_ref().map_or_else(own, |given| Some(Level::Given(Arc::clone(given)))));
        scope.levels.extend(frame.callers.iter().rev().cloned().map(Level::Call));
        scope.push(frame.instance.as_ref().and_then(|instance| values.test(instance.test)).map(Level::Call));
        scope.levels.push(Level::Saved(Arc::clone(&frame.saved)));
        scope.levels.push(Level::File(Arc::clone(&frame.file)));
        scope.push(
            test_file(&cx.program, request.owner, frame).and_then(|file| values.test_file(file)).map(Level::TestFile),
        );
        scope.levels.push(Level::Run(Arc::clone(&frame.run)));
        scope.api_levels(values, &cx.program, action);
        scope.levels.push(Level::Config(values.config()));
        scope
    }

    /// The scope `test`'s example rows render in before the run: the test's vars, its file's and
    /// the config's.
    pub fn of_test(values: &RunValues, test: &Test) -> Self {
        let mut scope = Self::default();
        scope.push(values.test(test.id).map(Level::Call));
        scope.push(values.test_file(test.file).map(Level::TestFile));
        scope.levels.push(Level::Config(values.config()));
        scope
    }

    fn api_levels(&mut self, values: &RunValues, program: &Program, action: Option<&Action>) {
        let Some(ActionKind::Call(api)) = action.map(|action| &action.kind) else { return };
        self.push(values.api(*api).map(Level::Api));
        let group = program.apis.get(api).map(|api| api.group);
        self.push(group.and_then(|group| values.api_file(group)).map(Level::ApiFile));
    }

    /// Puts `vars` in front of every other level, as definitions that may read each other.
    pub fn define(&mut self, vars: Arc<Defined>) {
        self.levels.insert(0, Level::Call(vars));
    }

    /// Puts `given` in front of every other level.
    pub fn give(&mut self, given: Arc<Saved>) {
        self.levels.insert(0, Level::Given(given));
    }

    /// Puts `vars` behind every other level, as definitions found only when no other level has
    /// the name.
    pub fn fall_back(&mut self, vars: Arc<Defined>) {
        self.levels.push(Level::Call(vars));
    }

    /// Removes the `count` furthest levels.
    pub fn drop_back(&mut self, count: usize) {
        self.levels.truncate(self.levels.len().saturating_sub(count));
    }

    /// Removes the `count` closest levels.
    pub fn drop_front(&mut self, count: usize) {
        self.levels.drain(..count.min(self.levels.len()));
    }

    fn push(&mut self, level: Option<Level>) {
        self.levels.extend(level);
    }

    /// Every name a level defines (not saved values), sorted, once each.
    pub fn defined_names(&self) -> Vec<String> {
        let mut names: Vec<String> =
            self.levels.iter().filter_map(Level::defined).flat_map(|defined| defined.keys().cloned()).collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Every name a level defines or holds a saved value for, sorted, once each.
    pub fn names(&self) -> Vec<String> {
        let mut names = self.defined_names();
        names.extend(self.levels.iter().filter_map(Level::saved).flat_map(|saved| saved.keys().cloned()));
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Whether the level at `at` holds the values saved earlier in the test instance.
    pub fn is_saved(&self, at: usize) -> bool {
        matches!(self.levels.get(at), Some(Level::Saved(_)))
    }

    /// The first level from `from` on that has `name`, with its position.
    pub fn find(&self, program: &Program, name: &str, from: usize) -> Option<(usize, Found<'_>)> {
        let mut levels = self.levels.iter().enumerate().skip(from);
        levels.find_map(|(at, level)| level.get(program, name).map(|found| (at, found)))
    }
}

/// The test file whose vars apply: a reused test's, the file of a `beforeAll`, or the
/// instance's test's file.
fn test_file(program: &Program, owner: OwnerKey, frame: &Frame) -> Option<FileId> {
    if frame.test_file.is_some() {
        return frame.test_file;
    }
    match owner {
        OwnerKey::File(file) => Some(file),
        OwnerKey::Instance(_) => {
            frame.instance.as_ref().and_then(|instance| program.tests.get(&instance.test)).map(|test| test.file)
        }
        OwnerKey::Run => None,
    }
}
