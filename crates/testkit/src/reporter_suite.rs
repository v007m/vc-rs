use std::time::Duration;

use serde_json::json;
use vc_interfaces::{Artifact, Reporter, RunSummary};
use vc_model::{
    ActionId, ActionOutcome, CheckKind, CheckResult, CleanupRecord, Event, Failure, FileId, InstanceId, ModelError,
    Origin, OwnerKey, Pos, RunStatus, SourceFileId, SourceSpan, StringTable, TestId, TestResult, TestStatus, Totals,
};

/// Builds a fresh reporter.
pub type MakeReporter<'a> = dyn FnMut() -> Box<dyn Reporter> + Send + 'a;

/// The contract every [`Reporter`] meets (D112). `make` builds a reporter for a program that
/// knows none of the IDs the suite's events name, so it must fall back to their raw form.
///
/// # Panics
///
/// When the reporter breaks the contract.
pub async fn reporter_suite(make: &mut MakeReporter<'_>) {
    finishes_a_run_without_events(make).await;
    takes_every_kind_of_event(make).await;
}

async fn finishes_a_run_without_events(make: &mut MakeReporter<'_>) {
    let mut reporter = make();
    let summary = RunSummary::new(RunStatus::Passed, Totals::default());
    let artifacts = reporter.finish(&summary).await;
    assert_eq!(artifacts.as_ref().err(), None, "finish without events failed");
    check_artifacts(&artifacts.unwrap_or_default());
}

async fn takes_every_kind_of_event(make: &mut MakeReporter<'_>) {
    let mut reporter = make();
    let built = every_event();
    assert_eq!(built.as_ref().err(), None, "the suite's IDs intern");
    let Ok((events, totals)) = built else { return };
    for event in &events {
        let taken = reporter.on_event(event).await;
        assert_eq!(taken.err(), None, "on_event failed for {event:?}");
    }
    let summary = RunSummary::new(RunStatus::Failed, totals).timed(1, Duration::from_millis(5)).profiles(["dev"]);
    let artifacts = reporter.finish(&summary).await;
    assert_eq!(artifacts.as_ref().err(), None, "finish failed");
    check_artifacts(&artifacts.unwrap_or_default());
}

fn check_artifacts(artifacts: &[Artifact]) {
    for artifact in artifacts {
        assert!(!artifact.reporter.is_empty(), "an artifact names its reporter: {artifact:?}");
        assert!(artifact.path.file_name().is_some(), "an artifact names a file: {artifact:?}");
    }
}

/// The IDs the suite's events name.
#[derive(Clone, Copy)]
struct Ids {
    file: FileId,
    test: TestId,
    setup: ActionId,
    call: ActionId,
    owner: OwnerKey,
}

/// One of each event, with IDs from a table the reporter does not have.
fn every_event() -> Result<(Vec<Event>, Totals), ModelError> {
    let mut table = StringTable::new();
    let file = FileId::new(table.intern("users.crud")?);
    let test = TestId::new(table.intern("users.crud#getsUser")?);
    let ids = Ids {
        file,
        test,
        setup: ActionId::new(0),
        call: ActionId::new(1),
        owner: OwnerKey::Instance(InstanceId::new(0)),
    };
    let mut totals = Totals::default();
    for status in [TestStatus::Failed, TestStatus::Blocked, TestStatus::Flaky, TestStatus::Skipped] {
        totals.add(status);
    }
    let mut events = vec![Event::RunStarted { files: 1, instances: 3 }, Event::FileStarted { file }];
    events.extend(actions(ids));
    events.extend(results(ids));
    events.extend([Event::FileFinished { file, totals }, Event::RunFinished { status: RunStatus::Failed, totals }]);
    Ok((events, totals))
}

fn call_failure(ids: Ids) -> Failure {
    Failure::new("expected 200, got 500").with_code("V5101").at(ids.call)
}

/// A passing lifecycle action, a failing call with its check, and cleanups.
fn actions(ids: Ids) -> Vec<Event> {
    let Ids { file, test, setup, call, owner } = ids;
    let cleanup = CleanupRecord { action: setup, owner, outcome: ActionOutcome::Passed };
    vec![
        Event::ActionStarted { action: setup, owner: OwnerKey::File(file) },
        Event::ActionFinished { action: setup, owner: OwnerKey::File(file), outcome: ActionOutcome::Passed },
        Event::ActionStarted { action: call, owner },
        Event::CheckEvaluated { action: call, owner, check: Box::new(failed_check(test)) },
        Event::ActionFinished { action: call, owner, outcome: ActionOutcome::Failed(call_failure(ids)) },
        Event::CleanupFinished { cleanup },
        Event::CleanupsAbandoned { owner: OwnerKey::Run, cleanups: vec![setup] },
    ]
}

/// A failed, a blocked, a flaky and a skipped test.
fn results(ids: Ids) -> Vec<Event> {
    let finished = |status, instance: Option<u32>, failure: Option<Failure>| Event::TestFinished {
        result: TestResult {
            test: ids.test,
            instance: instance.map(InstanceId::new),
            status,
            attempts: u32::from(instance.is_some()),
            failure,
            skip_reason: instance.is_none().then(|| "parked".to_owned()),
        },
    };
    vec![
        finished(TestStatus::Failed, Some(0), Some(call_failure(ids))),
        finished(TestStatus::Blocked, Some(1), Some(Failure::new("setup failed").at(ids.setup))),
        finished(TestStatus::Flaky, Some(2), Some(call_failure(ids))),
        finished(TestStatus::Skipped, None, None),
    ]
}

fn failed_check(test: TestId) -> CheckResult {
    let span = SourceSpan::new(SourceFileId::new(9), Pos::new(3, 5, 20), Pos::new(3, 16, 31));
    CheckResult {
        kind: CheckKind::Status,
        target: None,
        origin: Origin::Test(test),
        span,
        passed: false,
        expected: json!(200),
        actual: Some(json!(500)),
        diff: None,
        error: None,
        code: Some("V5101".to_owned()),
    }
}
