//! How a test did across stored runs (G15): the History tab's flaky and failing tests.

/// One test over the newest runs of a window, from [`crate::HistoryReader::trends`].
#[derive(Clone, Debug, Default, PartialEq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub struct TestTrend {
    /// Its full ID: `users.crud#getsUser`.
    pub test_id: String,
    /// Its results that ran in the window, each `examples` row and repeat counted; skipped
    /// results are left out.
    pub results: u32,
    /// How many of them failed or were blocked.
    pub failed: u32,
    /// How many of them were flaky.
    pub flaky: u32,
    /// Their mean wall time, in milliseconds.
    pub mean_ms: f64,
}
