use serde_json::json;
use vc_model::{
    ActionId, ActionOutcome, CheckResult, CleanupRecord, Failure, InstanceId, Origin, OwnerKey, Pos, RunStatus,
    SourceFileId, SourceSpan, TestId, Totals,
};

use super::*;
use crate::collect::StepRecord;
use crate::names::Names;
use crate::testing;

fn sample() -> (Names, TestId, ActionId) {
    let (program, plan) = testing::project();
    let test = *program.tests.keys().next().unwrap();
    let seed = plan.files[0].before_all[0].action;
    (Names::new(program, &plan), test, seed)
}

fn failing(test: TestId) -> CheckResult {
    let mut check = testing::sample_check(test);
    check.passed = false;
    check.actual = Some(json!(500));
    check.code = Some("V5101".into());
    check
}

#[test]
fn encodes_owners_and_origins() {
    let (names, test, seed) = sample();
    let file = names.program().tests[&test].file;
    assert_eq!(owner(&names, OwnerKey::Run), json!({"kind": "run"}));
    assert_eq!(owner(&names, OwnerKey::File(file)), json!({"kind": "file", "file": "users.crud"}));
    assert_eq!(owner(&names, OwnerKey::Instance(InstanceId::new(2))), json!({"kind": "instance", "instance": 2}));
    let api = *names.program().apis.keys().next().unwrap();
    assert_eq!(origin(&names, Origin::Api(api)), "api users#get");
    assert_eq!(origin(&names, Origin::Test(test)), "test users.crud#getsUser");
    assert_eq!(origin(&names, Origin::Action(seed)), "action seed");
}

#[test]
fn encodes_checks_with_where_they_were_written() {
    let (names, test, _) = sample();
    let encoded = check(&names, &failing(test));
    let location = json!({"path": "services/users/crud.test.yaml", "line": 1, "column": 1});
    let expected = json!({
        "key": "status", "passed": false, "expected": 200, "actual": 500, "diff": null, "error": null,
        "code": "V5101", "origin": "test users.crud#getsUser", "location": location,
    });
    assert_eq!(encoded, expected);
    let nowhere = SourceSpan::new(SourceFileId::new(40), Pos::new(1, 1, 0), Pos::new(1, 1, 0));
    assert_eq!(super::location(names.location(nowhere)), serde_json::Value::Null);
}

#[test]
fn encodes_failures_outcomes_steps_and_cleanups() {
    let (names, test, seed) = sample();
    let failed = Failure::new("410 gone").with_code("V5107").at(seed);
    let encoded = json!({"message": "410 gone", "code": "V5107", "action": "seed"});
    assert_eq!(failure(&names, &failed), encoded);
    assert_eq!(failure(&names, &Failure::new("x")), json!({"message": "x", "code": null, "action": null}));
    assert_eq!(outcome(&names, &ActionOutcome::Passed), ("passed", serde_json::Value::Null));
    let outcome_failed = ActionOutcome::Failed(failed);
    assert_eq!(outcome(&names, &outcome_failed), ("failed", encoded.clone()));
    let checks = vec![failing(test)];
    let record = StepRecord { action: seed, owner: OwnerKey::Run, outcome: outcome_failed.clone(), checks };
    let stepped = step(&names, &record);
    assert_eq!(stepped["action"], "seed");
    assert_eq!(stepped["owner"], json!({"kind": "run"}));
    assert_eq!((&stepped["status"], &stepped["failure"]), (&json!("failed"), &encoded));
    assert_eq!(stepped["checks"][0]["key"], "status");
    let record = CleanupRecord { action: seed, owner: OwnerKey::Run, outcome: outcome_failed };
    let cleaned = json!({"action": "seed", "owner": {"kind": "run"}, "status": "failed", "failure": encoded});
    assert_eq!(cleanup(&names, &record), cleaned);
}

#[test]
fn encodes_totals_and_run_statuses() {
    let totals_in = Totals { passed: 1, failed: 2, blocked: 3, flaky: 4, skipped: 5 };
    let expected = json!({"passed": 1, "failed": 2, "blocked": 3, "flaky": 4, "skipped": 5});
    assert_eq!(totals(totals_in), expected);
    assert_eq!(run_status(RunStatus::Passed), "passed");
    assert_eq!(run_status(RunStatus::Failed), "failed");
    assert_eq!(run_status(RunStatus::Cancelled), "cancelled");
}
