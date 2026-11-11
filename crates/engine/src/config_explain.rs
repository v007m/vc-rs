//! `vc config explain <key>`: a setting's value and the layer that set it, over the values it
//! replaced (D129): built-in, `vibranium.config.yaml`, profiles, the user config, `.env`,
//! `VIB_*`, flags and `--set`. Every value is masked as [`Engine::config_values`] masks it, so a
//! secret's value never reaches the terminal (D232).

use std::fmt::Write as _;

use vc_model::{Program, TemplateValue, VarName};

use crate::config_values::shown_unless_secret;
use crate::explain::{CONFIG, ExplainReport, Explainer, VARS, did_you_mean};
use crate::settings::KEYS;
use crate::shown::Shown;
use crate::{ConfigValue, Engine, EngineError, ExplainRequest, reporting};

/// The timeout a call has when nothing sets one: the executor's.
const TIMEOUT: &str = "30s";
/// Where a built-in value comes from.
pub const BUILT_IN: &str = "built-in";

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
    ("defaults.retry.attempts", Some("0"), retry_attempts),
    ("defaults.retry.delay", Some("0s"), retry_delay),
    ("reports.dir", Some(reporting::REPORTS_DIR), reports_dir),
    ("reports.formats", Some("ctrf"), formats),
    ("reports.html.layout", Some("folder"), html_layout),
];

/// One value a key was set to, and where.
pub type Layer = (String, String);

impl Engine {
    /// The value `key` has under `request`, the layer that set it, and the values it replaced,
    /// each masked.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors, [`EngineError::Setting`] for a bad
    /// setting or an unknown profile, and [`EngineError::Selection`] when `key` is not a setting
    /// or a var anything sets, with the closest keys as help.
    pub async fn config_explain(&self, key: &str, request: &ExplainRequest) -> Result<ExplainReport, EngineError> {
        let found = self.config_values(&keys_under(key), request).await?;
        let text = found.values.iter().map(text).collect();
        Ok(ExplainReport { text, notices: found.notices })
    }
}

/// The settings under `key` when it names a group of them (`defaults.retry`), else `key`.
fn keys_under(key: &str) -> Vec<String> {
    let group = format!("{key}.");
    let under: Vec<String> =
        SETTINGS.iter().map(|(name, _, _)| *name).filter(|name| name.starts_with(&group)).map(str::to_owned).collect();
    if under.is_empty() { vec![key.to_owned()] } else { under }
}

/// The values `key` had, lowest first; `None` when it is no setting, and none when it is a
/// var nothing sets.
pub fn layers_of(explainer: &Explainer<'_>, key: &str) -> Option<Vec<Layer>> {
    key.strip_prefix(VARS).map_or_else(|| setting_layers(explainer, key), |name| Some(var_layers(explainer, name)))
}

/// Every setting `vc config explain` knows, then `vars.<name>` for each var some layer sets.
pub fn every_key(explainer: &Explainer<'_>) -> Vec<String> {
    let mut keys: Vec<String> = SETTINGS.iter().map(|(name, _, _)| (*name).to_owned()).collect();
    keys.extend(explainer.config_vars().into_iter().map(|var| format!("{VARS}{}", var.name)));
    keys
}

/// `key = value  (origin)`, then each value it replaced, latest first.
fn text(value: &ConfigValue) -> String {
    let mut text = String::new();
    for (at, layer) in value.layers.iter().enumerate() {
        let (shown, origin) = (&layer.value, &layer.source);
        let _ = if at == 0 {
            writeln!(text, "{} = {shown}  ({origin})", value.key)
        } else {
            writeln!(text, "  over {shown}  ({origin})")
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
    let mut layers: Vec<Layer> =
        found.into_iter().filter_map(|(value, at)| Some((shown_unless_secret(&shown, value?), at))).collect();
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
pub fn unknown(explainer: &Explainer<'_>, key: &str) -> EngineError {
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

fn retry_attempts(program: &Program) -> Option<String> {
    program.config.defaults.retry.map(|retry| retry.attempts.to_string())
}

fn retry_delay(program: &Program) -> Option<String> {
    program.config.defaults.retry.map(|retry| duration(retry.delay.get()))
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
