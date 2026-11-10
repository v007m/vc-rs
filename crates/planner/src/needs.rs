//! Needs analysis (D122): which shared `beforeRun` and `beforeAll` actions something reads, found
//! by visiting its `${var.<actionId>.…}` references.

use std::collections::{BTreeSet, HashMap};

use vc_model::{ActionId, Namespace, Needs, PlannedAction, Program, Reference, Segment, Test};

use crate::visit::{Visit, Walker};

/// The shared actions in scope, by the `id` their saved values are read under. A file's
/// `beforeAll` names shadow the run's `beforeRun` names.
#[derive(Clone, Debug, Default)]
pub struct Shared {
    names: HashMap<String, ActionId>,
}

impl Shared {
    /// Plans `actions` in order: each needs the shared actions before it that it reads, and is
    /// then in scope for those after it.
    pub fn plan(&mut self, program: &Program, actions: &[ActionId]) -> Vec<PlannedAction> {
        let mut planned = Vec::with_capacity(actions.len());
        for &action in actions {
            let mut walker = Walker::new(program, Reads::new(self));
            walker.action(action);
            planned.push(PlannedAction { action, needs: walker.into_visitor().needs() });
            self.add(program, action);
        }
        planned
    }

    /// What an instance of `test` reads.
    pub fn instance(&self, program: &Program, test: &Test) -> Needs {
        let mut walker = Walker::new(program, Reads::new(self));
        walker.run_scope();
        walker.instance(test);
        walker.into_visitor().needs()
    }

    fn add(&mut self, program: &Program, action: ActionId) {
        let name = usize::try_from(action.index())
            .ok()
            .and_then(|at| program.actions.get(at))
            .and_then(|action| action.id)
            .and_then(|sym| program.str(sym));
        if let Some(name) = name {
            self.names.insert(name.to_owned(), action);
        }
    }
}

/// Collects the shared actions whose names `var` references start with.
struct Reads<'s> {
    shared: &'s Shared,
    found: BTreeSet<ActionId>,
}

impl<'s> Reads<'s> {
    const fn new(shared: &'s Shared) -> Self {
        Self { shared, found: BTreeSet::new() }
    }

    fn needs(self) -> Needs {
        self.found.into_iter().collect()
    }
}

impl Visit for Reads<'_> {
    fn reference(&mut self, reference: &Reference) {
        if reference.ns != Namespace::Var {
            return;
        }
        let Some(Segment::Key(name)) = reference.path.segments.first() else { return };
        if let Some(action) = self.shared.names.get(name) {
            self.found.insert(*action);
        }
    }
}
