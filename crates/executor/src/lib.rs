//! Runs a Plan through the interfaces: frames, lifecycle, events and the redactor.
//!
//! So far: the request pipeline (D141), `tower` services around the [`vc_interfaces::Transport`]
//! built by [`pipeline`]: timing, retry ([`HttpRetryPolicy`], D143), auth and `beforeRequest` hooks
//! ([`RequestStage`], null objects until M3 and M4), the per-host limit ([`HostLimits`]) and the
//! per-attempt timeout.

mod exit;
mod machine;
mod pipeline;

pub use exit::{EXIT_CANCELLED, EXIT_FAILED, EXIT_NOTHING_RAN, EXIT_PASSED, exit_code};
pub use machine::{
    ActionReport, FailedActions, FileCommand, FileInput, FileOrchestrator, FilePhase, InstanceCommand, InstanceInput,
    InstancePhase, InstanceScript, InstanceWorkflow, Launch, PendingCleanup, RunCommand, RunInput, RunOrchestrator,
    RunPhase, RunSummary, Saved, Verdict,
};
pub use pipeline::{
    Attempted, BoxFuture, Call, Delivery, Hop, HostLimit, HostLimits, HttpRetryPolicy, NoRetry, Pipeline,
    PipelineParts, RequestStage, Retried, Retry, RetryOn, RetryPolicy, Stage, Timeout, Timing, Transmit, Unchanged,
    pipeline,
};
