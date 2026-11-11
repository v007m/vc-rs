//! The user config, `~/.config/vibranium/config.yaml`: the lowest layer of settings (D129).

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use saphyr_parser::{Event, Parser};
use vc_interfaces::{ConfigEntry, ConfigSource, ConfigSourceError, Environment};

/// The top-level keys that are settings; anything else, such as `new.comments`, is a preference
/// other commands read.
const SETTINGS: &[&str] = &["profile", "defaults", "reports", "vars"];

/// The user's own preference that lets a plugin run unconfined where it cannot be confined
/// (D260): read only here, never from the project's config, a profile or `.env`.
const ALLOW_UNCONFINED: &str = "plugins.allowUnconfined";

/// Where the user config lives, under the user's config folder.
const FILE: [&str; 2] = ["vibranium", "config.yaml"];

/// The user's own settings, shared by every project.
///
/// A YAML map read as dotted keys: `defaults: {timeout: 5s}` sets `defaults.timeout`, and a list
/// is joined with commas, so `reports: {formats: [ctrf, junit]}` sets `reports.formats` to
/// `ctrf,junit`. Only `profile`, `defaults.*`, `reports.*` and `vars.*` are settings. Each
/// entry's origin is the file's path. `plugins.allowUnconfined: true` is a preference
/// ([`UserConfig::allows_unconfined_plugins`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserConfig {
    origin: String,
    entries: Vec<(String, String)>,
    unconfined: bool,
}

impl UserConfig {
    /// The default path: `$XDG_CONFIG_HOME/vibranium/config.yaml`, else
    /// `$HOME/.config/vibranium/config.yaml` (`%APPDATA%\vibranium\config.yaml` on Windows);
    /// `None` when `environment` names no such folder.
    #[must_use]
    pub fn default_path(environment: &dyn Environment) -> Option<PathBuf> {
        let set = |name: &str| environment.var(name).filter(|value| !value.is_empty()).map(PathBuf::from);
        let folder = set("XDG_CONFIG_HOME")
            .or_else(|| set("HOME").map(|home| home.join(".config")))
            .or_else(|| set("APPDATA"))?;
        Some(FILE.iter().fold(folder, |path, part| path.join(part)))
    }

    /// Reads `path`; a missing file is an empty layer.
    ///
    /// # Errors
    ///
    /// [`ConfigSourceError::Unreadable`] when the file cannot be read or is not YAML.
    pub fn read(path: &Path) -> Result<Self, ConfigSourceError> {
        let origin = path.display().to_string();
        match std::fs::read_to_string(path) {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Self { origin, ..Self::default() }),
            Err(error) => Err(ConfigSourceError::Unreadable { origin, reason: error.to_string() }),
            Ok(text) => Self::parse(&text, origin),
        }
    }

    /// The layer written as `text`, read from `origin`.
    ///
    /// # Errors
    ///
    /// [`ConfigSourceError::Unreadable`] when `text` is not YAML.
    pub fn parse(text: &str, origin: impl Into<String>) -> Result<Self, ConfigSourceError> {
        let origin = origin.into();
        let mut flat = Flatten::default();
        let mut parser = Parser::new_from_str(text);
        while let Some(next) = parser.next_event() {
            let (event, _) = next.map_err(|error| ConfigSourceError::Unreadable {
                origin: origin.clone(),
                reason: format!("it is not YAML: {} (line {})", error.info(), error.marker().line()),
            })?;
            flat.event(event);
        }
        let unconfined = flat.entries.iter().any(|(key, value)| key == ALLOW_UNCONFINED && value == "true");
        let setting = |(key, _): &(String, String)| SETTINGS.contains(&key.split('.').next().unwrap_or_default());
        Ok(Self { origin, entries: flat.entries.into_iter().filter(setting).collect(), unconfined })
    }

    /// Whether the user lets a plugin run unconfined, with a warning, where it cannot be confined
    /// (`plugins.allowUnconfined: true`, D260).
    #[must_use]
    pub const fn allows_unconfined_plugins(&self) -> bool {
        self.unconfined
    }
}

impl ConfigSource for UserConfig {
    fn entries(&self) -> Result<Vec<ConfigEntry>, ConfigSourceError> {
        let entry = |(key, value): &(String, String)| ConfigEntry::new(key, value, &self.origin);
        Ok(self.entries.iter().map(entry).collect())
    }
}

/// One open YAML collection.
enum Frame {
    /// A map, with the key waiting for its value; `named` when opening it added to the path.
    Map { key: Option<String>, named: bool },
    /// A list under `key`, joined with commas when it closes.
    List { key: String, items: Vec<String> },
    /// A collection with no dotted key, such as a map inside a list: left out.
    Skip,
}

/// YAML events read into dotted keys and values.
#[derive(Default)]
struct Flatten {
    frames: Vec<Frame>,
    path: Vec<String>,
    entries: Vec<(String, String)>,
}

impl Flatten {
    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Scalar(text, ..) => self.scalar(text.into_owned()),
            Event::MappingStart(..) => self.open(true),
            Event::SequenceStart(..) => self.open(false),
            Event::MappingEnd | Event::SequenceEnd => self.close(),
            _ => {}
        }
    }

    fn scalar(&mut self, text: String) {
        if let Some(Frame::List { items, .. }) = self.frames.last_mut() {
            items.push(text);
            return;
        }
        let Some(Frame::Map { key, .. }) = self.frames.last_mut() else { return };
        match key.take() {
            None => *key = Some(text),
            Some(name) => {
                let full = self.dotted(&name);
                self.entries.push((full, text));
            }
        }
    }

    fn open(&mut self, map: bool) {
        let root = self.frames.is_empty();
        let key = match self.frames.last_mut() {
            Some(Frame::Map { key, .. }) => key.take(),
            _ => None,
        };
        let frame = match (key, map) {
            (Some(key), true) => {
                self.path.push(key);
                Frame::Map { key: None, named: true }
            }
            (Some(key), false) => Frame::List { key, items: Vec::new() },
            (None, true) if root => Frame::Map { key: None, named: false },
            (None, _) => Frame::Skip,
        };
        self.frames.push(frame);
    }

    fn close(&mut self) {
        match self.frames.pop() {
            Some(Frame::Map { named: true, .. }) => {
                self.path.pop();
            }
            Some(Frame::List { key, items }) => {
                let full = self.dotted(&key);
                self.entries.push((full, items.join(",")));
            }
            _ => {}
        }
    }

    fn dotted(&self, key: &str) -> String {
        self.path.iter().map(String::as_str).chain([key]).collect::<Vec<_>>().join(".")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use vc_interfaces::{ConfigEntry, ConfigSource, ConfigSourceError};
    use vc_testkit::{MemoryEnvironment, config_source_suite};

    use super::UserConfig;

    #[test]
    fn a_user_config_meets_the_config_source_contract() {
        config_source_suite(&UserConfig::parse("defaults:\n  timeout: 5s\n", "user.yaml").unwrap());
        config_source_suite(&UserConfig::default());
    }

    #[test]
    fn maps_become_dotted_keys_lists_join_and_preferences_are_left_out() {
        let text = "new:\n  comments: brief\nprofile: dev\ndefaults:\n  perHost: 2\nreports:\n  formats: [ctrf, junit]\n\
                    vars:\n  userId: 7\n  items:\n    - {a: 1}\n";
        let entries = UserConfig::parse(text, "u.yaml").unwrap().entries().unwrap();
        let expected = [
            ConfigEntry::new("profile", "dev", "u.yaml"),
            ConfigEntry::new("defaults.perHost", "2", "u.yaml"),
            ConfigEntry::new("reports.formats", "ctrf,junit", "u.yaml"),
            ConfigEntry::new("vars.userId", "7", "u.yaml"),
            ConfigEntry::new("vars.items", "", "u.yaml"),
        ];
        assert_eq!(entries, expected);
    }

    #[test]
    fn allowing_unconfined_plugins_is_a_preference_not_a_setting() {
        let allowing = UserConfig::parse("plugins:\n  allowUnconfined: true\n", "u.yaml").unwrap();
        assert!(allowing.allows_unconfined_plugins());
        assert_eq!(allowing.entries().unwrap(), []);
        let other = |text: &str| UserConfig::parse(text, "u.yaml").unwrap().allows_unconfined_plugins();
        assert!(!other("plugins:\n  allowUnconfined: false\n"));
        assert!(!other("allowUnconfined: true\n"));
        assert!(!UserConfig::default().allows_unconfined_plugins());
    }

    #[test]
    fn a_missing_file_is_empty_and_bad_yaml_is_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        assert_eq!(UserConfig::read(&path).unwrap().entries().unwrap(), []);
        std::fs::write(&path, "defaults: [unclosed\n").unwrap();
        let error = UserConfig::read(&path).unwrap_err();
        assert!(matches!(error, ConfigSourceError::Unreadable { .. }), "{error}");
        std::fs::write(&path, "vars:\n  who: user\n").unwrap();
        let origin = path.display().to_string();
        assert_eq!(UserConfig::read(&path).unwrap().entries().unwrap(), [ConfigEntry::new("vars.who", "user", origin)]);
    }

    #[test]
    fn a_missing_file_keeps_its_path_and_only_a_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let empty = UserConfig::parse("", path.display().to_string()).unwrap();
        assert_eq!(UserConfig::read(&path).unwrap(), empty, "the empty layer still names the file it would be");
        let error = UserConfig::read(dir.path()).unwrap_err();
        assert!(matches!(error, ConfigSourceError::Unreadable { .. }), "a folder is not a missing file: {error}");
    }

    #[test]
    fn the_default_path_follows_xdg_then_home_then_appdata() {
        let path = |vars: &[(&str, &str)]| UserConfig::default_path(&MemoryEnvironment::new(vars));
        let xdg = path(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/h")]);
        assert_eq!(xdg, Some(PathBuf::from("/x/vibranium/config.yaml")));
        assert_eq!(
            path(&[("XDG_CONFIG_HOME", ""), ("HOME", "/h")]),
            Some(PathBuf::from("/h/.config/vibranium/config.yaml"))
        );
        assert_eq!(path(&[("APPDATA", "/a")]), Some(PathBuf::from("/a/vibranium/config.yaml")));
        assert_eq!(path(&[]), None);
    }
}
