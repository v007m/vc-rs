use async_trait::async_trait;
use vc_model::{Action, ActionId, ActionKind, ActionOutcome, Failure, Program, TestBody};

use super::{ActionExecutor, ActionRequest};
use crate::ActionReport;

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
        let mut report = ActionReport::passed();
        for action in steps {
            let step = request.cx.executors.execute(ActionRequest { action, ..request.clone() }).await;
            report.cleanups.extend(step.cleanups);
            report.saved.extend(step.saved);
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
