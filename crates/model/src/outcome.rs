use crate::{ActionId, FileId, InstanceId, TestId};

/// The frame an action runs in, which decides how long its saves and cleanups live.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OwnerKey {
    /// `beforeRun` and `afterRun`.
    Run,
    /// A file's `beforeAll` and `afterAll`.
    File(FileId),
    /// One test instance.
    Instance(InstanceId),
}

/// Why an action, a test or a run did not pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The action that failed, when one did.
    pub action: Option<ActionId>,
    /// The diagnostic code (`V5105`), when the failure has one.
    pub code: Option<String>,
    /// What went wrong, for reports.
    pub message: String,
}

impl Failure {
    /// A failure with `message`, no code and no action.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self { action: None, code: None, message: message.into() }
    }

    /// The same failure with the diagnostic `code`.
    #[must_use]
    pub fn with_code(self, code: impl Into<String>) -> Self {
        Self { code: Some(code.into()), ..self }
    }

    /// The same failure, raised by `action`.
    #[must_use]
    pub fn at(self, action: ActionId) -> Self {
        Self { action: Some(action), ..self }
    }
}

/// How one action ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionOutcome {
    /// Its request went through and its checks held.
    Passed,
    /// It failed.
    Failed(Failure),
}

impl ActionOutcome {
    /// The failure, when the action failed.
    #[must_use]
    pub const fn failure(&self) -> Option<&Failure> {
        match self {
            Self::Passed => None,
            Self::Failed(failure) => Some(failure),
        }
    }
}

/// How one test instance ended (D126).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TestStatus {
    /// Every action passed.
    Passed,
    /// An action failed and no retry passed.
    Failed,
    /// A setup it needs failed (`beforeAll`, `beforeEach`, `before`); never retried.
    Blocked,
    /// Failed, then passed when `retry` re-ran the whole instance.
    Flaky,
    /// Not run: `skip`, or the run was cancelled first.
    Skipped,
}

/// How a whole run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RunStatus {
    /// No test failed or was blocked.
    Passed,
    /// A test failed or was blocked.
    Failed,
    /// Cancelled by a signal; cleanups ran unless aborted.
    Cancelled,
}

/// The result of one test instance, or of a skipped test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestResult {
    /// The test.
    pub test: TestId,
    /// The instance; `None` for a test skipped at plan time.
    pub instance: Option<InstanceId>,
    /// How it ended.
    pub status: TestStatus,
    /// Attempts started; 0 when it never ran.
    pub attempts: u32,
    /// The failure that decided a failed or blocked status, or the failed attempt before a
    /// flaky pass.
    pub failure: Option<Failure>,
    /// Why it was skipped.
    pub skip_reason: Option<String>,
}

/// One cleanup that ran: a failed cleanup is reported (V5107) and changes no result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanupRecord {
    /// The cleanup action.
    pub action: ActionId,
    /// The frame that registered it.
    pub owner: OwnerKey,
    /// How it ended.
    pub outcome: ActionOutcome,
}

/// Instance counts by status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Passed.
    pub passed: usize,
    /// Failed.
    pub failed: usize,
    /// Blocked.
    pub blocked: usize,
    /// Flaky: they count as passed for the exit code.
    pub flaky: usize,
    /// Skipped.
    pub skipped: usize,
}

impl Totals {
    /// Counts one more instance with `status`.
    pub const fn add(&mut self, status: TestStatus) {
        match status {
            TestStatus::Passed => self.passed += 1,
            TestStatus::Failed => self.failed += 1,
            TestStatus::Blocked => self.blocked += 1,
            TestStatus::Flaky => self.flaky += 1,
            TestStatus::Skipped => self.skipped += 1,
        }
    }

    /// Adds every count of `other`.
    pub const fn merge(&mut self, other: Self) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.blocked += other.blocked;
        self.flaky += other.flaky;
        self.skipped += other.skipped;
    }

    /// Every instance and skipped test counted.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.passed + self.failed + self.blocked + self.flaky + self.skipped
    }

    /// Failed plus blocked: what makes a run fail.
    #[must_use]
    pub const fn failures(&self) -> usize {
        self.failed + self.blocked
    }
}

#[cfg(test)]
#[path = "outcome_tests.rs"]
mod tests;
