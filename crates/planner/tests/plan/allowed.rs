//! `allowFailure` (D248): which planned tests may fail without failing the run.

use vc_model::{AllowFailure, Plan, Program};
use vc_planner::{PlanOptions, Selection};

use crate::common;

/// A file allowed to fail until 2026-12-01, holding a test with its own reason, one with
/// `true`, one whose own date has passed, and a skipped one.
const FLAKY: &str = "name: Flaky
allowFailure: {reason: upstream sandbox, until: 2026-12-01}
tests:
  - id: listsOwn
    call: users#list
    allowFailure:
      reason: see 212
  - id: listsAny
    call: users#list
    allowFailure: true
  - id: listsLapsed
    call: users#list
    allowFailure: {reason: old, until: 2026-11-02}
  - id: listsSkipped
    call: users#list
    skip: down
";

/// A file with no `allowFailure`, holding one test that has it.
const OWN: &str = "name: Own
tests:
  - id: listsMine
    call: users#list
    allowFailure: true
  - id: listsStrictly
    call: users#list
";

fn program() -> Program {
    common::project_with(&[("services/users/flaky.test.yaml", FLAKY), ("services/users/own.test.yaml", OWN)])
}

/// Each allowed test by short ID, with its reason.
fn allowed(program: &Program, plan: &Plan) -> Vec<(String, Option<String>)> {
    let short = |id| common::name(program, id).rsplit('#').next().unwrap_or_default().to_owned();
    plan.allowed_failures.iter().map(|(test, allow)| (short(test.sym()), allow.reason.clone())).collect()
}

fn plan(program: &Program, change: impl FnOnce(&mut PlanOptions)) -> Plan {
    let mut options = common::today();
    change(&mut options);
    vc_planner::plan(program, &common::ids(&["users.flaky", "users.own"]), &options).unwrap()
}

#[test]
fn a_test_s_own_allow_failure_wins_and_its_file_s_covers_the_rest() {
    let program = program();
    let mut found = allowed(&program, &plan(&program, |_| {}));
    found.sort();
    let reason = |text: &str| Some(text.to_owned());
    assert_eq!(
        found,
        [
            ("listsAny".to_owned(), None),
            ("listsLapsed".to_owned(), reason("upstream sandbox")),
            ("listsMine".to_owned(), None),
            ("listsOwn".to_owned(), reason("see 212")),
        ],
        "a lapsed own date falls back to the file's; a skipped test and one with none are not allowed"
    );
}

#[test]
fn after_the_file_s_until_only_the_undated_settings_hold() {
    let program = program();
    let later = plan(&program, |options| options.today = Some("2026-12-02".into()));
    let mut found = allowed(&program, &later);
    found.sort();
    assert_eq!(
        found,
        [
            ("listsAny".to_owned(), None),
            ("listsMine".to_owned(), None),
            ("listsOwn".to_owned(), Some("see 212".into()))
        ]
    );
    let on_the_day = plan(&program, |options| options.today = Some("2026-12-01".into()));
    assert_eq!(on_the_day.allowed_failures.len(), 4, "`until` is the last day it holds");
}

#[test]
fn strict_allows_no_failure() {
    let program = program();
    let strict = plan(&program, |options| options.strict = true);
    assert!(strict.allowed_failures.is_empty(), "{:?}", strict.allowed_failures);
    assert_eq!(common::tests(&program, &strict).len(), 5, "--strict still runs every test");
}

#[test]
fn the_compiled_setting_keeps_its_reason_and_date() {
    let program = program();
    let file = program.test_files.values().find(|file| file.allow_failure.is_some()).unwrap();
    let expected = AllowFailure { reason: Some("upstream sandbox".into()), until: Some("2026-12-01".into()) };
    assert_eq!(file.allow_failure.as_ref(), Some(&expected));
    let every = vc_planner::plan(&program, &Selection::default(), &common::today()).unwrap();
    assert_eq!(every.allowed_failures.len(), 4, "no other file of the sample allows a failure");
}
