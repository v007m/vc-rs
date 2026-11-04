use vc_model::{ActionId, TestId};

use crate::ActionKindTag;

/// Why a run cannot start, or ended without a result.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ExecutorError {
    /// The plan reaches an action the program does not have.
    #[error("the plan reaches action {} that the program does not have", .0.index())]
    UnknownAction(ActionId),
    /// The plan names a test the program does not have.
    #[error("the plan names a test the program does not have ({0:?})")]
    UnknownTest(TestId),
    /// No executor is registered for this kind of action: `function:` until M4 (the planner
    /// rejects those actions first), `command:` until its runner lands.
    #[error("action {} is a `{kind}` action, which this build cannot run yet", .action.index())]
    NoExecutor {
        /// The action.
        action: ActionId,
        /// Its kind.
        kind: ActionKindTag,
    },
    /// The run task stopped without a result.
    #[error("the run stopped without a result")]
    Stopped,
}
