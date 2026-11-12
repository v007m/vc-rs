//! The teardown blocks a run's reports name when an action in one fails (D279, D304): each
//! file's `afterAll` and the run's `afterRun`.

use std::collections::HashMap;

use vc_model::{ActionId, Names, OwnerKey, Plan};

use crate::collect::StepRecord;
use crate::view::TeardownView;

/// The block each teardown action of a plan belongs to: `afterAll` with its file, or `afterRun`.
#[derive(Debug, Default)]
pub struct Teardown {
    blocks: HashMap<(OwnerKey, ActionId), (&'static str, Option<String>)>,
}

impl Teardown {
    /// The `afterAll` and `afterRun` actions of `plan`, each with its block.
    pub fn new(plan: &Plan, names: &Names) -> Self {
        let mut blocks = HashMap::new();
        for file in &plan.files {
            let block = ("afterAll", Some(names.file(file.id).to_owned()));
            blocks.extend(file.after_all.iter().map(|action| ((OwnerKey::File(file.id), *action), block.clone())));
        }
        blocks.extend(plan.after_run.iter().map(|action| ((OwnerKey::Run, *action), ("afterRun", None))));
        Self { blocks }
    }

    /// The blocks with a failed action among `lifecycle`, each once, in the order they failed,
    /// with the (masked) message of the first action that failed in it.
    pub fn failures(&self, lifecycle: &[StepRecord]) -> Vec<TeardownView> {
        let mut failed: Vec<TeardownView> = Vec::new();
        for step in lifecycle {
            let (Some(failure), Some((block, file))) =
                (step.outcome.failure(), self.blocks.get(&(step.owner, step.action)))
            else {
                continue;
            };
            if !failed.iter().any(|seen| seen.block == *block && seen.file == *file) {
                let message = failure.message.clone();
                failed.push(TeardownView { block: (*block).to_owned(), file: file.clone(), message });
            }
        }
        failed
    }

    /// The names of the blocks [`Teardown::failures`] gives: `afterAll in users.crud`, `afterRun`.
    pub fn failed(&self, lifecycle: &[StepRecord]) -> Vec<String> {
        self.failures(lifecycle).iter().map(TeardownView::label).collect()
    }
}

#[cfg(test)]
#[path = "teardown_tests.rs"]
mod tests;
