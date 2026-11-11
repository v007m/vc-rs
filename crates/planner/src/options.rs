//! What the caller asks the planner for.

use std::collections::{BTreeMap, BTreeSet};

use vc_model::TestId;

/// Which tests to run, as `vc run` and `vc plan` take them.
///
/// Positional selectors (`ids` and `paths`) are joined with "or"; the flags (`tags`, `grep`,
/// `changed`) then filter with "and". An empty selection selects every test.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "options struct filled with `..Default::default()` (D160)")]
pub struct Selection {
    /// Test IDs (`users.crud#getsUser`), instances (`users.crud#getsUser:2`), file IDs
    /// (`users.crud`) and globs over IDs (`users.*`). A path given here is treated as a path.
    pub ids: Vec<String>,
    /// Folders or test files, relative to the project root (`services/orders/`).
    pub paths: Vec<String>,
    /// One tag expression per `-t` flag, all of which must hold (`smoke,checkout & !slow`).
    pub tags: Vec<String>,
    /// Text that the test's full ID or name contains, ignoring case.
    pub grep: Option<String>,
    /// File IDs that changed, resolved by the engine through its `Vcs` (`--changed`).
    pub changed: Option<BTreeSet<String>>,
    /// Plans skipped tests and files instead of reporting them as skipped.
    pub include_skipped: bool,
}

/// Run settings that change the plan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "options struct filled with `..Default::default()` (D160)")]
pub struct PlanOptions {
    /// `--repeat N`: runs every selected test `N` times instead of its own `repeat`.
    pub repeat: Option<u32>,
    /// Today as `YYYY-MM-DD`, for `skip.until`; `None` keeps every skip in force.
    pub today: Option<String>,
    /// How many rows each test whose `examples` come from a file or a template has, as the
    /// engine counted them when it preloaded them (D132).
    pub example_rows: BTreeMap<TestId, u32>,
    /// Plans a test whose examples give no rows as skipped ([`crate::NO_ROWS`]), as a run's plan
    /// does; `false` plans it with no instances, as `vc list` and the catalog count without
    /// reading rows.
    pub skip_empty: bool,
}
