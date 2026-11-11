//! The run settings from the config layers above the project's files (D129): `.env`, the
//! `VIB_*` variables, the run's flags and `--set`, later layers winning.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use vc_interfaces::{ConfigEntry, ConfigSource, ConfigSourceError};
use vc_model::{Duration, Pos, Program, Retry, SourceFileId, SourceSpan};

use crate::EngineError;

/// A setting written as a list, and the layer that set it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Named {
    pub names: Vec<String>,
    pub origin: String,
}

/// What the layers set; `None` leaves the project's config in force.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// `profile`: the active profiles, in order.
    pub profiles: Option<Named>,
    /// `defaults.concurrency`.
    pub concurrency: Option<u32>,
    /// `defaults.perHost`.
    pub per_host: Option<u32>,
    /// `defaults.timeout`.
    pub timeout: Option<Duration>,
    /// `defaults.retry.attempts`: further attempts of a failed test instance.
    pub retry_attempts: Option<u32>,
    /// `defaults.retry.delay`: the pause between them.
    pub retry_delay: Option<Duration>,
    /// `reports.dir`, absolute or relative to the root.
    pub reports_dir: Option<String>,
    /// `reports.formats`.
    pub formats: Option<Named>,
    /// `reports.html.layout`: whether the HTML report is one file (`single`) or a folder.
    pub html_single_file: Option<bool>,
    /// `vars.<name>`, in the order set.
    pub vars: Vec<(String, String)>,
}

/// How one key changes the settings, or why its value does not fit.
type Apply = fn(&mut Settings, &ConfigEntry) -> Result<(), String>;

/// Every key a layer may set, but the `vars.` ones.
pub const KEYS: &[(&str, Apply)] = &[
    ("profile", profile),
    ("defaults.concurrency", concurrency),
    ("defaults.perHost", per_host),
    ("defaults.timeout", timeout),
    ("defaults.retry.attempts", retry_attempts),
    ("defaults.retry.delay", retry_delay),
    ("reports.dir", reports_dir),
    ("reports.formats", formats),
    ("reports.html.layout", html_layout),
];

/// The prefix of the keys that set config vars.
const VARS: &str = "vars.";

impl Settings {
    /// `program` with the `defaults` these settings set in place of its config's: the timeout,
    /// and the retry's attempts and delay, each over the config's own.
    pub fn apply(&self, program: &mut Arc<Program>) {
        if self.timeout.is_none() && self.retry_attempts.is_none() && self.retry_delay.is_none() {
            return;
        }
        let defaults = &mut Arc::make_mut(program).config.defaults;
        defaults.timeout = self.timeout.or(defaults.timeout);
        if self.retry_attempts.is_some() || self.retry_delay.is_some() {
            let written = defaults.retry.unwrap_or(Retry { attempts: 0, delay: Duration::new(StdDuration::ZERO) });
            let attempts = self.retry_attempts.unwrap_or(written.attempts);
            defaults.retry = Some(Retry { attempts, delay: self.retry_delay.unwrap_or(written.delay) });
        }
    }

    /// The settings `layers` give, lowest first, with `flags` (the run's own flags, such as
    /// `--profile`) between the last two.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] when a layer cannot be read, or sets an unknown key or a value
    /// that does not fit.
    #[cfg(test)]
    pub fn resolve(layers: &[&dyn ConfigSource], flags: &[ConfigEntry]) -> Result<Self, EngineError> {
        Self::applied(&ordered(layers, flags)?)
    }

    /// The settings `entries` give, in order, later ones winning.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] for an unknown key or a value that does not fit.
    pub fn applied(entries: &[ConfigEntry]) -> Result<Self, EngineError> {
        let mut settings = Self::default();
        settings.apply_all(entries)?;
        Ok(settings)
    }

    fn apply_all(&mut self, entries: &[ConfigEntry]) -> Result<(), EngineError> {
        entries.iter().try_for_each(|entry| self.apply(entry))
    }

    fn apply(&mut self, entry: &ConfigEntry) -> Result<(), EngineError> {
        let setting = |reason: String, help: Option<String>| EngineError::Setting {
            origin: entry.origin.clone(),
            reason: format!("`{}`: {reason}", entry.key),
            help,
        };
        if let Some(name) = entry.key.strip_prefix(VARS).filter(|name| !name.is_empty()) {
            self.vars.push((name.to_owned(), entry.value.clone()));
            return Ok(());
        }
        let Some((_, apply)) = KEYS.iter().find(|(key, _)| *key == entry.key) else {
            let known: Vec<&str> = KEYS.iter().map(|(key, _)| *key).collect();
            let help = format!("the settings are {} and vars.<name>", known.join(", "));
            return Err(setting("not a setting".to_owned(), Some(help)));
        };
        apply(self, entry).map_err(|reason| setting(reason, None))
    }
}

/// Every entry `layers` set, lowest first, with `flags` between the last two layers: the order
/// later entries win in.
///
/// # Errors
///
/// [`EngineError::Setting`] when a layer cannot be read.
pub fn ordered(layers: &[&dyn ConfigSource], flags: &[ConfigEntry]) -> Result<Vec<ConfigEntry>, EngineError> {
    let (top, below) = layers.split_last().map_or((None, layers), |(top, below)| (Some(*top), below));
    let mut ordered = Vec::new();
    for layer in below {
        ordered.extend(entries(*layer)?);
    }
    ordered.extend_from_slice(flags);
    if let Some(top) = top {
        ordered.extend(entries(top)?);
    }
    Ok(ordered)
}

fn entries(layer: &dyn ConfigSource) -> Result<Vec<ConfigEntry>, EngineError> {
    layer.entries().map_err(|error| source_error(&error))
}

/// The error for a config layer that cannot be read, naming the layer.
pub fn source_error(error: &ConfigSourceError) -> EngineError {
    let (origin, reason) = match error {
        ConfigSourceError::Unreadable { origin, reason } => (origin.clone(), format!("cannot read it: {reason}")),
        ConfigSourceError::Malformed { origin, entry } => (origin.clone(), format!("`{entry}` is not key=value")),
        _ => ("config".to_owned(), error.to_string()),
    };
    EngineError::Setting { origin, reason, help: None }
}

#[expect(clippy::unnecessary_wraps, reason = "every key has the `Apply` signature")]
fn profile(settings: &mut Settings, entry: &ConfigEntry) -> Result<(), String> {
    settings.profiles = Some(named(entry));
    Ok(())
}

fn concurrency(settings: &mut Settings, entry: &ConfigEntry) -> Result<(), String> {
    settings.concurrency = Some(count(&entry.value)?);
    Ok(())
}

fn per_host(settings: &mut Settings, entry: &ConfigEntry) -> Result<(), String> {
    settings.per_host = Some(count(&entry.value)?);
    Ok(())
}

fn timeout(settings: &mut Settings, entry: &ConfigEntry) -> Result<(), String> {
    settings.timeout = Some(duration(&entry.value)?);
    Ok(())
}

fn retry_attempts(settings: &mut Settings, entry: &ConfigEntry) -> Result<(), String> {
    let attempts = entry.value.trim().parse::<u32>().map_err(|_| format!("`{}` is not a whole number", entry.value))?;
    settings.retry_attempts = Some(attempts);
    Ok(())
}

fn retry_delay(settings: &mut Settings, entry: &ConfigEntry) -> Result<(), String> {
    settings.retry_delay = Some(duration(&entry.value)?);
    Ok(())
}

fn reports_dir(settings: &mut Settings, entry: &ConfigEntry) -> Result<(), String> {
    if entry.value.trim().is_empty() {
        return Err("the folder is empty".to_owned());
    }
    settings.reports_dir = Some(entry.value.trim().to_owned());
    Ok(())
}

#[expect(clippy::unnecessary_wraps, reason = "every key has the `Apply` signature")]
fn formats(settings: &mut Settings, entry: &ConfigEntry) -> Result<(), String> {
    settings.formats = Some(named(entry));
    Ok(())
}

fn html_layout(settings: &mut Settings, entry: &ConfigEntry) -> Result<(), String> {
    let single = match entry.value.trim() {
        "single" => true,
        "folder" => false,
        other => return Err(format!("`{other}` is not `folder` or `single`")),
    };
    settings.html_single_file = Some(single);
    Ok(())
}

/// The comma list `entry` sets, blanks dropped.
fn named(entry: &ConfigEntry) -> Named {
    let names = entry.value.split(',').map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).collect();
    Named { names, origin: entry.origin.clone() }
}

fn duration(value: &str) -> Result<Duration, String> {
    let nowhere = SourceSpan::new(SourceFileId::new(0), Pos::START, Pos::START);
    vc_template::parse_duration(value.trim(), nowhere)
        .map_err(|_| format!("`{value}` is not a duration such as `250ms`, `2s` or `1m`"))
}

fn count(value: &str) -> Result<u32, String> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|count| *count > 0)
        .ok_or_else(|| format!("`{value}` is not a whole number above 0"))
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
