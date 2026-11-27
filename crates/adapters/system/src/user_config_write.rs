//! The user config's text, and its guarded write (D711): the file at the path the engine fixed,
//! replaced only while it still hashes to what a caller read, as a project file's save is. A
//! link there (a dotfiles folder, say) is followed, so the file it leads to is written and the
//! link is kept.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use vc_interfaces::{ConfigSourceError, content_hash};
use vc_model::ContentHash;

use super::UserConfig;
use crate::sources::write::{stage, temp_beside};

impl UserConfig {
    /// The text of the file at `path`; `None` when there is none.
    ///
    /// # Errors
    ///
    /// [`ConfigSourceError::Unreadable`] when it is there but cannot be read as text.
    pub fn text(path: &Path) -> Result<Option<String>, ConfigSourceError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(unreadable(path, &error)),
        }
    }

    /// Replaces the file at `path` with `text`, or creates it with its folders, only while it
    /// hashes to `was`, or, for `None`, is not there. The hash is checked before anything is
    /// created and again just before the staged text is renamed over the file; no lock is taken.
    ///
    /// # Errors
    ///
    /// [`ConfigSourceError::Changed`] when the file is not the one read, and nothing is written;
    /// [`ConfigSourceError::Unwritable`] when the system refuses.
    pub fn write_unchanged(path: &Path, text: &str, was: Option<ContentHash>) -> Result<(), ConfigSourceError> {
        let target = followed(path);
        unchanged(&target, was)?;
        let temp = temp_beside(&target);
        let written = stage(&target, &temp, text)
            .map_err(|error| unwritable(path, &error))
            .and_then(|()| unchanged(&target, was))
            .and_then(|()| std::fs::rename(&temp, &target).map_err(|error| unwritable(path, &error)));
        if written.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        written
    }
}

/// What a link at `path` leads to; `path` itself when there is no link or nothing there.
fn followed(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Refuses unless the file at `target` hashes to `was`, or, for `None`, is not there.
fn unchanged(target: &Path, was: Option<ContentHash>) -> Result<(), ConfigSourceError> {
    let now = match std::fs::read(target) {
        Ok(bytes) => Some(content_hash(&bytes)),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => return Err(unreadable(target, &error)),
    };
    if now == was { Ok(()) } else { Err(ConfigSourceError::Changed { origin: target.display().to_string() }) }
}

fn unreadable(path: &Path, error: &std::io::Error) -> ConfigSourceError {
    ConfigSourceError::Unreadable { origin: path.display().to_string(), reason: error.to_string() }
}

fn unwritable(path: &Path, error: &std::io::Error) -> ConfigSourceError {
    ConfigSourceError::Unwritable { origin: path.display().to_string(), reason: error.to_string() }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::{ConfigSourceError, content_hash};

    use super::UserConfig;

    #[test]
    fn a_new_file_is_created_with_its_folders_then_replaced_only_while_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vibranium/config.yaml");
        assert_eq!(UserConfig::text(&path).unwrap(), None);
        UserConfig::write_unchanged(&path, "defaults:\n  timeout: 12s\n", None).unwrap();
        let first = UserConfig::text(&path).unwrap().unwrap();
        assert_eq!(first, "defaults:\n  timeout: 12s\n");
        let again = UserConfig::write_unchanged(&path, "profile: dev\n", None).unwrap_err();
        assert!(matches!(again, ConfigSourceError::Changed { .. }), "it is there now: {again}");
        let stale = UserConfig::write_unchanged(&path, "profile: dev\n", Some(content_hash(b"other"))).unwrap_err();
        assert!(matches!(stale, ConfigSourceError::Changed { .. }), "{stale}");
        UserConfig::write_unchanged(&path, "profile: dev\n", Some(content_hash(first.as_bytes()))).unwrap();
        assert_eq!(UserConfig::text(&path).unwrap().as_deref(), Some("profile: dev\n"));
        let left: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().flatten().collect();
        assert_eq!(left.len(), 1, "no staged file is left: {left:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_kept_and_the_file_it_leads_to_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("dotfiles.yaml");
        std::fs::write(&real, "profile: dev\n").unwrap();
        let link = dir.path().join("config.yaml");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        UserConfig::write_unchanged(&link, "profile: qa\n", Some(content_hash(b"profile: dev\n"))).unwrap();
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "profile: qa\n");
    }

    #[test]
    fn a_folder_is_not_a_file_to_read() {
        let dir = tempfile::tempdir().unwrap();
        let error = UserConfig::text(dir.path()).unwrap_err();
        assert!(matches!(error, ConfigSourceError::Unreadable { .. }), "{error}");
    }
}
