use std::sync::Arc;

use async_trait::async_trait;
use vc_model::{Action, ActionId, ActionKind, ActionOutcome, Failure, Program, Test, TestBody};

use super::{ActionExecutor, ActionRequest, provides};
use crate::{ActionReport, Defined};

/// Runs `test:` reuse: the reused test's call or steps, in order, in the current frame, stopping
/// at the first failure. The reused test's own `before`, `after` and test-level checks are not
/// run.
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
        let mut frame = request.frame.clone();
        frame.callers.extend(callers(&request));
        frame.test_file = Some(test.file);
        let mut report = ActionReport::passed();
        for action in steps {
            let step = ActionRequest { action, frame: frame.clone(), ..request.clone() };
            let step = request.cx.executors.execute(step).await;
            frame.absorb(&request.cx.program, action, &step);
            report.cleanups.extend(step.cleanups);
            report.saved.extend(step.saved);
            report.fakes.extend(step.fakes);
            report.checks.extend(step.checks);
            report.exchanges.extend(step.exchanges);
            report.response = step.response.or(report.response);
            if let ActionOutcome::Failed(failure) = step.outcome {
                report.outcome = ActionOutcome::Failed(failure);
                return report;
            }
        }
        let unsaved = declared.and_then(|action| provides::unsaved(program, action, &report.saved));
        report.outcome = unsaved.map_or(report.outcome, |failure| ActionOutcome::Failed(failure.at(request.action)));
        report
    }

    fn follows(&self, program: &Program, action: &Action) -> Vec<ActionId> {
        reused(program, action).unwrap_or_default()
    }
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
