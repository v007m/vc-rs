//! The teardown blocks a run's summary names when an action in one fails (D279): each file's
//! `afterAll` and the run's `afterRun`.

use std::collections::HashMap;

use vc_model::{ActionId, Names, OwnerKey, Plan};

use crate::collect::StepRecord;

/// The block each teardown action of a plan belongs to.
#[derive(Debug, Default)]
pub struct Teardown {
    blocks: HashMap<(OwnerKey, ActionId), String>,
}

impl Teardown {
    /// The `afterAll` and `afterRun` actions of `plan`, each with its block's name.
    pub fn new(plan: &Plan, names: &Names) -> Self {
        let mut blocks = HashMap::new();
        for file in &plan.files {
            let block = format!("afterAll in {}", names.file(file.id));
            blocks.extend(file.after_all.iter().map(|action| ((OwnerKey::File(file.id), *action), block.clone())));
        }
        blocks.extend(plan.after_run.iter().map(|action| ((OwnerKey::Run, *action), "afterRun".to_owned())));
        Self { blocks }
    }

    /// The blocks with a failed action among `lifecycle`, each once, in the order they failed.
    pub fn failed(&self, lifecycle: &[StepRecord]) -> Vec<&str> {
        let mut failed: Vec<&str> = Vec::new();
        let failures = lifecycle.iter().filter(|step| step.outcome.failure().is_some());
        for block in failures.filter_map(|step| self.blocks.get(&(step.owner, step.action))) {
            if !failed.contains(&block.as_str()) {
                failed.push(block);
            }
        }
        failed
    }
}
