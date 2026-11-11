//! Pure planner: a selection over a Program to a Plan.
//!
//! [`plan`] picks the tests a selection names, expands them into instances, plans the `beforeRun`
//! and `beforeAll` blocks only where an instance runs (D122) and records which shared actions
//! each instance reads. The render functions turn the plan into the plan line, the `vc plan` tree,
//! JSON, DOT and the `--explain` listing. No I/O: preloaded example row counts arrive in
//! [`PlanOptions`] (D132); [`rows_wanted`] names the templated tests whose rows the engine must
//! render first, and a test whose examples gave no rows is skipped ([`NO_ROWS`]).
//! [`active_profiles`] picks the profiles a run applies.

mod bodies;
mod build;
mod error;
mod expand;
mod needs;
mod options;
mod profiles;
mod render;
mod select;
#[cfg(test)]
mod testing;
mod visit;

pub use bodies::{bodies, bodies_from};
pub use build::{plan, rows_wanted};
pub use error::PlanError;
pub use expand::{NO_ROWS, no_rows_notices};
pub use options::{PlanOptions, Selection};
pub use profiles::{ActiveProfiles, active_profiles};
pub use render::{action_label, check_lines, dot, explain, json, plan_line, shown_check_lines, tree};
pub use select::{closest, select_apis, select_files};
