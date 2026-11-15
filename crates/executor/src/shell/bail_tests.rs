//! `--bail` one instance at a time: an instance keeps its turn until the run has carried out its
//! status, so the instance after a failure starts only once the run has cancelled, and never sends.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time;
use vc_model::{Plan, TestStatus};

use super::crew::Crew;
use super::instance::{Ticket, instance};
use super::run::{self, Inbound, RunFrame};
use super::script;
use crate::cancel::CancelTree;
use crate::fixture::{Fixture, Sent, Server};
use crate::{ActionExecutors, EventBus, Frame, HostLimits, Launch, PipelineParts, RedactingSink, RunInput};

const SECOND: Duration = Duration::from_secs(1);

/// A crew of one turn for `plan`, with the run's inbox the test reads in its place.
fn crew(fixture: Fixture, plan: &Plan) -> (Crew, Sent, mpsc::UnboundedReceiver<Inbound>) {
    let (server, sent) = Server::new();
    let cx = fixture.cx(ActionExecutors::standard(PipelineParts::new(server, HostLimits::spawn(8))));
    let scripts = Arc::new(script::scripts(&cx.program, plan).unwrap());
    let (run, inbox) = mpsc::unbounded_channel();
    let bus = RedactingSink::new(EventBus::spawn(), Arc::clone(&cx.redactor));
    (Crew { cx, bus, scripts, limit: Arc::new(Semaphore::new(1)), window: 1, run }, sent, inbox)
}

/// A plan of `count` instances of a test that calls `path`.
fn plan_of(path: &str, count: u32) -> (Fixture, Plan) {
    let mut fixture = Fixture::new();
    let call = fixture.call(path);
    let test = fixture.test(vec![call], None);
    let plan = fixture.plan(&[], &[(test, count)]);
    (fixture, plan)
}

/// Starts every instance of `plan`, each waiting for its turn.
fn start(crew: &Crew, plan: &Plan, cancel: &CancelTree) -> Vec<JoinHandle<TestStatus>> {
    let launch = |planned: &vc_model::Instance| Launch { instance: planned.clone(), block: None };
    let ticket = |planned| Ticket { launch: launch(planned), frame: Frame::default(), cancel: cancel.child() };
    plan.files[0].instances.iter().map(|planned| tokio::spawn(instance(crew.clone(), ticket(planned)))).collect()
}

async fn statuses(running: Vec<JoinHandle<TestStatus>>) -> Vec<TestStatus> {
    let mut statuses = Vec::new();
    for running in running {
        statuses.push(running.await.unwrap());
    }
    statuses
}

#[tokio::test(start_paused = true)]
async fn the_instance_after_a_failure_waits_for_the_run_and_never_sends_once_it_cancels() {
    let (fixture, plan) = plan_of("/fail", 2);
    let (crew, sent, mut inbox) = crew(fixture, &plan);
    let cancel = CancelTree::new(SECOND);
    let running = start(&crew, &plan, &cancel);
    let first = inbox.recv().await.unwrap();
    assert!(matches!(first.input, RunInput::Tested(TestStatus::Failed)) && first.carried_out.is_some());
    time::sleep(SECOND).await;
    assert_eq!(sent.counts().into_iter().collect::<Vec<_>>(), [("/fail".to_owned(), 1)], "the next one waits");
    cancel.cancel();
    first.carried_out.unwrap().send(()).unwrap();
    let second = inbox.recv().await.unwrap();
    assert!(matches!(second.input, RunInput::Tested(TestStatus::Skipped)));
    second.carried_out.unwrap().send(()).unwrap();
    assert_eq!(statuses(running).await, [TestStatus::Failed, TestStatus::Skipped]);
    assert!(sent.counts().is_empty(), "the instance after the failure sent nothing");
}

#[tokio::test(start_paused = true)]
async fn a_run_gone_before_it_carries_out_a_status_lets_the_turn_go() {
    let (fixture, plan) = plan_of("/fail", 2);
    let (crew, sent, mut inbox) = crew(fixture, &plan);
    let running = start(&crew, &plan, &CancelTree::new(SECOND));
    let both = async {
        drop(inbox.recv().await);
        drop(inbox);
        statuses(running).await
    };
    let statuses = time::timeout(10 * SECOND, both).await.expect("no instance waits forever");
    assert_eq!(statuses, [TestStatus::Failed, TestStatus::Failed]);
    assert_eq!(sent.counts()["/fail"], 2);
}

#[tokio::test(start_paused = true)]
async fn the_run_tells_an_instance_only_once_its_bail_cancel_is_carried_out() {
    let (fixture, plan) = plan_of("/slow", 1);
    let (crew, _sent, inbox) = crew(fixture, &plan);
    let (root, sender) = (CancelTree::new(SECOND), crew.run.clone());
    let watch = root.child();
    let running = tokio::spawn(run::run(crew, Arc::new(plan), RunFrame { inbox, cancel: root, bail: true }));
    let (carried_out, done) = oneshot::channel();
    sender.send(Inbound { input: RunInput::Tested(TestStatus::Failed), carried_out: Some(carried_out) }).unwrap();
    done.await.expect("the run tells, not drops");
    assert!(watch.is_cancelled(), "the cancel is carried out before the run tells");
    time::timeout(10 * SECOND, running).await.expect("the cancelled run finishes").unwrap();
}
