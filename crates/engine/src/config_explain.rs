//! `vc config explain <key>`: a setting's value and the layer that set it, over the values it
//! replaced (D129): built-in, `vibranium.config.yaml`, profiles, the user config, `.env`,
//! `VIB_*`, flags and `--set`.

use std::fmt::Write as _;

use vc_interfaces::ConfigEntry;
use vc_model::{Program, TemplateValue, VarName};

use crate::explain::{CONFIG, ExplainReport, Explainer, VARS, did_you_mean};
use crate::plan::{self, profile_flag};
use crate::settings::KEYS;
use crate::shown::Shown;
use crate::{Engine, EngineError, ExplainRequest, reporting};

/// The timeout a call has when nothing sets one: the executor's.
const TIMEOUT: &str = "30s";
/// Where a built-in value comes from.
const BUILT_IN: &str = "built-in";

/// What `vibranium.config.yaml` sets a key to, if anything.
type Written = fn(&Program) -> Option<String>;

/// A key, its built-in value and what the config sets it to.
type Setting = (&'static str, Option<&'static str>, Written);

/// Each key with its built-in value and what the project's config sets it to.
const SETTINGS: &[Setting] = &[
    ("profile", None, default_profiles),
    ("defaults.concurrency", Some("16"), concurrency),
    ("defaults.perHost", Some("8"), per_host),
    ("defaults.timeout", Some(TIMEOUT), timeout),
    ("reports.dir", Some(reporting::REPORTS_DIR), reports_dir),
    ("reports.formats", Some("ctrf"), formats),
    ("reports.html.layout", Some("folder"), html_layout),
];

/// One value a key was set to, and where.
type Layer = (String, String);

impl Engine {
    /// The value `key` has under `request`, the layer that set it, and the values it replaced.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors, [`EngineError::Setting`] for a bad
    /// setting or an unknown profile, and [`EngineError::Selection`] when `key` is not a setting
    /// or a var anything sets, with the closest keys as help.
    pub fn config_explain(&self, key: &str, request: &ExplainRequest) -> Result<ExplainReport, EngineError> {
        let flags: Vec<ConfigEntry> = profile_flag(&request.profiles).into_iter().collect();
        let layered = self.layered(&request.sets, &flags)?;
        let program = layered.program.as_ref();
        let (active, names) = plan::profiles(program, layered.settings.profiles.as_ref())?;
        let explainer = Explainer { program, active: &active, names: &names, entries: &layered.entries };
        let layers = match key.strip_prefix(VARS) {
            Some(name) => var_layers(&explainer, name),
            None => setting_layers(&explainer, key).ok_or_else(|| unknown(&explainer, key))?,
        };
        if layers.is_empty() {
            return Err(unknown(&explainer, key));
        }
        Ok(ExplainReport { text: text(key, &layers), notices: layered.notices })
    }
}

/// `key = value  (origin)`, then each value it replaced, latest first.
fn text(key: &str, layers: &[Layer]) -> String {
    let mut text = String::new();
    for (at, (value, origin)) in layers.iter().rev().enumerate() {
        let _ = if at == 0 {
            writeln!(text, "{key} = {value}  ({origin})")
        } else {
            writeln!(text, "  over {value}  ({origin})")
        };
    }
    text
}

/// The values a setting other than a var had, lowest first; `None` for an unknown key.
fn setting_layers(explainer: &Explainer<'_>, key: &str) -> Option<Vec<Layer>> {
    let (_, built_in, written) = SETTINGS.iter().find(|(name, _, _)| *name == key)?;
    let mut layers: Vec<Layer> = built_in.iter().map(|value| ((*value).to_owned(), BUILT_IN.to_owned())).collect();
    layers.extend(written(explainer.program).map(|value| (value, CONFIG.to_owned())));
    layers.extend(entries(explainer, key));
    Some(layers)
}

/// The values the var `name` had, lowest first: the config's, each active profile's, then
/// the settings'.
fn var_layers(explainer: &Explainer<'_>, name: &str) -> Vec<Layer> {
    let program = explainer.program;
    let mut found = vec![(var(program, &program.config.vars.values, name), CONFIG.to_owned())];
    for id in explainer.active {
        let profile = program.str(id.sym()).unwrap_or_default();
        let by_profile = program.config.vars.by_profile.get(id).and_then(|values| var(program, values, name));
        found.push((by_profile, format!("{CONFIG}, profile {profile}")));
        let defined = program.profiles.get(id).and_then(|defined| var(program, &defined.vars, name));
        found.push((defined, format!("profile {profile}")));
    }
    let shown = Shown { program, vars: &[] };
    let mut layers: Vec<Layer> = found.into_iter().filter_map(|(value, at)| Some((shown.value(value?), at))).collect();
    layers.extend(entries(explainer, &format!("{VARS}{name}")));
    layers
}

/// The value `values` gives the var `name`.
fn var<'v>(
    program: &Program,
    values: impl IntoIterator<Item = (&'v VarName, &'v TemplateValue)>,
    name: &str,
) -> Option<&'v TemplateValue> {
    values.into_iter().find(|(var, _)| program.str(var.sym()) == Some(name)).map(|(_, value)| value)
}

/// The values the layers above the files set `key` to, lowest first.
fn entries(explainer: &Explainer<'_>, key: &str) -> Vec<Layer> {
    let matching = explainer.entries.iter().filter(|entry| entry.key == key);
    matching.map(|entry| (entry.value.trim().to_owned(), entry.origin.clone())).collect()
}

/// `key` is no setting, or a var nothing sets; the closest keys are the help.
fn unknown(explainer: &Explainer<'_>, key: &str) -> EngineError {
    let mut known: Vec<String> = KEYS.iter().map(|(name, _)| (*name).to_owned()).collect();
    known.extend(explainer.config_vars().into_iter().map(|var| format!("{VARS}{}", var.name)));
    let near = vc_planner::closest(key, known.iter().map(String::as_str));
    let help = did_you_mean(&near).or_else(|| Some(format!("the settings are {} and vars.<name>", known.join(", "))));
    EngineError::Selection { reason: format!("no setting `{key}`"), help }
}

fn default_profiles(program: &Program) -> Option<String> {
    let names: Vec<&str> = program
        .profiles
        .values()
        .filter(|profile| profile.default)
        .filter_map(|profile| program.str(profile.id.sym()))
        .collect();
    (!names.is_empty()).then(|| names.join(","))
}

fn concurrency(program: &Program) -> Option<String> {
    program.config.defaults.concurrency.map(|count| count.to_string())
}

fn per_host(program: &Program) -> Option<String> {
    program.config.defaults.per_host.map(|count| count.to_string())
}

fn timeout(program: &Program) -> Option<String> {
    program.config.defaults.timeout.map(|timeout| duration(timeout.get()))
}

fn reports_dir(program: &Program) -> Option<String> {
    program.config.reports_dir.clone()
}

fn formats(program: &Program) -> Option<String> {
    let formats = &program.config.report_formats;
    (!formats.is_empty()).then(|| formats.join(","))
}

fn html_layout(program: &Program) -> Option<String> {
    program.config.html_single_file.then(|| "single".to_owned())
}

/// `20s`, or `250ms` when it is not whole seconds.
fn duration(duration: std::time::Duration) -> String {
    if duration.subsec_millis() == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

#[cfg(test)]
#[path = "config_explain_tests.rs"]
mod tests;
