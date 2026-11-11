//! Whether a plan generates data from the run's seed (D131): a `${fake.…}` reference, or a pipe
//! that draws (`random`, `shuffle`), in the run's vars or anything the plan runs. Only such a
//! run's seed is worth printing with a failure.

use vc_model::{Namespace, Plan, Program, Reference};

use crate::bodies;
use crate::visit::{Visit, Walker};

/// Notes whether any reference the walk reaches generates data.
struct Draws<F> {
    draws: F,
    found: bool,
}

impl<F: Fn(&str) -> bool> Visit for Draws<F> {
    fn reference(&mut self, reference: &Reference) {
        let piped = reference.pipes.iter().any(|pipe| (self.draws)(&pipe.name));
        self.found |= reference.ns == Namespace::Fake || piped;
    }
}

/// Whether running `plan` generates data from the run's seed: a `${fake.…}`, or a pipe that
/// `draws` names as drawing from it, in the run's vars, profiles or anything the plan runs.
#[must_use]
pub fn generates(program: &Program, plan: &Plan, draws: impl Fn(&str) -> bool) -> bool {
    let mut walker = Walker::new(program, Draws { draws, found: false });
    walker.run_scope();
    bodies::walk(&mut walker, program, plan);
    walker.into_visitor().found
}

#[cfg(test)]
#[path = "draws_tests.rs"]
mod tests;
