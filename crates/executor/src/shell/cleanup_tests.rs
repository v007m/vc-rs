//! No cleanup is lost when what registered it is dropped, aborted or panics.

use std::time::Duration;

use vc_model::{ActionId, ActionKind, Event, InstanceId, Loop, OwnerKey, RunStatus};

use super::RunOptions;
use super::tests::{run, totals};
use crate::fixture::Fixture;

const SECOND: Duration = Duration::from_secs(1);

/// A reuse of `[create, then]`, where `create` registers a call of `undo`; and that call.
fn creating_then(fixture: &mut Fixture, undo: &str, then: &str) -> (ActionId, ActionId) {
    let delete = fixture.call(undo);
    let steps = vec![fixture.creating("/users", delete), fixture.call(then)];
    let reused = fixture.test(steps, None);
    (fixture.action(ActionKind::Test(reused)), delete)
}

/// [`creating_then`] `/slow`, looped twice at once.
fn looped(fixture: &mut Fixture) -> (ActionId, ActionId) {
    let (turn, delete) = creating_then(fixture, "/users/delete", "/slow");
    fixture.action_mut(turn).repeat = Some(Loop::Repeat { times: 2, parallel: 2 });
    (turn, delete)
}

#[tokio::test(start_paused = true)]
async fn a_loop_dropped_past_the_timeout_cleans_up_what_its_turns_in_flight_finished() {
    let mut fixture = Fixture::new();
    let (step, delete) = looped(&mut fixture);
    let test = fixture.test(vec![step], None);
    fixture.test_mut(test).timeout = Some(vc_model::Duration::new(2 * SECOND));
    let plan = fixture.plan(&[], &[(test, 1)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[]).await;
    assert_eq!(outcome.summary.totals, totals([0, 1, 0, 0, 0]));
    assert_eq!((outcome.cleaned(), outcome.sent("/users/delete")), (vec![delete, delete], 2));
}

#[tokio::test(start_paused = true)]
async fn a_ctrl_c_during_a_loop_runs_its_turns_cleanups_newest_first_after_the_grace_period() {
    let mut fixture = Fixture::new();
    let first = fixture.call("/first/delete");
    let create = fixture.creating("/first", first);
    let (step, delete) = looped(&mut fixture);
    let test = fixture.test(vec![create, step], None);
    let plan = fixture.plan(&[], &[(test, 1)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[SECOND]).await;
    assert_eq!(outcome.summary.status, RunStatus::Cancelled);
    assert_eq!(outcome.cleaned(), [delete, delete, first]);
    assert_eq!((outcome.sent("/users/delete"), outcome.elapsed), (2, 6 * SECOND), "cut after the 5 s grace");
}

#[tokio::test(start_paused = true)]
async fn an_abort_during_a_step_abandons_the_cleanups_its_finished_parts_registered() {
    let mut fixture = Fixture::new();
    let (step, delete) = creating_then(&mut fixture, "/users/delete", "/slow");
    let test = fixture.test(vec![step], None);
    let plan = fixture.plan(&[], &[(test, 1)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[SECOND, SECOND, SECOND]).await;
    assert_eq!(outcome.summary.status, RunStatus::Cancelled);
    let abandoned = Event::CleanupsAbandoned { owner: OwnerKey::Instance(InstanceId::new(0)), cleanups: vec![delete] };
    assert!(outcome.events.contains(&abandoned), "{:?}", outcome.events);
    assert_eq!((outcome.cleaned(), outcome.sent("/users/delete")), (vec![], 0));
}

#[tokio::test(start_paused = true)]
async fn an_action_that_panics_fails_and_the_cleanups_it_and_earlier_steps_registered_run() {
    let mut fixture = Fixture::new();
    let first = fixture.call("/first/delete");
    let create = fixture.creating("/first", first);
    let (step, delete) = creating_then(&mut fixture, "/users/delete", "/panic");
    let test = fixture.test(vec![create, step], None);
    let plan = fixture.plan(&[], &[(test, 1)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[]).await;
    assert_eq!(outcome.summary.totals, totals([0, 1, 0, 0, 0]));
    assert_eq!(outcome.cleaned(), [delete, first]);
    let failure = outcome.results()[0].failure.clone().map(|failure| failure.message);
    assert_eq!(failure.as_deref(), Some("the action panicked: the fixture's worker failed"));
}

#[tokio::test(start_paused = true)]
async fn a_cleanup_that_panics_fails_and_the_cleanups_before_it_still_run() {
    let mut fixture = Fixture::new();
    let (first, crash) = (fixture.call("/first/delete"), fixture.call("/panic"));
    let steps = vec![fixture.creating("/first", first), fixture.creating("/second", crash)];
    let test = fixture.test(steps, None);
    let plan = fixture.plan(&[], &[(test, 1)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[]).await;
    assert_eq!(outcome.summary.totals, totals([1, 0, 0, 0, 0]));
    assert_eq!((outcome.cleaned(), outcome.sent("/first/delete")), (vec![crash, first], 1));
}

#[tokio::test(start_paused = true)]
async fn a_before_all_that_panics_blocks_its_tests_and_cleans_up_what_it_registered() {
    let mut fixture = Fixture::new();
    let (before, delete) = creating_then(&mut fixture, "/users/delete", "/panic");
    let get = fixture.call("/users/1");
    let test = fixture.test(vec![get], None);
    let plan = fixture.plan(&[before], &[(test, 1)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[]).await;
    assert_eq!(outcome.summary.totals, totals([0, 0, 1, 0, 0]));
    assert_eq!((outcome.cleaned(), outcome.sent("/users/delete"), outcome.sent("/users/1")), (vec![delete], 1, 0));
}
