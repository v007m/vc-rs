//! Runs a Plan through the interfaces: frames, lifecycle, events and the redactor.
//!
//! The run, file and instance frames are pure state machines ([`RunOrchestrator`],
//! [`FileOrchestrator`], [`InstanceWorkflow`]). [`Executor`] runs a plan with one task per frame
//! that owns its machine and carries out its commands; workers report back as inputs. Shared
//! lifecycle results land in write-once [`SharedSlots`], events go out on the [`EventBus`] and
//! each action kind runs through the [`ActionExecutors`] registry.
//!
//! So far: the request pipeline (D141), `tower` services around the [`vc_interfaces::Transport`]
//! built by [`pipeline`]: timing, retry ([`HttpRetryPolicy`], D143), auth and `beforeRequest` hooks
//! ([`RequestStage`], null objects until M3 and M4), the per-host limit ([`HostLimits`]) and the
//! per-attempt timeout. Runtime values: [`RunValues`] holds the var definitions of each level with
//! the active profiles applied once per run; each action reads its [`Frame`] and renders through
//! a renderer, so a `call:` sends the real request and saves values for later actions.

mod action;
mod bus;
mod cancel;
mod check;
mod error;
mod exit;
#[cfg(test)]
mod fixture;
mod machine;
mod pipeline;
mod redactor;
mod shell;
mod slots;
mod values;

pub use action::{
    ActionCx, ActionExecutor, ActionExecutors, ActionKindTag, ActionRequest, CallExecutor, TestExecutor, body_actions,
};
pub use bus::EventBus;
pub use error::ExecutorError;

pub use check::{Arguments, Judge, MatcherFn, MatcherRegistry};
pub use exit::{EXIT_CANCELLED, EXIT_FAILED, EXIT_NOTHING_RAN, EXIT_PASSED, exit_code};
pub use machine::{
    ActionReport, CANCELLED, FailedActions, FileCommand, FileInput, FileOrchestrator, FilePhase, InstanceCommand,
    InstanceInput, InstancePhase, InstanceScript, InstanceWorkflow, Launch, PendingCleanup, RunCommand, RunInput,
    RunOrchestrator, RunPhase, RunSummary, Saved, Verdict,
};
pub use pipeline::{
    Attempted, BoxFuture, Call, Delivery, Hop, HostLimit, HostLimits, HttpRetryPolicy, Jittered, NoRetry, Pipeline,
    PipelineParts, RequestStage, Retried, Retry, RetryOn, RetryPolicy, Stage, Timeout, Timing, Transmit, Unchanged,
    pipeline,
};
pub use redactor::{MASKED, RedactingSink, Redactor};
pub use shell::{Executor, RunHandle, RunOptions};
pub use slots::{SharedResult, SharedSlots};
use values::Render;
pub use values::{Defined, FakeData, FakeFn, Frame, Iteration, RunInputs, RunValues};
