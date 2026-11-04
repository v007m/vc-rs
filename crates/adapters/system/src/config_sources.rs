//! The config layers above the project's files (D129): `.env`, the `VIB_*` variables, `--set`.

use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;

use vc_interfaces::{ConfigEntry, ConfigSource, ConfigSourceError, Environment};

/// The `VIB_*` variables a run reads, and the setting each one sets.
const VIB_VARS: &[(&str, &str)] = &[("VIB_PROFILE", "profile")];

/// The file at the project root that holds the `.env` layer.
const DOT_ENV: &str = ".env";

/// The origin `--set` entries carry.
const SET_FLAG: &str = "--set";

/// The project's `.env` file.
///
/// It holds `NAME=value` lines, `#` comments, an optional `export ` and optional quotes. Its
/// `VIB_*` lines are settings; every line is also an environment variable for `${env.NAME}`,
/// under the process environment ([`DotEnv::vars`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DotEnv {
    vars: Vec<(String, String)>,
}

impl DotEnv {
    /// Reads `<root>/.env`; a project without one has an empty layer.
    ///
    /// # Errors
    ///
    /// [`ConfigSourceError::Unreadable`] when the file is a link or cannot be read, and
    /// [`ConfigSourceError::Malformed`] when a line is not `NAME=value`.
    pub fn read(root: &Path) -> Result<Self, ConfigSourceError> {
        let path = root.join(DOT_ENV);
        let unreadable = |reason: String| ConfigSourceError::Unreadable { origin: DOT_ENV.to_owned(), reason };
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(unreadable(error.to_string())),
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(unreadable("it is a link, which is not followed".to_owned()));
            }
            Ok(_) => {}
        }
        let text = std::fs::read_to_string(&path).map_err(|error| unreadable(error.to_string()))?;
        Self::parse(&text)
    }

    /// The layer written as `text`.
    ///
    /// # Errors
    ///
    /// [`ConfigSourceError::Malformed`] when a line is not `NAME=value`.
    pub fn parse(text: &str) -> Result<Self, ConfigSourceError> {
        let lines = text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#'));
        let vars = lines.map(|line| assignment(line, DOT_ENV)).collect::<Result<_, _>>()?;
        Ok(Self { vars })
    }

    /// Every variable the file sets, in the order written.
    #[must_use]
    pub fn vars(&self) -> &[(String, String)] {
        &self.vars
    }
}

impl ConfigSource for DotEnv {
    fn entries(&self) -> Result<Vec<ConfigEntry>, ConfigSourceError> {
        let setting =
            |(name, value): &(String, String)| vib_key(name).map(|key| ConfigEntry::new(key, value.clone(), DOT_ENV));
        Ok(self.vars.iter().filter_map(setting).collect())
    }
}

/// The `VIB_*` variables of the process environment, such as `VIB_PROFILE=staging`.
#[derive(Clone)]
pub struct VibVars {
    environment: Arc<dyn Environment>,
}

impl VibVars {
    /// The `VIB_*` layer read from `environment`.
    #[must_use]
    pub fn new(environment: Arc<dyn Environment>) -> Self {
        Self { environment }
    }
}

impl ConfigSource for VibVars {
    fn entries(&self) -> Result<Vec<ConfigEntry>, ConfigSourceError> {
        let set =
            |&(name, key): &(&str, &str)| self.environment.var(name).map(|value| ConfigEntry::new(key, value, name));
        Ok(VIB_VARS.iter().filter_map(set).collect())
    }
}

/// The `--set key=value` flags, in the order given: the top config layer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetFlags {
    flags: Vec<String>,
}

impl SetFlags {
    /// The layer the `flags` give, each one `key=value`.
    #[must_use]
    pub const fn new(flags: Vec<String>) -> Self {
        Self { flags }
    }
}

impl ConfigSource for SetFlags {
    fn entries(&self) -> Result<Vec<ConfigEntry>, ConfigSourceError> {
        let entry = |flag: &String| {
            let (key, value) = assignment(flag, SET_FLAG)?;
            Ok(ConfigEntry::new(key, value, SET_FLAG))
        };
        self.flags.iter().map(entry).collect()
    }
}

/// The setting a `VIB_*` variable sets, if it is one.
fn vib_key(name: &str) -> Option<&'static str> {
    VIB_VARS.iter().find(|(var, _)| *var == name).map(|(_, key)| *key)
}

/// `line` read as `NAME=value`, without an `export ` before it or quotes around the value.
fn assignment(line: &str, origin: &str) -> Result<(String, String), ConfigSourceError> {
    let malformed = || ConfigSourceError::Malformed { origin: origin.to_owned(), entry: line.to_owned() };
    let bare = line.strip_prefix("export ").unwrap_or(line);
    let (name, value) = bare.split_once('=').ok_or_else(malformed)?;
    let name = name.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return Err(malformed());
    }
    Ok((name.to_owned(), unquoted(value.trim()).to_owned()))
}

/// `value` without one pair of matching quotes around it.
fn unquoted(value: &str) -> &str {
    ['"', '\''].iter().find_map(|quote| value.strip_prefix(*quote)?.strip_suffix(*quote)).unwrap_or(value)
}

#[cfg(test)]
#[path = "config_sources_tests.rs"]
mod tests;
