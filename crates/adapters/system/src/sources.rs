use std::path::{Path, PathBuf};

use ignore::{DirEntry, WalkBuilder};
use vc_interfaces::{Discovery, SourceError, SourceFilter, SourceProvider, SourceWriter, content_hash};
use vc_model::{ContentHash, SourcePath, hidden_part};

/// Reads a project's definition files from disk, and lists the other files as assets; reads
/// any file by path on request.
///
/// Walks the root with `ignore`, so `.gitignore` (and `.ignore`) files are respected even
/// outside a git repository; the user's global git excludes are not, so every machine finds
/// the same files. Folders the filter excludes are not entered. Links to folders are not
/// followed; a link to a file is read only when its target is inside the root and, unless the
/// link's own path is hidden, outside every hidden file and folder.
#[derive(Clone, Debug)]
pub struct FsSources {
    root: PathBuf,
}

impl FsSources {
    /// A provider for the project at `root`.
    ///
    /// # Errors
    ///
    /// [`SourceError::Root`] when `root` cannot be resolved.
    pub fn new(root: &Path) -> Result<Self, SourceError> {
        let root = std::fs::canonicalize(root).map_err(|error| root_error(root, &error))?;
        Ok(Self { root })
    }

    /// The project root, with links resolved.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn walk(&self, filter: &SourceFilter) -> ignore::Walk {
        let (root, dirs) = (self.root.clone(), filter.clone());
        let mut walk = WalkBuilder::new(&self.root);
        walk.hidden(false).git_global(false).require_git(false).follow_links(false);
        walk.filter_entry(move |entry| relative(&root, entry.path()).is_none_or(|(text, _)| !dirs.skips(&text)));
        walk.build()
    }

    fn visit(&self, entry: &DirEntry, filter: &SourceFilter, found: &mut Discovery) {
        let Some(kind) = entry.file_type() else { return };
        if kind.is_dir() {
            return;
        }
        let Some((text, utf8)) = relative(&self.root, entry.path()) else { return };
        match filter.select(&text) {
            Some(Ok(path)) if utf8 => match self.load(path, entry.path()) {
                Ok(file) => found.files.push(file),
                Err(problem) => found.problems.push(problem),
            },
            Some(Ok(path)) => found.problems.push(SourceError::InvalidPath { path: path.to_string() }),
            Some(Err(problem)) => found.problems.push(problem),
            None if utf8 => found.assets.extend(filter.asset(&text)),
            None => {}
        }
    }

    /// Reads the file through its resolved path, so a link cannot lead out of the root.
    fn load(&self, path: SourcePath, on_disk: &Path) -> Result<vc_model::SourceFile, SourceError> {
        let failed = |error: std::io::Error| match error.kind() {
            std::io::ErrorKind::NotFound => SourceError::Missing { path: path.to_string() },
            _ => SourceError::Read { path: path.to_string(), reason: error.to_string() },
        };
        let target = std::fs::canonicalize(on_disk).map_err(failed)?;
        if !self.leads_in(&path, &target) {
            return Err(SourceError::OutsideRoot { path: path.to_string() });
        }
        if target.is_dir() {
            return Err(SourceError::Read { path: path.to_string(), reason: "it is a folder, not a file".to_owned() });
        }
        let bytes = std::fs::read(&target).map_err(failed)?;
        vc_interfaces::source_file(path, bytes)
    }
}

impl SourceProvider for FsSources {
    fn discover(&self, filter: &SourceFilter) -> Result<Discovery, SourceError> {
        if let Err(error) = std::fs::read_dir(&self.root) {
            return Err(root_error(&self.root, &error));
        }
        let mut found = Discovery::default();
        for entry in self.walk(filter) {
            match entry {
                Ok(entry) => self.visit(&entry, filter, &mut found),
                Err(error) => found.problems.push(self.walk_problem(&error)),
            }
        }
        found.sort();
        Ok(found)
    }

    fn read(&self, paths: &[SourcePath]) -> Result<Discovery, SourceError> {
        if let Err(error) = std::fs::read_dir(&self.root) {
            return Err(root_error(&self.root, &error));
        }
        let mut found = Discovery::default();
        for path in unique(paths) {
            match self.load(path.clone(), &self.root.join(path.as_str())) {
                Ok(file) => found.files.push(file),
                Err(problem) => found.problems.push(problem),
            }
        }
        found.sort();
        Ok(found)
    }
}

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

    /// Writes the text to a hidden file beside the file at `path` (beside what a link there
    /// leads to, inside the root), checks that file's hash against `was`, and renames the hidden
    /// file over it, which replaces it at once on Unix and Windows. The hidden file is removed
    /// when the check or the rename fails.
    fn write_unchanged(&self, path: &SourcePath, text: &str, was: Option<ContentHash>) -> Result<(), SourceError> {
        let target = self.resolved(path)?;
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

    /// Whether `resolved`, where `path` leads on disk, is inside the root and, unless `path`
    /// itself names a hidden file or folder, outside every hidden one: a link named
    /// `x.test.yaml` cannot open `.env`.
    fn leads_in(&self, path: &SourcePath, resolved: &Path) -> bool {
        let Ok(rest) = resolved.strip_prefix(&self.root) else { return false };
        let hidden = rest.components().any(|part| hidden_part(&part.as_os_str().to_string_lossy()));
        !hidden || path.is_hidden()
    }

    fn walk_problem(&self, error: &ignore::Error) -> SourceError {
        let path = error_path(error).and_then(|path| relative(&self.root, path)).map(|(text, _)| text);
        SourceError::Read { path: path.unwrap_or_else(|| ".".to_owned()), reason: error.to_string() }
    }
}

/// `path` relative to `root` with `/` separators, and whether every part was UTF-8 (parts that
/// are not are decoded lossily); `None` for the root itself or a path outside it.
fn relative(root: &Path, path: &Path) -> Option<(String, bool)> {
    let rest = path.strip_prefix(root).ok()?;
    let parts: Vec<_> = rest.components().map(std::path::Component::as_os_str).collect();
    if parts.is_empty() {
        return None;
    }
    let utf8 = parts.iter().all(|part| part.to_str().is_some());
    Some((parts.iter().map(|part| part.to_string_lossy()).collect::<Vec<_>>().join("/"), utf8))
}

/// A hidden file beside `target` for one write to stage its text in, named after the file, the
/// process, the thread and the time, so two writes never share one.
fn temp_beside(target: &Path) -> PathBuf {
    let name = target.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let thread: String = format!("{:?}", std::thread::current().id()).chars().filter(char::is_ascii_digit).collect();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    target.with_file_name(format!(".{name}.{}-{thread}-{nanos}.vc-tmp", std::process::id()))
}

/// Writes `text` to `temp`, a new file, with the folders it needs and the permissions of
/// `target` when it exists; a read-only `target` is refused, as a plain write refuses it.
fn stage(target: &Path, temp: &Path, text: &str) -> std::io::Result<()> {
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

/// `paths` in order, each once.
fn unique(paths: &[SourcePath]) -> std::collections::BTreeSet<&SourcePath> {
    paths.iter().collect()
}

fn error_path(error: &ignore::Error) -> Option<&Path> {
    match error {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => error_path(err),
        _ => None,
    }
}

fn root_error(root: &Path, error: &std::io::Error) -> SourceError {
    SourceError::Root { root: root.display().to_string(), reason: error.to_string() }
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
