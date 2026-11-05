use crate::{
    ActionId, ActionOutcome, CheckResult, CleanupRecord, Exchange, Failure, FileId, OwnerKey, RunStatus, TestResult, Totals,
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
    /// A call sent a request and got a response; the redactor has masked it, and its bodies are
    /// cut at `history.maxBodySize`. A polled call reports the exchange its checks judged.
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
    /// The run was aborted (a third Ctrl-C) before these cleanups ran, newest first.
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
