//! The run settings from the config layers above the project's files (D129): `.env`, the
//! `VIB_*` variables, the run's flags and `--set`, later layers winning.

use vc_interfaces::{ConfigEntry, ConfigSource, ConfigSourceError};
use vc_model::{Duration, Pos, SourceFileId, SourceSpan};

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
    /// `reports.dir`, absolute or relative to the root.
    pub reports_dir: Option<String>,
    /// `reports.formats`.
    pub formats: Option<Named>,
    /// `vars.<name>`, in the order set.
    pub vars: Vec<(String, String)>,
}

/// How one key changes the settings, or why its value does not fit.
type Apply = fn(&mut Settings, &ConfigEntry) -> Result<(), String>;

/// Every key a layer may set, but the `vars.` ones.
const KEYS: &[(&str, Apply)] = &[
    ("profile", profile),
    ("defaults.concurrency", concurrency),
    ("defaults.perHost", per_host),
    ("defaults.timeout", timeout),
    ("reports.dir", reports_dir),
    ("reports.formats", formats),
];

/// The prefix of the keys that set config vars.
const VARS: &str = "vars.";

impl Settings {
    /// The settings `layers` give, lowest first, with `flags` (the run's own flags, such as
    /// `--profile`) between the last two.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] when a layer cannot be read, or sets an unknown key or a value
    /// that does not fit.
    pub fn resolve(layers: &[&dyn ConfigSource], flags: &[ConfigEntry]) -> Result<Self, EngineError> {
        let mut settings = Self::default();
        let (top, below) = layers.split_last().map_or((None, layers), |(top, below)| (Some(*top), below));
        for layer in below {
            settings.apply_all(&entries(*layer)?)?;
        }
        settings.apply_all(flags)?;
        if let Some(top) = top {
            settings.apply_all(&entries(top)?)?;
        }
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
    let nowhere = SourceSpan::new(SourceFileId::new(0), Pos::START, Pos::START);
    let parsed = vc_template::parse_duration(entry.value.trim(), nowhere)
        .map_err(|_| format!("`{}` is not a duration such as `250ms`, `2s` or `1m`", entry.value))?;
    settings.timeout = Some(parsed);
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

/// The comma list `entry` sets, blanks dropped.
fn named(entry: &ConfigEntry) -> Named {
    let names = entry.value.split(',').map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).collect();
    Named { names, origin: entry.origin.clone() }
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
