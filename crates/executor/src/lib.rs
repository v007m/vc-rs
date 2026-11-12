//! Runs a Plan through the interfaces: frames, lifecycle, events and the redactor.
//!
//! The run, file and instance frames are pure state machines ([`RunOrchestrator`],
//! [`FileOrchestrator`], [`InstanceWorkflow`]). [`Executor`] runs a plan with one task per frame
//! that owns its machine and carries out its commands; workers report back as inputs. Shared
//! lifecycle results land in write-once [`SharedSlots`], events go out on the [`EventBus`] and
//! each action kind runs through the [`ActionExecutors`] registry.
//!
//! So far: the request pipeline (D141), `tower` services around the [`vc_interfaces::Transport`]
//! built by [`pipeline`]: timing, retry ([`HttpRetryPolicy`], D143), auth (a [`RequestStage`] null
//! object until M3), each api's `beforeRequest` hooks, the per-host limit ([`HostLimits`]) and the
//! per-attempt timeout. Hooks, custom checks and `function:` actions ([`FunctionExecutor`]) run
//! through the [`vc_interfaces::HookRunner`] in [`PipelineParts::hook_runner`] (D172). Runtime values: [`RunValues`] holds the var definitions of each level with
//! the active profiles applied once per run; each action reads its [`Frame`] and renders through
//! a renderer, so a `call:` sends the real request and saves values for later actions.
//! [`SeededRandom`] is the run's `--seed` random source, the same in every composition root.
//! [`Iterations`] is the seam the load engine drives: one test instance at a time, per user;
//! [`ProgramIterations`] runs the program's tests through it, the shared lifecycle once.

mod action;
mod bus;
mod cancel;
mod check;
mod error;
mod exchange_mask;
mod exit;
#[cfg(test)]
mod fixture;
mod iterations;
mod machine;
mod mask;
mod pipeline;
mod redactor;
mod seeded;
mod shell;
mod slots;
mod values;

pub use action::{
    ActionCx, ActionExecutor, ActionExecutors, ActionKindTag, ActionRequest, AuthKey, CachedExecutor, CallExecutor,
    CleanupSink, CommandExecutor, CommandShell, FunctionExecutor, InlineExecutor, Lease, NoAuth, RuleHooks, RunCache,
    TestExecutor, TokenCache, auth_steps, body_actions, received,
};
use action::{CleanupCollector, called, own_limit};
pub use bus::EventBus;
pub use cancel::CancelTree;
pub use error::ExecutorError;

pub use check::{Arguments, Judge, MatcherFn, MatcherRegistry, Schemas};
pub use exit::{EXIT_CANCELLED, EXIT_FAILED, EXIT_NOTHING_RAN, EXIT_PASSED, exit_code};
pub use iterations::{IterationCall, IterationFailure, IterationReport, Iterations};
pub use machine::{
    ActionReport, CANCELLED, FailedActions, FileCommand, FileInput, FileOrchestrator, FilePhase, InstanceCommand,
    InstanceInput, InstancePhase, InstanceScript, InstanceWorkflow, Launch, PendingCleanup, RunCommand, RunInput,
    RunOrchestrator, RunPhase, RunSummary, Saved, Verdict,
};
pub use mask::{CelRule, MaskRule, MaskRuleMaker, MaskRules, RedactRuleKind, RegexRule};
pub use pipeline::{
    Attempted, BoxFuture, Call, Delivery, Hop, HostLimit, HostLimits, HttpRetryPolicy, Jittered, NoRetry, Pipeline,
    PipelineParts, RequestStage, Retried, Retry, RetryOn, RetryPolicy, Stage, Timeout, Timing, Transmit, Unchanged,
    pipeline,
};
pub use redactor::{MASKED, RedactingSink, Redactor};
pub use seeded::{MAX_FRESH_SEED, SeededRandom};
pub use shell::{Executor, IterationSetup, ProgramIterations, RunHandle, RunInterrupter, RunOptions};
pub use slots::{SharedResult, SharedSlots};
use values::{Around, Render};
pub use values::{
    BodyFiles, Defined, ExampleRows, Frame, Iteration, RandomFor, RunInputs, RunValues, Unread, UnreadVar, draw_rows,
    example_rows, row_names, row_seed, unread_var,
};
