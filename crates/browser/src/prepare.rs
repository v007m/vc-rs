//! What a playground run plans: the default profiles, the seed, the example rows drawn from it
//! and the plan, as `vc run` with no flags plans them (`engine/plan.rs` and `examples.rs`; the
//! engine's glue is layered over settings the playground does not have, so it is mirrored here
//! and the CLI's `playground` e2e test holds the two to the same output).

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher as _, Hasher as _};
use std::sync::Arc;

use vc_executor::{Defined, ExampleRows, RunInputs, RunValues, SeededRandom};
use vc_interfaces::{Environment, Random};
use vc_model::{Examples, Plan, ProfileId, Program};
use vc_planner::{PlanError, PlanOptions, Selection};

use crate::BrowserError;

/// A planned run: the plan, the active profiles by ID and by name, the seed and its random
/// source, the rows drawn before planning and the notices so far.
pub struct Prepared {
    pub program: Arc<Program>,
    pub plan: Arc<Plan>,
    pub profiles: Vec<ProfileId>,
    pub names: Vec<String>,
    pub seed: u64,
    pub random: Arc<dyn Random>,
    pub rows: ExampleRows,
    pub notices: Vec<String>,
}

/// Plans every test of `program` with its default profiles, drawing from `seed` or a fresh one.
pub fn prepare(
    program: Arc<Program>,
    environment: Arc<dyn Environment>,
    (seed, today): (Option<u64>, String),
) -> Result<Prepared, BrowserError> {
    let (profiles, names) = vc_planner::active_profiles(&program, &[]).map_err(|error| plan_error(error, &program))?;
    let seed = seed.unwrap_or_else(|| RandomState::new().build_hasher().finish());
    let random: Arc<dyn Random> = Arc::new(SeededRandom::new(seed));
    let inputs =
        RunInputs::new(Arc::clone(&random), environment).with_profiles(profiles.clone()).with_vars(Defined::new());
    let options = PlanOptions { today: Some(today), ..PlanOptions::default() };
    let (plan, rows) = planned(&program, inputs, (seed, options))?;
    let notices = vc_planner::no_rows_notices(&program, &plan);
    Ok(Prepared { program, plan: Arc::new(plan), profiles, names, seed, random, rows, notices })
}

/// Renders the rows of the templated tests the run does not skip, each from its own source
/// ([`vc_executor::row_seed`]), then plans once with their counts.
fn planned(
    program: &Program,
    inputs: RunInputs,
    (seed, mut options): (u64, PlanOptions),
) -> Result<(Plan, ExampleRows), BrowserError> {
    let selection = Selection::default();
    let templated = program.tests.values().any(|test| matches!(test.examples, Some(Examples::Template(_))));
    let wanted = if templated { vc_planner::rows_wanted(program, &selection, &options) } else { Ok(Vec::new()) };
    let wanted = wanted.map_err(|error| plan_error(error, program))?;
    let mut rows = ExampleRows::new();
    if !wanted.is_empty() {
        let values = RunValues::new(program, inputs);
        let random_for = |id: &str| Arc::new(SeededRandom::new(vc_executor::row_seed(seed, id))) as Arc<dyn Random>;
        rows = vc_executor::draw_rows(program, &values, (&wanted, &random_for)).map_err(|diagnostic| {
            BrowserError::invalid(std::iter::once(*diagnostic).collect(), program.files.clone())
        })?;
        let counts = rows.iter().map(|(test, drawn)| (*test, u32::try_from(drawn.len()).unwrap_or(u32::MAX)));
        options.example_rows.extend(counts);
    }
    let plan = vc_planner::plan(program, &selection, &options).map_err(|error| plan_error(error, program))?;
    Ok((plan, rows))
}

/// The planner's refusal: definition problems as the project's diagnostics, the rest with help.
fn plan_error(error: PlanError, program: &Program) -> BrowserError {
    match error {
        PlanError::Diagnostics(found) => BrowserError::invalid(found.into_iter().collect(), program.files.clone()),
        other => BrowserError::refused(other.to_string(), other.help()),
    }
}

/// The plan line `vc run` heads its report with: `plan  3 tests in 2 files · profile local`.
pub fn headline(prepared: &Prepared) -> String {
    let line = vc_planner::plan_line(&prepared.program, &prepared.plan);
    let line = match prepared.names.as_slice() {
        [] => line,
        [one] => format!("{line} · profile {one}"),
        many => format!("{line} · profiles {}", many.join(", ")),
    };
    format!("plan  {line}")
}
