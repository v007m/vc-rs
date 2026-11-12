//! `vc explain <id>`: a test or api as a run under the active profiles sees it (D88): the final
//! URL, each var with the level that set it, the headers and auth, the merged expect with the
//! values it checks, the cleanup it registers and, for a test, the lifecycle around it.

#[path = "explain_api.rs"]
mod api;
#[path = "explain_call.rs"]
mod call;
#[path = "explain_steps.rs"]
mod steps;

use std::fmt::Write as _;

use vc_interfaces::ConfigEntry;
use vc_model::{Action, ActionId, ActionKind, Api, ProfileId, Program, Sym, VarDefs};

pub use self::api::{ApiExplained, AuthShown, SentHeader};
use self::call::Called;

use crate::plan::{self, profile_flag};
use crate::shown::{self, Var};
use crate::{Engine, EngineError, ExplainRequest};

/// The config file, as the level its vars come from.
pub const CONFIG: &str = "vibranium.config.yaml";
/// The prefix of the settings that set config vars.
pub const VARS: &str = "vars.";

/// What `vc explain` or `vc config explain` shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExplainReport {
    /// The explanation, one item per line.
    pub text: String,
    /// Problems that did not stop it, one line each.
    pub notices: Vec<String>,
}

impl Engine {
    /// Explains the test or api with the full ID `id` under the profiles and settings of
    /// `request`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors, [`EngineError::Setting`] for a bad
    /// setting or an unknown profile, and [`EngineError::Selection`] when no test or api has the
    /// ID, with the closest ones as help.
    pub fn explain(&self, id: &str, request: &ExplainRequest) -> Result<ExplainReport, EngineError> {
        let flags: Vec<ConfigEntry> = profile_flag(&request.profiles).into_iter().collect();
        let layered = self.layered(&request.sets, &flags)?;
        let program = layered.program.as_ref();
        let (active, names) = plan::profiles(program, layered.settings.profiles.as_ref())?;
        let explainer = Explainer { program, active: &active, names: &names, entries: &layered.entries };
        Ok(ExplainReport { text: explainer.explain(id)?, notices: layered.notices })
    }
}

/// The program, the active profiles and the settings a definition is explained under.
pub struct Explainer<'a> {
    pub program: &'a Program,
    pub active: &'a [ProfileId],
    pub names: &'a [String],
    pub entries: &'a [ConfigEntry],
}

impl Explainer<'_> {
    fn explain(&self, id: &str) -> Result<String, EngineError> {
        if let Some(test) = self.program.tests.values().find(|test| self.text(test.id.sym()) == id) {
            return Ok(self.test(test));
        }
        let Some(api) = self.program.apis.values().find(|api| self.text(api.id.sym()) == id) else {
            return Err(unknown(self.program, id));
        };
        let mut out = self.header(id, api.name.as_deref());
        let mut vars = self.config_vars();
        self.api_vars(api, &mut vars);
        out.push_str(&self.request(&Called { api, action: None }, &vars));
        Ok(out)
    }

    /// `users.crud#createsUser  Creates a user`, then the active profiles.
    fn header(&self, id: &str, name: Option<&str>) -> String {
        let mut out = id.to_owned();
        if let Some(name) = name {
            out.push_str("  ");
            out.push_str(name);
        }
        let profiles = if self.names.is_empty() { "(none)".to_owned() } else { self.names.join(", ") };
        let _ = write!(out, "\nprofile  {profiles}\n");
        out
    }

    /// The config's vars, each active profile's over them, then the settings' (D129).
    pub(crate) fn config_vars(&self) -> Vec<Var> {
        let mut vars = Vec::new();
        self.level(&mut vars, &self.program.config.vars, CONFIG);
        for profile in self.active.iter().filter_map(|id| self.program.profiles.get(id)) {
            let origin = format!("profile {}", self.text(profile.id.sym()));
            for (name, value) in &profile.vars {
                shown::set(
                    &mut vars,
                    Var {
                        name: self.text(name.sym()),
                        value: value.clone(),
                        origin: origin.clone(),
                        saved: false,
                        replaced: Vec::new(),
                    },
                );
            }
        }
        for entry in self.entries {
            let Some(name) = entry.key.strip_prefix(VARS).filter(|name| !name.is_empty()) else { continue };
            let var = Var {
                name: name.to_owned(),
                value: shown::literal(&entry.value),
                origin: entry.origin.clone(),
                saved: false,
                replaced: Vec::new(),
            };
            shown::set(&mut vars, var);
        }
        vars
    }

    /// The vars of `api`'s file, then its own.
    fn api_vars(&self, api: &Api, vars: &mut Vec<Var>) {
        if let Some(group) = self.program.api_groups.get(&api.group) {
            self.level(vars, &group.vars, &format!("file {}", self.text(group.id.sym())));
        }
        self.level(vars, &api.vars, &format!("api {}", self.text(api.id.sym())));
    }

    /// The vars `defs` sets at the level `origin`, each active profile's over them.
    pub(crate) fn level(&self, vars: &mut Vec<Var>, defs: &VarDefs, origin: &str) {
        let profiles = self.active.iter().filter_map(|id| Some((id, defs.by_profile.get(id)?)));
        let profiles = profiles.map(|(id, values)| (format!("{origin}, profile {}", self.text(id.sym())), values));
        for (origin, values) in std::iter::once((origin.to_owned(), &defs.values)).chain(profiles) {
            for (name, value) in values {
                let var = Var {
                    name: self.text(name.sym()),
                    value: value.clone(),
                    origin: origin.clone(),
                    saved: false,
                    replaced: Vec::new(),
                };
                shown::set(vars, var);
            }
        }
    }

    /// The api a cleanup calls, else what it does.
    fn cleanup(&self, id: ActionId) -> String {
        match self.action(id).map(|action| &action.kind) {
            Some(ActionKind::Call(api)) => self.text(api.sym()),
            _ => vc_planner::action_label(self.program, id),
        }
    }

    fn action(&self, id: ActionId) -> Option<&Action> {
        self.program.actions.get(usize::try_from(id.index()).ok()?)
    }

    fn text(&self, sym: Sym) -> String {
        self.program.str(sym).unwrap_or_default().to_owned()
    }
}

/// No test or api has the ID `id`; the closest ones are the help.
fn unknown(program: &Program, id: &str) -> EngineError {
    let tests = program.tests.keys().map(|test| test.sym());
    let names = tests.chain(program.apis.keys().map(|api| api.sym())).filter_map(|sym| program.str(sym));
    EngineError::Selection {
        reason: format!("no test or api `{id}`"),
        help: did_you_mean(&vc_planner::closest(id, names)),
    }
}

/// `did you mean `a` or `b`?`, when there is anything near.
pub fn did_you_mean(near: &[String]) -> Option<String> {
    let near: Vec<String> = near.iter().map(|name| format!("`{name}`")).collect();
    (!near.is_empty()).then(|| format!("did you mean {}?", near.join(" or ")))
}

#[cfg(test)]
#[path = "explain_tests.rs"]
mod tests;
