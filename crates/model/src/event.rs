use crate::{ActionId, ActionOutcome, CheckResult, CleanupRecord, FileId, OwnerKey, RunStatus, TestResult, Totals};

/// Everything observable about a run, in the order it happened (D103).
///
/// Check results leave the executor through its redactor; exchanges are added later.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// A call's check was evaluated; the redactor has masked it.
    CheckEvaluated {
        /// The call.
        action: ActionId,
        /// The frame it ran in.
        owner: OwnerKey,
        /// The check and how it went.
        check: Box<CheckResult>,
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
pub struct Sequenced {
    /// The position in the run; a gap means a subscriber missed events.
    pub seq: u64,
    /// The event.
    pub event: Event,
}

/// What a subscriber receives.
#[derive(Clone, Debug, PartialEq, Eq)]
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
