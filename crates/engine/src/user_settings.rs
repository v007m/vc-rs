//! The user config through the web UI's Settings (D129's user layer, D711, D712): read, and
//! changed one key at a time, at the one path the engine fixes (the builder's, else
//! `$XDG_CONFIG_HOME`, `$HOME/.config` or `%APPDATA%`), never one a client names. A change is a
//! preview with the file's digest, written only with that digest and only while the file still
//! hashes to it. Only the settings `--set` takes may be changed (not `plugins.allowUnconfined`),
//! each value is checked as `--set` checks it, a `reports.dir` must be a folder inside the
//! project (V5132) and a `profile` one of the project's. A refusal never echoes a client's value.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use vc_adapter_system::{SystemEnvironment, UserConfig};
use vc_executor::MASKED;
use vc_interfaces::{ConfigEntry, ConfigSource, ConfigSourceError, Environment, content_hash};

use crate::config_values::Masker;
use crate::editor_files::hex;
use crate::settings::{KEYS, Named, Settings};
use crate::user::UnderProject;
use crate::user_settings_text::{Edit, edited};
use crate::{Engine, EngineError, plan};

/// The digest of a user config that is not there yet.
pub const NEW_FILE: &str = "new";

/// How a refusal names the layer.
const ORIGIN: &str = "the user config";

/// What a value of each key looks like, for a refusal's help.
const SHAPES: &[(&str, &str)] = &[
    ("profile", "a profile ID of this project, or several joined with `,`"),
    ("defaults.concurrency", "a whole number above 0"),
    ("defaults.perHost", "a whole number above 0"),
    ("defaults.timeout", "a duration such as `250ms`, `2s` or `1m`"),
    ("defaults.retry.attempts", "a whole number"),
    ("defaults.retry.delay", "a duration such as `250ms`, `2s` or `1m`"),
    ("reports.dir", "a folder inside this project, written as plain names"),
    ("reports.formats", "report formats joined with `,`, such as `ctrf,html`"),
    ("reports.html.layout", "`folder` or `single`"),
];

/// The user config as Settings shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct UserSettings {
    /// Where it is, with the home folder as `~`.
    pub path: String,
    /// The file's digest, which a change gives back; [`NEW_FILE`] when there is no file.
    pub digest: String,
    /// Its settings in file order, values masked as a run's are.
    pub entries: Vec<UserSetting>,
}

/// One setting of the user config.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct UserSetting {
    /// The dotted key, such as `defaults.timeout` or `vars.userId`.
    pub key: String,
    /// Its value as written, masked; a list joined with `,`.
    pub value: String,
    /// Whether this project's config sets the key, so the value does not count here.
    pub shadowed: bool,
}

/// One change to the user config: a key set to a value, or removed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps from their own requests; every user is in this repo (D160)"
)]
pub struct UserSettingEdit {
    /// A key `--set` takes: `profile`, `defaults.*`, `reports.*` or `vars.<name>`.
    pub key: String,
    /// The value, as `--set` writes it (a list joined with `,`); `None` removes the key.
    pub value: Option<String>,
}

/// A change to the user config, previewed or written.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct UserSettingsChange {
    /// Where it is, with the home folder as `~`.
    pub path: String,
    /// The text before, masked; `None` when there was no file.
    pub before: Option<String>,
    /// The text after, masked.
    pub after: String,
    /// The digest to write with: the file's when previewed, the new one once written.
    pub digest: String,
}

impl Engine {
    /// The user config, its values masked.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] when there is no home folder to find it in, or it cannot be read.
    pub async fn user_settings(&self) -> Result<UserSettings, EngineError> {
        let (path, shown) = self.user_file()?;
        let text = UserConfig::text(&path).map_err(|error| unreadable(&error, &shown))?;
        let digest = digest(text.as_deref());
        let parsed = UserConfig::parse(text.as_deref().unwrap_or_default(), ORIGIN)
            .map_err(|error| unreadable(&error, &shown))?;
        let entries = parsed.entries().map_err(|error| unreadable(&error, &shown))?;
        let kept = self.program().ok().map(|(program, _)| {
            let under = UnderProject::new(&parsed, &program);
            under.entries().unwrap_or_default().into_iter().map(|entry| entry.key).collect::<Vec<_>>()
        });
        let masker = self.text_masker().await;
        let mut shown_entries = Vec::with_capacity(entries.len());
        for entry in entries {
            let shadowed = kept.as_ref().is_some_and(|kept| !kept.contains(&entry.key));
            let value = masked(masker.as_ref(), &entry.value).await;
            shown_entries.push(UserSetting { key: entry.key, value, shadowed });
        }
        Ok(UserSettings { path: shown, digest, entries: shown_entries })
    }

    /// `edits` made to the user config: previewed, or written when `write` gives the digest
    /// the preview gave.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] for a key the user config does not take, a value that does not
    /// fit, or a file that cannot be read or written; [`EngineError::Reporter`] (V5132) for a
    /// `reports.dir` outside the project; [`EngineError::Stale`] when the file changed since the
    /// preview.
    pub async fn user_settings_write(
        &self,
        edits: &[UserSettingEdit],
        write: Option<&str>,
    ) -> Result<UserSettingsChange, EngineError> {
        let (path, shown) = self.user_file()?;
        edits.iter().enumerate().try_for_each(|(at, edit)| self.fits(at + 1, edit))?;
        let before = UserConfig::text(&path).map_err(|error| unreadable(&error, &shown))?;
        let changes: Vec<Edit<'_>> = edits.iter().map(|edit| (edit.key.as_str(), edit.value.as_deref())).collect();
        let after = edited(before.as_deref(), &changes)?;
        let mut digest = digest(before.as_deref());
        if let Some(expected) = write {
            digest = written((&path, &shown), (before.as_deref(), &after), (expected, &digest))?;
        }
        let masker = self.text_masker().await;
        let before = match before {
            Some(text) => Some(masked(masker.as_ref(), &text).await),
            None => None,
        };
        Ok(UserSettingsChange { path: shown, before, after: masked(masker.as_ref(), &after).await, digest })
    }

    /// What masks text as a run's values are; `None` when the project cannot be layered, so
    /// every value is masked whole.
    async fn text_masker(&self) -> Option<Masker> {
        let mut layered = self.layered(&[], &[]).ok()?;
        Some(self.masker(&mut layered).await)
    }

    /// The user config's path, and how it is shown.
    fn user_file(&self) -> Result<(PathBuf, String), EngineError> {
        let path = self.user_config_path(self.process().as_ref()).ok_or_else(|| EngineError::Setting {
            origin: ORIGIN.to_owned(),
            reason: "there is no home folder to keep it in".to_owned(),
            help: Some("set HOME (or XDG_CONFIG_HOME, or APPDATA on Windows)".to_owned()),
        })?;
        let shown = self.shown_path(&path);
        Ok((path, shown))
    }

    /// `path` with the home folder as `~`, for a client to show.
    pub(crate) fn shown_path(&self, path: &Path) -> String {
        home_shown(path, self.process().as_ref())
    }

    /// The process environment: the builder's, else this process's.
    fn process(&self) -> Arc<dyn Environment> {
        self.environment.clone().unwrap_or_else(|| Arc::new(SystemEnvironment::capture()))
    }

    /// Refuses edit `at` unless its key is a setting and its value fits it.
    fn fits(&self, at: usize, edit: &UserSettingEdit) -> Result<(), EngineError> {
        let var = edit.key.strip_prefix("vars.");
        if let Some(name) = var {
            return if var_name(name) { Ok(()) } else { Err(not_a_key(at)) };
        }
        let Some((key, _)) = KEYS.iter().find(|(key, _)| *key == edit.key) else { return Err(not_a_key(at)) };
        let Some(value) = &edit.value else { return Ok(()) };
        let entry = ConfigEntry::new(*key, value, ORIGIN);
        let shape = SHAPES.iter().find(|(name, _)| name == key).map_or("", |(_, shape)| shape);
        let misfit = || EngineError::Setting {
            origin: ORIGIN.to_owned(),
            reason: format!("edit {at}: `{key}` does not take that value"),
            help: Some(format!("`{key}` takes {shape}")),
        };
        Settings::applied(std::slice::from_ref(&entry)).map_err(|_| misfit())?;
        match *key {
            "reports.dir" => self.client_sets(&[format!("reports.dir={value}")]),
            "profile" => self.known_profiles(value).map_err(|_| misfit()),
            _ => Ok(()),
        }
    }

    /// Refuses unless each profile `names` lists is one of the project's.
    fn known_profiles(&self, names: &str) -> Result<(), EngineError> {
        let (program, _) = self.program()?;
        let names = names.split(',').map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).collect();
        plan::profiles(&program, Some(&Named { names, origin: ORIGIN.to_owned() })).map(|_| ())
    }
}

/// `text` masked by `masker`, or whole without one.
async fn masked(masker: Option<&Masker>, text: &str) -> String {
    match masker {
        Some(masker) => masker.text(text).await,
        None => MASKED.to_owned(),
    }
}

/// Writes `after` over `before` at `path` (shown as `shown`) once `expected` is the file's
/// digest; the new digest.
fn written(
    (path, shown): (&Path, &str),
    (before, after): (Option<&str>, &str),
    (expected, digest): (&str, &str),
) -> Result<String, EngineError> {
    let stale = || EngineError::Stale { reason: format!("{shown}: nothing was written: it changed since the preview") };
    if expected != digest {
        return Err(stale());
    }
    let was = before.map(|text| content_hash(text.as_bytes()));
    UserConfig::write_unchanged(path, after, was).map_err(|error| match error {
        ConfigSourceError::Changed { .. } => stale(),
        other => unreadable(&other, shown),
    })?;
    Ok(digest_of(after))
}

/// The digest of the text `text`; [`NEW_FILE`] for no file.
fn digest(text: Option<&str>) -> String {
    text.map_or_else(|| NEW_FILE.to_owned(), digest_of)
}

fn digest_of(text: &str) -> String {
    hex(content_hash(text.as_bytes()).as_bytes())
}

/// `path` with the home folder `environment` names as `~`.
fn home_shown(path: &Path, environment: &dyn Environment) -> String {
    let home = environment.var("HOME").filter(|home| !home.is_empty());
    home.and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf))
        .map_or_else(|| path.display().to_string(), |rest| format!("~/{}", rest.display()))
}

/// Whether `name` is a var name: a letter or `_`, then letters, digits and `_`.
fn var_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|next| next.is_ascii_alphanumeric() || next == '_')
}

fn not_a_key(at: usize) -> EngineError {
    let keys: Vec<&str> = KEYS.iter().map(|(key, _)| *key).collect();
    EngineError::Setting {
        origin: ORIGIN.to_owned(),
        reason: format!("edit {at}: that key is not one the user config takes"),
        help: Some(format!("the keys are {} and vars.<name>", keys.join(", "))),
    }
}

/// `error` reading or writing the user config, naming it as `shown`.
fn unreadable(error: &ConfigSourceError, shown: &str) -> EngineError {
    let reason = match error {
        ConfigSourceError::Unreadable { reason, .. } => format!("cannot read it: {reason}"),
        ConfigSourceError::Unwritable { reason, .. } => format!("cannot write it: {reason}"),
        other => other.to_string(),
    };
    EngineError::Setting { origin: shown.to_owned(), reason, help: None }
}

#[cfg(test)]
#[path = "user_settings_tests.rs"]
mod tests;
