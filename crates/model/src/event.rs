use std::time::Duration;

use crate::{
    ActionId, ActionOutcome, CheckResult, CleanupRecord, Exchange, Failure, FileId, OwnerKey, RunStatus, TestResult,
    Totals,
};

/// Everything observable about a run, in the order it happened (D103).
///
/// Check results and exchanges leave the executor through its redactor.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum Event {
    /// The run started.
    RunStarted {
        /// Files in the plan.
        files: usize,
        /// Instances in the plan.
        instances: usize,
    },
    /// A file's `beforeAll` is about to run.
    FileStarted {
        /// The file.
        file: FileId,
    },
    /// A file's instances, `afterAll` and cleanups are done.
    FileFinished {
        /// The file.
        file: FileId,
        /// Its instances by status.
        totals: Totals,
    },
    /// An action started.
    ActionStarted {
        /// The action.
        action: ActionId,
        /// The frame it runs in.
        owner: OwnerKey,
    },
    /// An action ended, or was not run because a shared action it needs failed.
    ActionFinished {
        /// The action.
        action: ActionId,
        /// The frame it ran in.
        owner: OwnerKey,
        /// How it ended.
        outcome: ActionOutcome,
    },
    /// A call sent a request and got a response, whole; the redactor has masked it. History and
    /// reports cut its bodies at `history.maxBodySize` only after that, so a cut never splits a
    /// secret before it is masked. A polled call reports the exchange its checks judged.
    Exchanged {
        /// The call.
        action: ActionId,
        /// The frame it ran in.
        owner: OwnerKey,
        /// The request as sent and the response as received, with timings.
        exchange: Box<Exchange>,
    },
    /// A call's check was evaluated; the redactor has masked it.
    CheckEvaluated {
        /// The call.
        action: ActionId,
        /// The frame it ran in.
        owner: OwnerKey,
        /// The check and how it went.
        check: Box<CheckResult>,
    },
    /// An action read a value that an earlier `function:` or `test:` action saved without
    /// declaring it in `provides` (V5108, D186); the run goes on. Published before the action's
    /// `ActionFinished`; the redactor has masked it.
    Warned {
        /// The action that read the value.
        action: ActionId,
        /// The frame it ran in.
        owner: OwnerKey,
        /// The warning, at the action that saved the value.
        warning: Failure,
    },
    /// A cleanup ran.
    CleanupFinished {
        /// What ran and how it ended.
        cleanup: CleanupRecord,
    },
    /// The run was aborted (a third Ctrl-C) before these cleanups finished: the one it stopped in
    /// flight first, then those left, newest first.
    CleanupsAbandoned {
        /// The frame that registered them.
        owner: OwnerKey,
        /// The cleanup actions that did not run.
        cleanups: Vec<ActionId>,
    },
    /// A test instance ended, or a selected test was skipped.
    TestFinished {
        /// The result.
        result: TestResult,
    },
    /// The run ended.
    RunFinished {
        /// How it ended.
        status: RunStatus,
        /// Every instance by status.
        totals: Totals,
    },
    /// One second of a load run closed: what the live view shows. Its rate is over the last
    /// 3 s, its other numbers over the rolling window (the last 10 s), the users over the run
    /// so far.
    LoadTick {
        /// The second of the run that closed, from 1.
        second: u64,
        /// The virtual users running.
        users: u32,
        /// Iterations per second over the last 3 s, rounded.
        rps: u64,
        /// The median iteration latency.
        p50: Duration,
        /// The 95th percentile latency.
        p95: Duration,
        /// The 99th percentile latency.
        p99: Duration,
        /// Failed iterations.
        errors: u64,
        /// The names of the thresholds failing in the rolling window, such as `p95` or
        /// `errorRate`; a test's own threshold is `<test>: <metric>`.
        breaching: Vec<String>,
    },
}

/// An event with its run-wide sequence number, from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Sequenced {
    /// The position in the run; a gap means a subscriber missed events.
    pub seq: u64,
    /// The event.
    pub event: Event,
}

/// What a subscriber receives.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum Feed {
    /// The next event.
    Event(Sequenced),
    /// The subscriber's buffer was full and it missed `count` events; it should reload them from
    /// the history store.
    Missed {
        /// How many events were dropped.
        count: u64,
    },
}
