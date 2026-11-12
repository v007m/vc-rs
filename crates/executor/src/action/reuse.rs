use std::sync::Arc;

use async_trait::async_trait;
use tokio::time::{self, Instant};
use vc_diagnostics::Code;
use vc_model::{Action, ActionId, ActionKind, ActionOutcome, Failure, Program, Test, TestBody};

use super::{ActionExecutor, ActionRequest, provides};
use crate::{ActionReport, CleanupCollector, Defined};

/// Runs `test:` reuse: the reused test's call or steps, in order, in the current frame.
///
/// It stops at the first failure. The reused test runs as written (D270): its calls carry its
/// own test-level checks, merged in at link time, and none of the reusing test's step defaults;
/// its own `before` and `after` are not run. The reusing action's own `timeout` limits the whole reuse (D192): the step
/// running when it passes is dropped and fails with V5104. Each step's cleanups are handed to
/// the reuse's [`CleanupCollector`] as it finishes, as are those of a dropped step's finished
/// parts, so none is lost, and neither are the reuse's own when a caller's deadline drops it. It
/// takes them with [`CleanupCollector::settle`] once its frame is gone, so the turns of a looped
/// step that its `timeout` dropped hand theirs over too.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct TestExecutor;

#[async_trait]
impl ActionExecutor for TestExecutor {
    async fn execute(&self, request: ActionRequest) -> ActionReport {
        let program = &request.cx.program;
        let Some(test) = request.cx.action(request.action).and_then(|action| reused_test(program, action)) else {
            return ActionReport::failed(Failure::new("the action reuses no test in the program").at(request.action));
        };
        let steps = body_actions(&test.body);
        let declared = request.cx.action(request.action);
        let deadline = declared.and_then(|action| action.timeout).map(|limit| Instant::now() + limit.get());
        let kept = CleanupCollector::under(request.frame.sink.clone());
        let mut frame = kept.frame(&request.frame);
        frame.callers.extend(callers(&request));
        frame.test_file = Some(test.file);
        let mut report = ActionReport::passed();
        for action in steps {
            let step = ActionRequest { action, frame: frame.clone(), ..request.clone() };
            let step = within(deadline, request.cx.executors.execute(step), request.action).await;
            frame.absorb(&request.cx.program, action, &step);
            let part = step.failing_after(report.exchanges.len());
            kept.hand(step.cleanups);
            report.saved.extend(step.saved);
            report.fakes.extend(step.fakes);
            report.checks.extend(step.checks);
            report.exchanges.extend(step.exchanges);
            report.warnings.extend(step.warnings);
            report.response = step.response.or(report.response);
            if let ActionOutcome::Failed(failure) = step.outcome {
                report.outcome = ActionOutcome::Failed(failure);
                report.failing = Some(part);
                break;
            }
        }
        drop(frame);
        report.cleanups = kept.settle(Vec::new()).await;
        if report.outcome == ActionOutcome::Passed {
            let unsaved = declared.and_then(|action| provides::unsaved(program, action, &report.saved));
            report.outcome =
                unsaved.map_or(report.outcome, |failure| ActionOutcome::Failed(failure.at(request.action)));
        }
        report
    }

    fn follows(&self, program: &Program, action: &Action) -> Vec<ActionId> {
        reused(program, action).unwrap_or_default()
    }
}

/// `work`'s report, or the V5104 failure of the reuse `id` once `deadline` passes.
async fn within(deadline: Option<Instant>, work: impl Future<Output = ActionReport>, id: ActionId) -> ActionReport {
    let Some(deadline) = deadline else { return work.await };
    time::timeout_at(deadline, work).await.unwrap_or_else(|_| {
        let failure = Failure::new("the reused test ran past its `timeout`").with_code(Code::V5104.to_string());
        ActionReport::failed(failure.at(id))
    })
}

/// The vars a reuse adds to its steps' scope, outermost first: the reused test's, then the
/// reusing action's, which win.
fn callers(request: &ActionRequest) -> Vec<Arc<Defined>> {
    let values = &request.cx.values;
    let test = request.cx.action(request.action).and_then(|action| match action.kind {
        ActionKind::Test(test) => values.test(test),
        _ => None,
    });
    test.into_iter().chain(values.action(request.action)).collect()
}

/// The actions of the test `action` reuses.
fn reused(program: &Program, action: &Action) -> Option<Vec<ActionId>> {
    reused_test(program, action).map(|test| body_actions(&test.body))
}

/// The test `action` reuses.
fn reused_test<'p>(program: &'p Program, action: &Action) -> Option<&'p Test> {
    let ActionKind::Test(test) = &action.kind else { return None };
    program.tests.get(test)
}

/// A test's call or steps, in order.
#[must_use]
pub fn body_actions(body: &TestBody) -> Vec<ActionId> {
    match body {
        TestBody::Call(action) => vec![*action],
        TestBody::Steps(steps) => steps.clone(),
    }
}
