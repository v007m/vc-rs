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
    /// No executor is registered for this kind of action, as when an embedder builds the
    /// registry without one.
    #[error("action {} is a `{kind}` action, which this build cannot run yet", .action.index())]
    NoExecutor {
        /// The action.
        action: ActionId,
        /// Its kind.
        kind: ActionKindTag,
    },
    /// A load run's `beforeRun` or `beforeAll` action failed, or was cut, so no iteration can
    /// run.
    #[error("the shared action {} failed: {message}", .action.index())]
    SharedFailed {
        /// The action.
        action: ActionId,
        /// Its failure.
        message: String,
    },
    /// The run task stopped without a result.
    #[error("the run stopped without a result")]
    Stopped,
}
