use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::sync::Arc;

use vc_model::{ActionId, Lifecycle, Plan, Program, TestId};

use crate::{ExecutorError, InstanceScript};

/// Each planned test's actions, built once per run and shared by its instances.
pub type Scripts = BTreeMap<TestId, Arc<InstanceScript>>;

/// The script of every test the plan runs.
pub fn scripts(program: &Program, plan: &Plan) -> Result<Scripts, ExecutorError> {
    let mut scripts = Scripts::new();
    for instance in plan.files.iter().flat_map(|file| &file.instances) {
        if let Entry::Vacant(entry) = scripts.entry(instance.test) {
            entry.insert(Arc::new(script(program, plan, instance.test)?));
        }
    }
    Ok(scripts)
}

/// `beforeEach` and `before`, then the call or steps, then `after` and `afterEach`; the test's
/// `retry`, else `defaults.retry` (D126); and the `allowFailure` the plan holds for it (D248).
fn script(program: &Program, plan: &Plan, id: TestId) -> Result<InstanceScript, ExecutorError> {
    let test = program.tests.get(&id).ok_or(ExecutorError::UnknownTest(id))?;
    let lifecycle = program.test_files.get(&test.file).map(|file| &file.lifecycle);
    let each = |pick: fn(&Lifecycle) -> &Vec<ActionId>| lifecycle.map(pick).cloned().unwrap_or_default();
    let before = each(|blocks| &blocks.before_each).into_iter().chain(test.before.iter().copied()).collect();
    let after = test.after.iter().copied().chain(each(|blocks| &blocks.after_each)).collect();
    let retry = test.retry.or(program.config.defaults.retry);
    let allow_failure = plan.allowed_failures.get(&id).cloned();
    Ok(InstanceScript { before, body: crate::body_actions(&test.body), after, retry, allow_failure })
}

/// Every action the run starts directly: the shared lifecycle and each script.
pub fn roots(plan: &Plan, scripts: &Scripts) -> Vec<ActionId> {
    let shared = |planned: &[vc_model::PlannedAction]| planned.iter().map(|planned| planned.action).collect::<Vec<_>>();
    let run = shared(&plan.before_run).into_iter().chain(plan.after_run.iter().copied());
    let files = plan.files.iter().flat_map(|file| shared(&file.before_all).into_iter().chain(file.after_all.clone()));
    let tests =
        scripts.values().flat_map(|script| script.before.iter().chain(&script.body).chain(&script.after).copied());
    run.chain(files).chain(tests).collect()
}
