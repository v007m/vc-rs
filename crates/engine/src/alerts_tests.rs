use vc_interfaces::{NoticeFailure, RunSummary};
use vc_model::{Alert, AlertTrigger, RunStatus, Tag, TestStatus, Totals};

use super::{Ended, Finished, decide};

const ALL: &[AlertTrigger] = &AlertTrigger::ALL;

fn rule(on: &[AlertTrigger], profiles: &[&str], tags: &[&str]) -> Alert {
    Alert {
        on: on.to_vec(),
        profiles: profiles.iter().map(|profile| (*profile).to_owned()).collect(),
        tags: tags.iter().map(|tag| Tag((*tag).to_owned())).collect(),
        notify: Vec::new(),
    }
}

fn ended(test: &str, tag: &str, status: TestStatus) -> Ended {
    let message = if status == TestStatus::Failed { "expected 200, got 500" } else { "" };
    Ended { test: test.to_owned(), tags: vec![Tag(tag.to_owned())], status, message: message.to_owned() }
}

fn run(status: RunStatus, tests: &[(&str, &str, TestStatus)], breaches: &[&str]) -> Finished {
    let summary = RunSummary::new(status, Totals::default()).profiles(["staging"]);
    Finished {
        project: "shop".to_owned(),
        summary,
        tests: tests.iter().map(|(test, tag, status)| ended(test, tag, *status)).collect(),
        breaches: breaches.iter().map(|breach| (*breach).to_owned()).collect(),
        ran: !tests.is_empty() || !breaches.is_empty(),
        teardown: Vec::new(),
        report: None,
    }
}

const PASSED: &[(&str, &str, TestStatus)] = &[("users.crud#getsUser", "smoke", TestStatus::Passed)];
const FAILED: &[(&str, &str, TestStatus)] =
    &[("users.crud#getsUser", "smoke", TestStatus::Failed), ("users.crud#listsUsers", "slow", TestStatus::Flaky)];
const FLAKY: &[(&str, &str, TestStatus)] = &[("users.crud#getsUser", "smoke", TestStatus::Flaky)];
const BLOCKED: &[(&str, &str, TestStatus)] = &[("users.crud#getsUser", "smoke", TestStatus::Blocked)];

/// The trigger each rule fires with, in order; `None` when it does not.
fn fired(rules: &[Alert], run: &Finished, previous: Option<RunStatus>) -> Vec<Option<AlertTrigger>> {
    let fired = decide(rules, run, previous);
    rules
        .iter()
        .map(|rule| fired.iter().find(|fired| std::ptr::eq(fired.rule, rule)).map(|fired| fired.notice.trigger))
        .collect()
}

/// A rule's triggers, the run, the previous run's status, and the trigger expected to fire.
type Case = (&'static str, &'static [AlertTrigger], Finished, Option<RunStatus>, Option<AlertTrigger>);

#[test]
fn each_rule_fires_for_the_first_trigger_it_lists_that_happened() {
    use AlertTrigger::{Blocked, Failure, Flaky, Recovery, Threshold};
    let failed = Some(RunStatus::Failed);
    let passed = Some(RunStatus::Passed);
    let table: &[Case] = &[
        ("a failure", ALL, run(RunStatus::Failed, FAILED, &[]), passed, Some(Failure)),
        ("a blocked test is a failure", &[Failure], run(RunStatus::Failed, BLOCKED, &[]), None, Some(Failure)),
        ("blocked alone", &[Blocked], run(RunStatus::Failed, BLOCKED, &[]), None, Some(Blocked)),
        ("failure outranks blocked", &[Blocked, Failure], run(RunStatus::Failed, BLOCKED, &[]), None, Some(Failure)),
        ("a failed test is not blocked", &[Blocked], run(RunStatus::Failed, FAILED, &[]), None, None),
        ("failure outranks flaky", &[Flaky, Failure], run(RunStatus::Failed, FAILED, &[]), None, Some(Failure)),
        ("flaky when it is all that is listed", &[Flaky], run(RunStatus::Failed, FAILED, &[]), None, Some(Flaky)),
        ("a flaky pass", ALL, run(RunStatus::Passed, FLAKY, &[]), None, Some(Flaky)),
        ("a breach", ALL, run(RunStatus::Failed, &[], &["errorRate"]), None, Some(Threshold)),
        ("a pass after a failure", ALL, run(RunStatus::Passed, PASSED, &[]), failed, Some(Recovery)),
        ("a pass after a pass", ALL, run(RunStatus::Passed, PASSED, &[]), passed, None),
        ("a first pass", ALL, run(RunStatus::Passed, PASSED, &[]), None, None),
        ("a cancelled run never recovers", ALL, run(RunStatus::Cancelled, PASSED, &[]), failed, None),
        ("a run where nothing ran never recovers", ALL, run(RunStatus::Passed, &[], &[]), failed, None),
        ("an unlisted trigger", &[Recovery], run(RunStatus::Failed, FAILED, &[]), passed, None),
        ("nothing listed", &[], run(RunStatus::Failed, FAILED, &[]), failed, None),
    ];
    for (name, on, run, previous, want) in table {
        assert_eq!(fired(&[rule(on, &[], &[])], run, *previous), [*want], "{name}");
    }
}

#[test]
fn profiles_and_tags_narrow_what_a_rule_sees() {
    let failing = run(RunStatus::Failed, FAILED, &[]);
    let rules = [
        rule(ALL, &["prod"], &[]),
        rule(ALL, &["prod", "staging"], &[]),
        rule(ALL, &[], &["slow"]),
        rule(ALL, &[], &["smoke"]),
        rule(ALL, &[], &["nightly"]),
    ];
    let want = [None, Some(AlertTrigger::Failure), Some(AlertTrigger::Flaky), Some(AlertTrigger::Failure), None];
    assert_eq!(fired(&rules, &failing, None), want);
    let recovered = fired(&[rule(ALL, &[], &["slow"])], &failing, Some(RunStatus::Failed));
    assert_eq!(recovered, [Some(AlertTrigger::Flaky)], "the rule's own tests passed; flaky outranks recovery");
}

#[test]
fn a_notice_carries_the_failures_its_rule_sees_the_breaches_and_the_report() {
    let mut finished = run(RunStatus::Failed, FAILED, &["p95 of checkout.flow#paysOrder"]);
    finished.report = Some("reports/ctrf.json".to_owned());
    let rules = [rule(ALL, &[], &["smoke"]), rule(ALL, &[], &["slow"])];
    let fired = decide(&rules, &finished, None);
    let [smoke, slow] = fired.as_slice() else { panic!("{fired:?}") };
    assert_eq!(smoke.notice.failures, [NoticeFailure::new("users.crud#getsUser", "expected 200, got 500")]);
    assert_eq!(smoke.notice.breaches, ["p95 of checkout.flow#paysOrder"]);
    assert_eq!((smoke.notice.project.as_str(), smoke.notice.report.as_deref()), ("shop", Some("reports/ctrf.json")));
    assert_eq!(smoke.notice.summary.profiles, ["staging"]);
    assert_eq!(slow.notice.trigger, AlertTrigger::Threshold, "a breach outranks flaky, whatever the tags");
    assert!(slow.notice.failures.is_empty(), "a rule's notice lists only the tests its tags pick");
}

#[test]
fn a_blocked_notice_lists_only_the_blocked_tests_and_a_failure_notice_the_failed_and_blocked() {
    use AlertTrigger::{Blocked, Failure};
    let tests = &[
        ("users.crud#getsUser", "smoke", TestStatus::Failed),
        ("users.crud#listsUsers", "smoke", TestStatus::Blocked),
        ("users.crud#makesUser", "smoke", TestStatus::Flaky),
    ];
    let finished = run(RunStatus::Failed, tests, &[]);
    let rules = [rule(&[Blocked], &[], &[]), rule(&[Failure], &[], &[])];
    let fired = decide(&rules, &finished, None);
    let listed =
        |index: usize| fired[index].notice.failures.iter().map(|failure| failure.test.clone()).collect::<Vec<_>>();
    assert_eq!(fired[0].notice.trigger, Blocked);
    assert_eq!(listed(0), ["users.crud#listsUsers"]);
    assert_eq!(fired[1].notice.trigger, Failure);
    assert_eq!(listed(1), ["users.crud#getsUser", "users.crud#listsUsers"]);
}
