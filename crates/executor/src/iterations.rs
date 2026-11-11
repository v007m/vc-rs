//! The seam a load run drives (lld "Load engine"): one instance of a test, run again and again
//! by each virtual user, through the same action runner as a functional run.

use std::time::Duration;

use async_trait::async_trait;
use vc_model::{ApiId, TestId};

/// Runs single instances of a program's tests for a load run.
///
/// The run's `beforeRun` and each mixed file's `beforeAll` have run once already, so an
/// iteration reads their saved values from the shared slots; `beforeEach`, `before`, the steps
/// and the cleanups run inside every iteration. The load engine times each call from its
/// scheduled start and owns the stages, think time and thresholds; an implementation only runs
/// the instance and reports what it sent.
///
/// ```
/// use std::time::Duration;
///
/// use vc_executor::{IterationCall, IterationReport, Iterations};
/// use vc_model::TestId;
///
/// struct AlwaysPasses;
///
/// #[async_trait::async_trait]
/// impl Iterations for AlwaysPasses {
///     async fn iterate(&self, _test: TestId, _user: u32) -> IterationReport {
///         let call = IterationCall { api: None, status: Some(200), elapsed: Duration::from_millis(3) };
///         IterationReport { passed: true, calls: vec![call], failure: None }
///     }
/// }
/// ```
#[async_trait]
pub trait Iterations: Send + Sync {
    /// Runs one instance of `test` for virtual user `user` (from 1), with `checks: api` or
    /// `full` as the scenario asks, and reports how it went. A failure is a report, not an
    /// error: the load run counts it and goes on.
    async fn iterate(&self, test: TestId, user: u32) -> IterationReport;
}

/// How one iteration went.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the executor and by load-engine test fakes; every user is in this repo (D160)"
)]
pub struct IterationReport {
    /// Whether every action and check passed.
    pub passed: bool,
    /// Each request the instance sent, in order, cleanups included.
    pub calls: Vec<IterationCall>,
    /// What failed it, when it did not pass.
    pub failure: Option<IterationFailure>,
}

/// What failed an iteration: the first failure, as a functional run reports it, and where.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the executor and by load-engine test fakes; every user is in this repo (D160)"
)]
pub struct IterationFailure {
    /// The api the failing action calls; `None` when the failure is not at a call to an api
    /// (a `command:` step, an inline request, a test that is not in the plan).
    pub api: Option<ApiId>,
    /// The failing action's last response status; `None` when it got none (a timeout, a
    /// refused connection, a template that failed before the request).
    pub status: Option<u16>,
    /// The diagnostic code (`V5101` for a check, `V5105` for the transport), when it has one.
    pub code: Option<String>,
    /// What went wrong.
    pub message: String,
}

/// One request an iteration sent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the executor and by load-engine test fakes; every user is in this repo (D160)"
)]
pub struct IterationCall {
    /// The api it called, or `None` for an inline request.
    pub api: Option<ApiId>,
    /// The response status, or `None` when no response came (timeout, refused connection).
    pub status: Option<u16>,
    /// From sending the request to the end of the response body.
    pub elapsed: Duration,
}
