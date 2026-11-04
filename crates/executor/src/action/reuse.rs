use std::sync::Arc;

use async_trait::async_trait;
use vc_model::{Action, ActionId, ActionKind, ActionOutcome, Failure, Program, TestBody};

use super::{ActionExecutor, ActionRequest};
use crate::{ActionReport, Defined};

/// Runs `test:` reuse: the reused test's call or steps, in order, in the current frame, stopping
/// at the first failure. The reused test's own `before`, `after` and test-level checks are not
/// run.
#[derive(Clone, Copy, Debug, Default)]
pub struct TestExecutor;

#[async_trait]
impl ActionExecutor for TestExecutor {
    async fn execute(&self, request: ActionRequest) -> ActionReport {
        let Some(steps) = request.cx.action(request.action).and_then(|action| reused(&request.cx.program, action))
        else {
            return ActionReport::failed(Failure::new("the action reuses no test in the program").at(request.action));
        };
        let mut frame = request.frame.clone();
        frame.callers.extend(callers(&request));
        let mut report = ActionReport::passed();
        for action in steps {
            let step = ActionRequest { action, frame: frame.clone(), ..request.clone() };
            let step = request.cx.executors.execute(step).await;
            frame.absorb(&request.cx.program, action, &step);
            report.cleanups.extend(step.cleanups);
            report.saved.extend(step.saved);
            report.fakes.extend(step.fakes);
            report.response = step.response.or(report.response);
            if let ActionOutcome::Failed(failure) = step.outcome {
                report.outcome = ActionOutcome::Failed(failure);
                break;
            }
        }
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
    let ActionKind::Test(test) = &action.kind else { return None };
    Some(body_actions(&program.tests.get(test)?.body))
}

/// A test's call or steps, in order.
#[must_use]
pub fn body_actions(body: &TestBody) -> Vec<ActionId> {
    match body {
        TestBody::Call(action) => vec![*action],
        TestBody::Steps(steps) => steps.clone(),
    }
}
