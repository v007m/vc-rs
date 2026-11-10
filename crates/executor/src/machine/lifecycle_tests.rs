use vc_model::{OwnerKey, Plan};

use super::super::sim;
use super::{LifecycleStep, LoadLifecycle};

/// `beforeRun` 1, a file with `beforeAll` 2 and 3 and `afterAll` 4, `afterRun` 5.
fn plan() -> (Plan, OwnerKey) {
    let file = (*sim::planned_file(vec![sim::planned(2, &[]), sim::planned(3, &[2])], vec![], &[4])).clone();
    let owner = OwnerKey::File(file.id);
    let plan = Plan {
        before_run: vec![sim::planned(1, &[])],
        files: vec![file],
        after_run: sim::acts(&[5]),
        skipped: Vec::new(),
    };
    (plan, owner)
}

#[test]
fn shared_actions_run_before_run_first_and_after_run_last_each_owner_cleaned_after_its_own() {
    let (plan, file) = plan();
    let lifecycle = LoadLifecycle::new(&plan);
    let run = OwnerKey::Run;
    let act = |owner, index| LifecycleStep::Act(owner, sim::act(index));
    assert_eq!(lifecycle.prepare, [(run, sim::act(1)), (file, sim::act(2)), (file, sim::act(3))]);
    let finish = [act(file, 4), LifecycleStep::Clean(file), act(run, 5), LifecycleStep::Clean(run)];
    assert_eq!(lifecycle.finish, finish);
    assert_eq!(lifecycle.unwind().collect::<Vec<_>>(), [LifecycleStep::Clean(file), LifecycleStep::Clean(run)]);
}

#[test]
fn once_cancelled_only_the_cleanups_run() {
    let (act, clean) = (LifecycleStep::Act(OwnerKey::Run, sim::act(1)), LifecycleStep::Clean(OwnerKey::Run));
    assert!(act.runs(false) && clean.runs(false));
    assert!(!act.runs(true) && clean.runs(true));
}
