//! The secrets a run reads (D280): each `${secret.X}` its walk reaches, so the check before a
//! run asks a value only of those. The walk follows what the walker follows: the run-wide vars,
//! headers and profiles, then `beforeRun`, each file's lifecycle blocks, the selected tests and
//! what they call or reuse (apis, api files, auth and its steps, cleanups), and `afterRun`.

use std::collections::BTreeSet;

use vc_model::{ActionId, Namespace, Plan, Program, Reference, Segment};

use crate::bodies::walk;
use crate::visit::{Visit, Walker};

/// Collects the name of every `${secret.X}` the walk reaches.
#[derive(Default)]
struct Read(BTreeSet<String>);

impl Visit for Read {
    fn reference(&mut self, reference: &Reference) {
        if reference.ns == Namespace::Secret
            && let Some(Segment::Key(name)) = reference.path.segments.first()
        {
            self.0.insert(name.clone());
        }
    }
}

/// The secrets a run of `plan` reads: the run-wide templates and everything `plan` runs.
#[must_use]
pub fn secrets(program: &Program, plan: &Plan) -> BTreeSet<String> {
    let mut walker = Walker::new(program, Read::default());
    walker.run_scope();
    walk(&mut walker, program, plan);
    walker.into_visitor().0
}

/// The secrets `actions` read, with the run-wide templates: for an ad hoc call or a cleanup.
#[must_use]
pub fn secrets_from(program: &Program, actions: &[ActionId]) -> BTreeSet<String> {
    let mut walker = Walker::new(program, Read::default());
    walker.run_scope();
    walker.actions(actions);
    walker.into_visitor().0
}

/// The secrets the alert channels' settings read, which a run's end renders.
#[must_use]
pub fn alert_secrets(program: &Program) -> BTreeSet<String> {
    let mut walker = Walker::new(program, Read::default());
    walker.alerts();
    walker.into_visitor().0
}
