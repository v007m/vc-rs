//! `vc plan`, and the part of `vc run` that decides what runs: settings, profiles, selection and
//! the planner.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::Value;
use vc_executor::RunInputs;
use vc_interfaces::{ConfigEntry, Environment, TestRow, VcsError};
use vc_model::{Plan, ProfileId, Program, SourcePath};
use vc_planner::{PlanError, PlanOptions, Selection};

use crate::examples::{self, Draw};
use crate::failed;
use crate::ports::Layered;
use crate::project::invalid;
use crate::run;
use crate::settings::{Named, Settings};
use crate::{Engine, EngineError, PlanRequest, SelectRequest};

/// What `--changed` compares against when no ref is given: the working tree's changes.
const DEFAULT_BASE: &str = "HEAD";
/// Files every test reads: a change to one selects every test.
const SHARED: &[&str] = &["vibranium.config.yaml", ".env.schema"];
/// What a save in `vc watch` selects every test for: the shared files and the local `.env`.
const SAVED_SHARED: &[&str] = &["vibranium.config.yaml", ".env.schema", ".env"];

/// What a run would do, without sending a request.
#[derive(Clone, Debug)]
pub struct PlanReport {
    program: Arc<Program>,
    plan: Arc<Plan>,
    profiles: Vec<String>,
    notices: Vec<String>,
}

impl PlanReport {
    /// The plan line, with the active profiles:
    /// `1 test in 1 file · beforeEach ×3 calls · profile staging`.
    #[must_use]
    pub fn line(&self) -> String {
        let line = vc_planner::plan_line(&self.program, &self.plan);
        match self.profiles.as_slice() {
            [] => line,
            [one] => format!("{line} · profile {one}"),
            many => format!("{line} · profiles {}", many.join(", ")),
        }
    }

    /// The plan as an indented tree, in run order.
    #[must_use]
    pub fn tree(&self) -> String {
        vc_planner::tree(&self.program, &self.plan)
    }

    /// The plan as JSON, for scripts and agents (D130).
    #[must_use]
    pub fn json(&self) -> Value {
        vc_planner::json(&self.program, &self.plan)
    }

    /// The plan as a Graphviz DOT graph.
    #[must_use]
    pub fn dot(&self) -> String {
        vc_planner::dot(&self.program, &self.plan)
    }

    /// How many test instances the plan runs.
    #[must_use]
    pub fn instance_count(&self) -> usize {
        self.plan.instance_count()
    }

    /// Problems that did not stop the plan, one line each.
    #[must_use]
    pub fn notices(&self) -> &[String] {
        &self.notices
    }
}

/// A planned run: the program with the settings applied, the plan, the active profiles, the
/// environment the run reads and the `.env` variables a `command:` adds to it.
pub struct Prepared {
    pub program: Arc<Program>,
    pub plan: Arc<Plan>,
    pub settings: Settings,
    pub profiles: Vec<ProfileId>,
    pub names: Vec<String>,
    pub notices: Vec<String>,
    pub environment: Arc<dyn Environment>,
    /// The `.env` variables the process environment does not set.
    pub dot_env: Vec<(String, String)>,
    /// The run's random source and the example rows drawn from it before planning.
    pub draw: Draw,
}

/// What `vc run` adds to a plan request: its flags that are settings, the results `--failed`
/// runs again, and `--seed`, which the example rows are drawn with too.
#[derive(Default)]
pub struct Extra {
    pub flags: Vec<ConfigEntry>,
    pub rerun: Vec<TestRow>,
    pub seed: Option<u64>,
}

impl Prepared {
    /// The plan as `vc plan` shows it.
    pub fn report(&self) -> PlanReport {
        PlanReport {
            program: Arc::clone(&self.program),
            plan: Arc::clone(&self.plan),
            profiles: self.names.clone(),
            notices: self.notices.clone(),
        }
    }
}

impl Engine {
    /// Plans what `request` selects, as `vc run` would, without sending a request.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors, [`EngineError::Setting`] for a bad
    /// setting or an unknown profile, and [`EngineError::Selection`] when a selector cannot be
    /// read, matches nothing, or `--changed` cannot ask git.
    pub fn plan(&self, request: &PlanRequest) -> Result<PlanReport, EngineError> {
        Ok(self.prepare(request, Extra::default())?.report())
    }

    /// Reads the settings, compiles, applies the settings, picks the profiles and plans.
    pub(crate) fn prepare(&self, request: &PlanRequest, extra: Extra) -> Result<Prepared, EngineError> {
        let Extra { mut flags, rerun, seed } = extra;
        flags.splice(0..0, profile_flag(&request.profiles));
        let Layered { mut program, notices, settings, environment, dot_env, .. } =
            self.layered(&request.sets, &flags)?;
        if let Some(timeout) = settings.timeout {
            Arc::make_mut(&mut program).config.defaults.timeout = Some(timeout);
        }
        let (profiles, names) = profiles(&program, settings.profiles.as_ref())?;
        let mut selection = self.selection(&request.select)?;
        if !rerun.is_empty() {
            selection.ids = failed::selectors(&program, &rerun, request.repeat)?;
        }
        let mut draw = Draw::new(seed);
        let inputs = RunInputs::new(Arc::clone(&draw.random), Arc::clone(&environment));
        let inputs = inputs.with_profiles(profiles.clone()).with_vars(run::vars(&settings.vars));
        let unrendered = examples::render(&program, inputs, &mut draw);
        let (repeat, today, example_rows) =
            (request.repeat, Some(self.clock.today().to_string()), examples::counts(&draw.rows));
        let options = PlanOptions { repeat, today, example_rows };
        let refused = |error| unrendered.refused(&error, &program).unwrap_or_else(|| plan_error(error, &program));
        let plan = vc_planner::plan(&program, &selection, &options).map_err(refused)?;
        Ok(Prepared { program, plan: Arc::new(plan), settings, profiles, names, notices, environment, dot_env, draw })
    }

    /// The planner's selection for `select`, with `--changed` asked of the vcs; `vc watch`'s
    /// saved files stand in for `--changed`.
    pub(crate) fn selection(&self, select: &SelectRequest) -> Result<Selection, EngineError> {
        let changed = match (&select.saved, select.changed.as_deref()) {
            (Some(saved), _) => affected(saved, SAVED_SHARED),
            (None, Some(base)) => self.changed(base)?,
            (None, None) => None,
        };
        Ok(Selection {
            ids: select.ids.clone(),
            paths: select.paths.clone(),
            tags: select.tags.clone(),
            grep: select.grep.clone(),
            changed,
            include_skipped: select.include_skipped,
        })
    }

    /// The IDs of the definition files changed since `base` (`HEAD` when empty: the uncommitted
    /// and untracked changes); `None`, every file, when a file every test reads changed.
    fn changed(&self, base: &str) -> Result<Option<BTreeSet<String>>, EngineError> {
        let base = if base.is_empty() { DEFAULT_BASE } else { base };
        let paths = self.vcs.changed(base).map_err(|error| vcs_error(&error))?;
        Ok(affected(&paths, SHARED))
    }
}

/// The IDs of the definition files among `paths`; `None`, every file, when one is `shared`.
fn affected(paths: &[SourcePath], shared: &[&str]) -> Option<BTreeSet<String>> {
    if paths.iter().any(|path| shared.contains(&path.as_str())) {
        return None;
    }
    Some(paths.iter().filter_map(SourcePath::file_id).collect())
}

/// `--profile`'s entry, when profiles are named: a comma list of them all, in order.
pub fn profile_flag(profiles: &[String]) -> Option<ConfigEntry> {
    (!profiles.is_empty()).then(|| ConfigEntry::new("profile", profiles.join(","), "--profile"))
}

/// The active profiles, by ID and by name: those `named`, else the ones marked `default: true`.
/// The active profiles: their IDs, and their names for the plan line.
type Active = (Vec<ProfileId>, Vec<String>);

pub fn profiles(program: &Program, named: Option<&Named>) -> Result<Active, EngineError> {
    let Some(named) = named.filter(|named| !named.names.is_empty()) else {
        return defaults(program);
    };
    let mut ids = Vec::new();
    for name in &named.names {
        let Some(id) = program.profiles.keys().copied().find(|id| program.str(id.sym()) == Some(name)) else {
            let help = Some(format!("the profiles are {}", known(program)));
            return Err(EngineError::Setting {
                origin: named.origin.clone(),
                reason: format!("no profile `{name}`"),
                help,
            });
        };
        ids.push(id);
    }
    Ok((ids, named.names.clone()))
}

/// The profiles marked `default: true`; a project with profiles but no default needs one named.
fn defaults(program: &Program) -> Result<Active, EngineError> {
    let ids: Vec<ProfileId> = program.profiles.values().filter(|p| p.default).map(|p| p.id).collect();
    if ids.is_empty() && !program.profiles.is_empty() {
        let help = format!("pass `--profile <id>`; the profiles are {}", known(program));
        return Err(EngineError::Selection { reason: "no profile is active".into(), help: Some(help) });
    }
    let names = ids.iter().map(|id| program.str(id.sym()).unwrap_or_default().to_owned()).collect();
    Ok((ids, names))
}

fn known(program: &Program) -> String {
    let names: Vec<&str> = program.profiles.keys().filter_map(|id| program.str(id.sym())).collect();
    names.join(", ")
}

/// The planner's refusal as the engine's: definition problems as a check report, selector
/// problems with their help.
fn plan_error(error: PlanError, program: &Program) -> EngineError {
    match error {
        PlanError::Diagnostics(found) => invalid(found.into_iter().collect(), program.files.clone(), Vec::new()),
        other => EngineError::Selection { reason: other.to_string(), help: other.help() },
    }
}

/// `--changed` could not ask the vcs.
fn vcs_error(error: &VcsError) -> EngineError {
    let help = match error {
        VcsError::InvalidRef { .. } => Some("pass a branch, tag or commit, such as `--changed origin/main`".to_owned()),
        VcsError::Unavailable { .. } => Some("install git, or run without `--changed`".to_owned()),
        _ => None,
    };
    EngineError::Selection { reason: format!("--changed: {error}"), help }
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
