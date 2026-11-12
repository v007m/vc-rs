//! The secrets a run reads (D280): each `${secret.X}` its walk reaches, so the check before a
//! run asks a value only of those. The walk follows what the walker follows: the run-wide vars,
//! headers and the active profiles, then `beforeRun`, each file's lifecycle blocks, the selected
//! tests and what they call or reuse (apis, api files, auth and its steps, cleanups), and
//! `afterRun`. Code can read any secret through `secret()`, so a walk that reaches a function
//! action, a request hook or a function check, or a project with a plugin or a redact function,
//! reads every declared secret (D302).

use std::collections::BTreeSet;

use vc_model::{ActionId, Namespace, Plan, ProfileId, Program, RedactRule, Reference, Segment};

use crate::bodies::walk;
use crate::visit::{Visit, Walker};

/// Collects the name of every `${secret.X}` the walk reaches, and whether it reaches code.
#[derive(Default)]
struct Read {
    names: BTreeSet<String>,
    code: bool,
}

impl Visit for Read {
    fn reference(&mut self, reference: &Reference) {
        if reference.ns == Namespace::Secret
            && let Some(Segment::Key(name)) = reference.path.segments.first()
        {
            self.names.insert(name.clone());
        }
    }

    fn code(&mut self) {
        self.code = true;
    }
}

impl Read {
    /// The names read: every declared one as well when the walk reached code or `program` runs
    /// code of its own.
    fn of(mut self, program: &Program) -> BTreeSet<String> {
        if self.code || runs_code(program) {
            self.names.extend(program.config.secrets.declared.iter().cloned());
        }
        self.names
    }
}

/// Whether `program` runs code on every run: a plugin, or a redact rule's function.
fn runs_code(program: &Program) -> bool {
    let function = |rule: &RedactRule| matches!(rule, RedactRule::Function(_));
    !program.config.plugins.is_empty() || program.config.redact_rules.iter().any(function)
}

/// The secrets a run of `plan` under `profiles` reads: the run-wide templates and everything
/// `plan` runs.
#[must_use]
pub fn secrets(program: &Program, plan: &Plan, profiles: &[ProfileId]) -> BTreeSet<String> {
    let mut walker = Walker::new(program, Read::default()).with_profiles(profiles);
    walker.run_scope();
    walk(&mut walker, program, plan);
    walker.into_visitor().of(program)
}

/// The secrets `actions` read under `profiles`, with the run-wide templates: for an ad hoc call
/// or a cleanup.
#[must_use]
pub fn secrets_from(program: &Program, actions: &[ActionId], profiles: &[ProfileId]) -> BTreeSet<String> {
    let mut walker = Walker::new(program, Read::default()).with_profiles(profiles);
    walker.run_scope();
    walker.actions(actions);
    walker.into_visitor().of(program)
}

/// The secrets the alert channels' settings read, which a run's end renders.
#[must_use]
pub fn alert_secrets(program: &Program) -> BTreeSet<String> {
    let mut walker = Walker::new(program, Read::default());
    walker.alerts();
    walker.into_visitor().names
}
