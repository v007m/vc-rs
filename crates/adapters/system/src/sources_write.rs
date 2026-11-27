//! [`FsSources`] as a `SourceWriter`: writes, guarded saves and removals, only inside the root.

use std::path::{Path, PathBuf};

use vc_interfaces::{SourceError, SourceWriter, content_hash};
use vc_model::{ContentHash, SourcePath};

use super::FsSources;

impl SourceWriter for FsSources {
    /// Writes only inside the root: a folder or file that is a link out of it is refused before
    /// anything is created.
    fn write(&self, path: &SourcePath, text: &str) -> Result<(), SourceError> {
        let target = self.inside(path)?;
        if let Some(folder) = target.parent() {
            std::fs::create_dir_all(folder).map_err(|error| write_error(path, &error))?;
        }
        std::fs::write(&target, text).map_err(|error| write_error(path, &error))
    }

    /// Checks the hash of the file at `path` (of what a link there leads to, inside the root)
    /// against `was` before anything is created, then writes the text to a hidden file beside
    /// it, checks the hash again, and renames the hidden file over it, which replaces it at once
    /// on Unix and Windows. A refused save creates no folder, and the hidden file is removed
    /// when anything after it fails.
    fn write_unchanged(&self, path: &SourcePath, text: &str, was: Option<ContentHash>) -> Result<(), SourceError> {
        let target = self.resolved(path)?;
        unchanged(path, &target, was)?;
        let temp = temp_beside(&target);
        let written = stage(&target, &temp, text)
            .map_err(|error| write_error(path, &error))
            .and_then(|()| unchanged(path, &target, was))
            .and_then(|()| std::fs::rename(&temp, &target).map_err(|error| write_error(path, &error)));
        if written.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        written
    }

    /// Removes only inside the root, as [`FsSources::write`] writes.
    fn remove(&self, path: &SourcePath) -> Result<(), SourceError> {
        let target = self.inside(path)?;
        std::fs::remove_file(&target).map_err(|error| write_error(path, &error))
    }

    /// Sends the file, or the link, at `path` to the bin, the system's trash unless another was
    /// given, once it is known to be inside the root and unchanged; a file the bin does not take
    /// is left where it is, never deleted.
    fn trash(&self, path: &SourcePath, was: ContentHash) -> Result<String, SourceError> {
        let target = self.inside(path)?;
        unchanged(path, &target, Some(was))?;
        self.bin.put(&target).map_err(|_| SourceError::Untrashed { path: path.to_string() })
    }
}

impl FsSources {
    /// Where `path` is on disk, once the nearest part of it that exists is known to resolve
    /// inside the root.
    fn inside(&self, path: &SourcePath) -> Result<PathBuf, SourceError> {
        let target = self.root.join(path.as_str());
        let existing = target.ancestors().find(|at| at.symlink_metadata().is_ok()).unwrap_or(&self.root);
        let resolved = std::fs::canonicalize(existing).map_err(|error| write_error(path, &error))?;
        if !self.leads_in(path, &resolved) {
            return Err(SourceError::OutsideRoot { path: path.to_string() });
        }
        Ok(target)
    }

    /// Where `path` is on disk, as [`FsSources::inside`] finds it, with a link at its end
    /// followed once its target is known to be inside the root.
    fn resolved(&self, path: &SourcePath) -> Result<PathBuf, SourceError> {
        let target = self.inside(path)?;
        match std::fs::canonicalize(&target) {
            Ok(real) if self.leads_in(path, &real) => Ok(real),
            Ok(_) => Err(SourceError::OutsideRoot { path: path.to_string() }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(target),
            Err(error) => Err(write_error(path, &error)),
        }
    }
}

/// A hidden file beside `target` for one write to stage its text in, named after the file, the
/// process, the thread and the time, so two writes never share one.
pub fn temp_beside(target: &Path) -> PathBuf {
    let name = target.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let thread: String = format!("{:?}", std::thread::current().id()).chars().filter(char::is_ascii_digit).collect();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    target.with_file_name(format!(".{name}.{}-{thread}-{nanos}.vc-tmp", std::process::id()))
}

/// Writes `text` to `temp`, a new file, with the folders it needs and the permissions of
/// `target` when it exists; a read-only `target` is refused, as a plain write refuses it.
pub fn stage(target: &Path, temp: &Path, text: &str) -> std::io::Result<()> {
    let kept = std::fs::metadata(target).ok().map(|found| found.permissions());
    if kept.as_ref().is_some_and(std::fs::Permissions::readonly) {
        return Err(std::io::ErrorKind::PermissionDenied.into());
    }
    if let Some(folder) = target.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(temp)?;
    std::io::Write::write_all(&mut file, text.as_bytes())?;
    if let Some(kept) = kept {
        file.set_permissions(kept)?;
    }
    file.sync_all()
}

/// Refuses with [`SourceError::Changed`] unless the file at `target` hashes to `was`, or, for
/// `None`, is not there.
fn unchanged(path: &SourcePath, target: &Path, was: Option<ContentHash>) -> Result<(), SourceError> {
    let now = match std::fs::read(target) {
        Ok(bytes) => Some(content_hash(&bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(write_error(path, &error)),
    };
    if now == was { Ok(()) } else { Err(SourceError::Changed { path: path.to_string() }) }
}

fn write_error(path: &SourcePath, error: &std::io::Error) -> SourceError {
    SourceError::Write { path: path.to_string(), reason: error.to_string() }
}

#[cfg(test)]
#[path = "sources_write_tests.rs"]
mod tests;
